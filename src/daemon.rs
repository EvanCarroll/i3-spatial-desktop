//! Long-running mode. Bind keys to `nop i3-spatial-desktop <args>`: i3 does nothing
//! with them but reports each press as a `binding` event, which the daemon
//! executes in order over one persistent connection. It also re-settles
//! desktops when workspaces are created by anything else, renames a
//! desktop's satellites when its origin is renamed by anything else, and,
//! in the modes that spread a desktop over the monitors, brings the other
//! monitors along when the focus is moved by anything else.
//!
//! A control socket (see [`control_path`]) keeps one daemon per session:
//! a new daemon connects to it, which makes the old one exit.

use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::{UnixListener, UnixStream};

use clap::Parser;
use serde::Deserialize;

use crate::cli::{Cli, Cmd};
use crate::config::Config;
use crate::error::Result;
use crate::ipc::{
    self, Connection, EVENT_BINDING, EVENT_OUTPUT, EVENT_SHUTDOWN, EVENT_WORKSPACE,
};
use crate::mode::{MultiMonitorMode, Plan};
use crate::plan;
use crate::world::World;

/// What i3 reported, as far as the daemon cares.
#[derive(Debug, PartialEq, Eq)]
enum Event {
    /// A key binding ran this command.
    Binding(String),
    Created { id: u64, name: String },
    Renamed { id: u64, name: String },
    /// A workspace went away.
    Emptied { id: u64 },
    /// Another workspace has the focus.
    Focused,
    /// Monitors came, went or moved.
    Outputs,
    Shutdown,
}

/// How many times in a row following may find something to change before
/// it is taken for a fight with something else and stopped.
const RESTLESS: u32 = 5;

#[derive(Deserialize)]
struct BindingEvent {
    change: String,
    binding: Binding,
}

#[derive(Deserialize)]
struct Binding {
    command: String,
}

#[derive(Deserialize)]
struct WorkspaceEvent {
    change: String,
    current: Option<Current>,
}

#[derive(Deserialize)]
struct Current {
    id: u64,
    name: Option<String>,
}

/// Reads one message from the event connection. `None` for anything the
/// daemon has no use for.
fn decode(kind: u32, body: &[u8]) -> Option<Event> {
    match kind {
        EVENT_BINDING => {
            let event: BindingEvent = serde_json::from_slice(body).ok()?;
            (event.change == "run").then_some(Event::Binding(event.binding.command))
        }
        EVENT_WORKSPACE => {
            let event: WorkspaceEvent = serde_json::from_slice(body).ok()?;
            let Current { id, name } = event.current?;
            match event.change.as_str() {
                "init" => Some(Event::Created { id, name: name? }),
                "rename" => Some(Event::Renamed { id, name: name? }),
                "empty" => Some(Event::Emptied { id }),
                "focus" => Some(Event::Focused),
                _ => None,
            }
        }
        EVENT_OUTPUT => Some(Event::Outputs),
        EVENT_SHUTDOWN => Some(Event::Shutdown),
        _ => None,
    }
}

pub fn run(config: &Config) -> Result<()> {
    let socket = ipc::socket_path()?;
    take_over(&control_path(&socket))?;

    let mut events = Connection::connect_to(&socket)?;
    events.subscribe(&["binding", "workspace", "output", "shutdown"])?;
    let mut conn = Connection::connect_to(&socket)?;
    let defaults = config.to_args();
    // Workspace names by id. i3's rename event only carries the new name,
    // so this is where the old one comes from. Kept in event order, which
    // a fresh query after the fact would not be.
    let mut names: HashMap<u64, String> = conn
        .workspaces()?
        .into_iter()
        .map(|w| (w.id, w.name))
        .collect();

    // Only the modes that spread a desktop over the monitors follow.
    let follows = config.monitors.mode != MultiMonitorMode::DesktopPerMonitor;
    let mut restless = 0;
    if follows {
        report(follow(&mut conn, config, &mut restless));
    }

    loop {
        // EOF (i3 went away) ends the daemon like a shutdown event does.
        let (kind, body) = events.recv()?;
        match decode(kind, &body) {
            Some(Event::Binding(command)) => {
                restless = 0;
                for args in invocations(&command) {
                    let argv = ["i3-spatial-desktop".to_owned()]
                        .into_iter()
                        .chain(defaults.iter().cloned())
                        .chain(args);
                    match Cli::try_parse_from(argv).and_then(Cli::into_parts) {
                        Ok((_, Cmd::Daemon)) => {}
                        Ok((config, command)) => report(plan::execute(&mut conn, &config, command)),
                        Err(e) => eprintln!("i3-spatial-desktop: {command}: {e}"),
                    }
                }
            }
            Some(Event::Created { id, name }) => {
                names.insert(id, name.clone());
                report(react(&mut conn, config, |world| {
                    plan::adopt(world, config, &name)
                }));
            }
            Some(Event::Renamed { id, name }) => {
                if let Some(old) = names.insert(id, name.clone()) {
                    report(react(&mut conn, config, |world| {
                        plan::follow_rename(world, config, &old, &name)
                    }));
                }
            }
            Some(Event::Emptied { id }) => {
                names.remove(&id);
            }
            Some(Event::Focused | Event::Outputs) if follows => {
                report(follow(&mut conn, config, &mut restless));
            }
            Some(Event::Focused | Event::Outputs) => {}
            Some(Event::Shutdown) => return Ok(()),
            None => {}
        }
    }
}

/// Brings the other monitors along after the focus moved by other means.
/// A pass only acts on differences, so the events its own changes cause
/// find nothing left to do. `restless` counts the passes in a row that did
/// find something, and stops a chain that does not end.
fn follow(conn: &mut Connection, config: &Config, restless: &mut u32) -> Result<()> {
    if *restless >= RESTLESS {
        return Ok(());
    }
    let world = plan::tended(conn, &config.naming)?;
    let Some(plan) = plan::follow(&world, config) else {
        *restless = 0;
        return Ok(());
    };
    *restless += 1;
    if *restless == RESTLESS {
        eprintln!(
            "i3-spatial-desktop: the monitors keep changing; not following until the next key press"
        );
    }
    plan::carry_out(conn, &config.naming, world, &plan)
}

/// Answers an event: plans against i3 as it is now, and carries that out.
fn react(
    conn: &mut Connection,
    config: &Config,
    planner: impl FnOnce(&World) -> Result<Option<Plan>>,
) -> Result<()> {
    let world = plan::tended(conn, &config.naming)?;
    match planner(&world)? {
        Some(plan) => plan::carry_out(conn, &config.naming, world, &plan),
        None => Ok(()),
    }
}

fn report(result: Result<()>) {
    if let Err(e) = result {
        eprintln!("i3-spatial-desktop: {e}");
    }
}

/// `$XDG_RUNTIME_DIR/i3-spatial-desktop-<hash>.sock`, keyed by the window manager's
/// socket so each session gets its own. Socket paths are limited to ~108
/// bytes, which rules out simply appending to the window manager's path.
fn control_path(socket: &str) -> String {
    // FNV-1a: stable across builds, so an upgraded daemon finds the old one.
    let hash = socket.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    match std::env::var("XDG_RUNTIME_DIR") {
        Ok(dir) if !dir.is_empty() => format!("{dir}/i3-spatial-desktop-{hash:016x}.sock"),
        // Without a runtime dir, the WM socket's owner keeps users apart.
        _ => {
            let uid = std::fs::metadata(socket).map_or(0, |m| m.uid());
            format!("/tmp/i3-spatial-desktop-{uid}-{hash:016x}.sock")
        }
    }
}

/// Stops any running daemon, then listens on `path` and exits as soon as
/// a newer daemon connects.
fn take_over(path: &str) -> Result<()> {
    // Connecting is the stop signal; a failure just means none is running.
    // The old daemon never removes the file, so replacing it is race-free.
    drop(UnixStream::connect(path));
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path)?;

    std::thread::spawn(move || {
        if listener.accept().is_ok() {
            std::process::exit(0);
        }
    });
    Ok(())
}

/// Splits a binding's command the way i3 does (`;` and `,` separate
/// commands, `"…"` and `'…'` quote, `\` escapes inside double quotes) and
/// returns the arguments of every `nop i3-spatial-desktop …` command in it.
fn invocations(command: &str) -> Vec<Vec<String>> {
    let mut segments = vec![Vec::new()];
    let mut token: Option<String> = None;
    let mut quote: Option<char> = None;
    let mut chars = command.chars();

    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some('"'), '\\') => {
                if let Some(next) = chars.next() {
                    token.get_or_insert_default().push(next);
                }
            }
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => token.get_or_insert_default().push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                token.get_or_insert_default();
            }
            (None, ';' | ',') => {
                segments.last_mut().unwrap().extend(token.take());
                segments.push(Vec::new());
            }
            (None, c) if c.is_whitespace() => segments.last_mut().unwrap().extend(token.take()),
            (None, c) => token.get_or_insert_default().push(c),
        }
    }
    segments.last_mut().unwrap().extend(token);

    segments
        .into_iter()
        .filter_map(|tokens| match tokens.as_slice() {
            [nop, rel, rest @ ..] if nop == "nop" && rel == "i3-spatial-desktop" => Some(rest.to_vec()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn plain() {
        assert_eq!(invocations("nop i3-spatial-desktop focus up"), [s(&["focus", "up"])]);
    }

    #[test]
    fn quotes_and_escapes() {
        assert_eq!(
            invocations(r#"nop i3-spatial-desktop --left "a b" goto "4:db""#),
            [s(&["--left", "a b", "goto", "4:db"])]
        );
        assert_eq!(
            invocations(r#"nop i3-spatial-desktop goto "say \"hi\"""#),
            [s(&["goto", r#"say "hi""#])]
        );
        assert_eq!(invocations("nop i3-spatial-desktop goto 'x;y'"), [s(&["goto", "x;y"])]);
        assert_eq!(
            invocations(r#"nop i3-spatial-desktop --sep "" focus up"#),
            [s(&["--sep", "", "focus", "up"])]
        );
    }

    #[test]
    fn chained_commands() {
        assert_eq!(
            invocations("nop i3-spatial-desktop focus left; nop second, nop i3-spatial-desktop move up"),
            [s(&["focus", "left"]), s(&["move", "up"])]
        );
    }

    #[test]
    fn events() {
        let binding = |change: &str| {
            format!(r#"{{"change":"{change}","binding":{{"command":"nop i3-spatial-desktop focus up"}}}}"#)
        };
        assert_eq!(
            decode(EVENT_BINDING, binding("run").as_bytes()),
            Some(Event::Binding("nop i3-spatial-desktop focus up".into()))
        );
        assert_eq!(decode(EVENT_BINDING, binding("other").as_bytes()), None);

        let workspace = |change: &str| {
            format!(r#"{{"change":"{change}","current":{{"id":7,"name":"4:db"}},"old":null}}"#)
        };
        let name = || "4:db".to_owned();
        assert_eq!(
            decode(EVENT_WORKSPACE, workspace("init").as_bytes()),
            Some(Event::Created { id: 7, name: name() })
        );
        assert_eq!(
            decode(EVENT_WORKSPACE, workspace("rename").as_bytes()),
            Some(Event::Renamed { id: 7, name: name() })
        );
        assert_eq!(
            decode(EVENT_WORKSPACE, workspace("empty").as_bytes()),
            Some(Event::Emptied { id: 7 })
        );
        assert_eq!(
            decode(EVENT_WORKSPACE, workspace("focus").as_bytes()),
            Some(Event::Focused)
        );
        assert_eq!(decode(EVENT_WORKSPACE, workspace("urgent").as_bytes()), None);
        assert_eq!(decode(EVENT_OUTPUT, br#"{"change":"unspecified"}"#), Some(Event::Outputs));
        // A reload sends a workspace event with nothing in it.
        assert_eq!(
            decode(EVENT_WORKSPACE, br#"{"change":"reload","current":null}"#),
            None
        );
        assert_eq!(decode(EVENT_SHUTDOWN, br#"{"change":"restart"}"#), Some(Event::Shutdown));
        assert_eq!(decode(EVENT_WORKSPACE, b"not json"), None);
        assert_eq!(decode(0x8000_0003, b"{}"), None);
    }

    #[test]
    fn ignores_others() {
        assert!(invocations("nop").is_empty());
        assert!(invocations("nop something else").is_empty());
        assert!(invocations("workspace 1").is_empty());
    }
}
