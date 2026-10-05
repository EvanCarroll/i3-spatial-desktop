//! Every command as a plan: what it sends, and the state it must leave
//! behind. Planning is pure; [`execute`] and [`carry_out`] do the talking.

use std::collections::BTreeSet;

use crate::cli::{Cmd, destination};
use crate::command::{self, Command};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::grid::ORIGIN;
use crate::ipc::Connection;
use crate::mode::desktop_per_monitor::DesktopPerMonitor;
use crate::mode::location_per_monitor::LocationPerMonitor;
use crate::mode::pan::Pan;
use crate::mode::{MonitorGrid, MonitorMode, MultiMonitorMode, Plan, Route, Wanted};
use crate::naming::{DesktopName, Naming, leading_num, unpinned};
use crate::reconcile::{reconcile, tidy};
use crate::world::{Workspace, World};

pub fn execute(conn: &mut Connection, config: &Config, command: Cmd) -> Result<()> {
    let world = tended(conn, &config.naming)?;
    match plan(&world, config, command)? {
        Some(plan) => carry_out(conn, &config.naming, world, &plan),
        None => Ok(()),
    }
}

/// i3 as it is now, once the pins are in order.
pub fn tended(conn: &mut Connection, naming: &Naming) -> Result<World> {
    let world = World::snapshot(conn, naming)?;
    let tending = tend(&world);
    if tending.is_empty() {
        return Ok(world);
    }
    command::run(conn, &tending)?;
    World::snapshot(conn, naming)
}

/// What the pins need before anything is planned. A pin lasts while its
/// workspace is on screen, so a marker on a hidden workspace goes. And a
/// rule that names a pinned workspace plainly makes i3 create a second one
/// beside it, whose windows belong in the first.
fn tend(world: &World) -> Vec<Command> {
    let pinned = world.workspaces.iter().filter(|w| w.pinned);
    pinned
        .filter_map(|pinned| {
            let plain = unpinned(&pinned.name).0;
            let twin = world.named(plain);
            match (pinned.visible, twin) {
                (true, None) => None,
                (true, Some(twin)) => Some(Command::Fold {
                    from: twin.name.clone(),
                    into: pinned.name.clone(),
                }),
                (false, None) => Some(Command::Rename {
                    from: pinned.name.clone(),
                    to: plain.to_owned(),
                }),
                // The plain one takes over.
                (false, Some(twin)) => Some(Command::Fold {
                    from: pinned.name.clone(),
                    into: twin.name.clone(),
                }),
            }
        })
        .collect()
}

/// What the monitors must show now that the focus has moved by other means
/// than this tool, if anything.
pub fn follow(world: &World, config: &Config) -> Option<Plan> {
    let want = mode(config, world).follow(world)?;
    Some(Plan {
        pre: Vec::new(),
        want,
    })
}

/// Runs a plan made from `world`: the action's own commands, whatever it
/// takes to reach the wanted state, then the bar order.
pub fn carry_out(conn: &mut Connection, naming: &Naming, world: World, plan: &Plan) -> Result<()> {
    let world = if plan.pre.is_empty() {
        world
    } else {
        // On their own, so a failure is reported as this command's.
        command::run(conn, &plan.pre)?;
        World::snapshot(conn, naming)?
    };
    command::run(conn, &reconcile(&world, &plan.want))?;

    let world = World::snapshot(conn, naming)?;
    let renames = tidy(&world, &desktops(&world, &plan.want));
    if !renames.is_empty() {
        command::run(conn, &renames)?;
        // Renaming moves a workspace to its assigned output, if it has one.
        let world = World::snapshot(conn, naming)?;
        command::run(conn, &reconcile(&world, &plan.want))?;
    }
    Ok(())
}

/// The desktops a wanted state is about, whose bar order is then seen to:
/// the ones gathered, or else the ones on the monitors that changed and the
/// one with the focus.
fn desktops(world: &World, want: &Wanted) -> BTreeSet<DesktopName> {
    if !want.gather.is_empty() {
        return (want.gather.iter().map(|(desktop, _)| desktop.clone())).collect();
    }
    (want.shown.values().chain(&want.focus))
        .map(|name| world.address_of(name).desktop)
        .collect()
}

/// The mode the settings ask for, on the monitors there are now.
fn mode<'a>(config: &'a Config, world: &World) -> Box<dyn MonitorMode + 'a> {
    let navigation = &config.navigation;
    let grid = || MonitorGrid::new(&world.monitors, &config.monitors.locations);
    match config.monitors.mode {
        MultiMonitorMode::DesktopPerMonitor => Box::new(DesktopPerMonitor { navigation }),
        MultiMonitorMode::LocationPerMonitor => Box::new(LocationPerMonitor {
            grid: grid(),
            navigation,
        }),
        MultiMonitorMode::Pan => Box::new(Pan {
            grid: grid(),
            navigation,
        }),
    }
}

fn plan(world: &World, config: &Config, command: Cmd) -> Result<Option<Plan>> {
    let mode = mode(config, world);
    let route = |route: Option<Route>| route.map(|route| route.plan);
    Ok(match command {
        Cmd::Focus {
            direction,
            location,
        } => route(mode.travel(world, destination(direction, location))?),
        Cmd::Move {
            direction,
            location,
        } => carry(world, mode.travel(world, destination(direction, location))?)?,
        Cmd::Output { side } => mode.relocate(world, side.into())?,
        Cmd::Goto { name, location } => route(mode.switch(world, &world.resolve(&name), location)?),
        Cmd::Send { name, location } => {
            carry(world, mode.switch(world, &world.resolve(&name), location)?)?
        }
        Cmd::RenameDesktop { name } => rename_desktop(world, mode.as_ref(), &name)?,
        Cmd::PinMonitor { state } => mode.pin(world, state)?,
        Cmd::Daemon => None,
    })
}

/// `move` and `send`: the container goes to the workspace the focus would
/// have gone to, and the focus stays. Nothing to do when that is the
/// focused workspace.
fn carry(world: &World, route: Option<Route>) -> Result<Option<Plan>> {
    let current = world.focused()?;
    Ok(route
        .filter(|route| route.target != current.name)
        .map(|route| Plan {
            pre: vec![Command::MoveContainerTo(route.target)],
            want: Wanted {
                shown: Default::default(),
                focus: Some(current.name.clone()),
                ..route.plan.want
            },
        }))
}

/// The new origin name for renaming the desktop of `origin` to `name`. The
/// number stays: `Bar` on desktop 4 is `4:Bar`; `7:Bar` is refused.
fn renamed_origin(world: &World, origin: &str, name: &str) -> Result<String> {
    if world.address_of(name).location != ORIGIN {
        return Err(Error::SatelliteName(name.to_owned()));
    }
    match (leading_num(origin), leading_num(name)) {
        (Some(num), None) => Ok(format!("{num}:{name}")),
        (old, new) if old == new => Ok(name.to_owned()),
        _ => Err(Error::NumberChange {
            from: origin.to_owned(),
            to: name.to_owned(),
        }),
    }
}

/// `workspaces` of the desktop `old` renamed to the desktop `new`, each
/// keeping its location suffix, in bar order so they stay sorted.
fn renames(workspaces: &mut [&Workspace], old: &DesktopName, new: &DesktopName) -> Vec<Command> {
    workspaces.sort_by_key(|w| w.address.location.bar_order());
    workspaces
        .iter()
        .map(|w| Command::Rename {
            from: w.name.clone(),
            to: renamed(&w.name, old, new),
        })
        .collect()
}

fn renamed(name: &str, old: &DesktopName, new: &DesktopName) -> String {
    format!("{new}{}", &name[old.as_str().len()..])
}

/// `rename-desktop`: the origin and every satellite.
fn rename_desktop(world: &World, mode: &dyn MonitorMode, name: &str) -> Result<Option<Plan>> {
    let current = world.focused()?;
    let old = &current.address.desktop;
    let new = DesktopName(renamed_origin(world, old.as_str(), name)?);
    if new == *old {
        return Ok(None);
    }
    if world.desktop(&new).next().is_some() {
        return Err(Error::DesktopExists(new.0));
    }
    let mut members: Vec<&Workspace> = world.desktop(old).collect();
    let pre = renames(&mut members, old, &new);
    let focus = renamed(&current.name, old, &new);
    let want = mode.settled(new, members[0].output.clone(), focus);
    Ok(Some(Plan { pre, want }))
}

/// A workspace created by something else (an app rule, a script, …) joins
/// its desktop, wherever the desktop's other workspaces are.
pub fn adopt(world: &World, config: &Config, name: &str) -> Result<Option<Plan>> {
    let desktop = world.address_of(name).desktop;
    let Some(output) = (world.desktop(&desktop))
        .find(|w| w.name != name)
        .map(|w| w.output.clone())
    else {
        return Ok(None);
    };
    let focus = world.focused()?.name.clone();
    Ok(Some(Plan {
        pre: Vec::new(),
        want: mode(config, world).settled(desktop, output, focus),
    }))
}

/// The number to put back when a rename dropped it: `4:Foo` renamed to
/// `Baz` becomes `4:Baz`. A rename to a different number was deliberate and
/// is left alone.
fn restored_number(old: &str, new: &str) -> Option<String> {
    match (leading_num(old), leading_num(new)) {
        (Some(num), None) => Some(format!("{num}:{new}")),
        _ => None,
    }
}

/// After an origin was renamed from `old` to `new` by something else (a
/// plain i3 `rename workspace`, e.g. from i3-input): keeps the desktop's
/// number if the rename dropped it, and brings the satellites along.
pub fn follow_rename(
    world: &World,
    config: &Config,
    old: &str,
    new: &str,
) -> Result<Option<Plan>> {
    // An origin renamed to another desktop's name. A pin is a rename too,
    // but leaves the address as it was.
    let (from, to) = (world.address_of(old), world.address_of(new));
    if from == to || from.location != ORIGIN || to.location != ORIGIN {
        return Ok(None);
    }
    let mut pre = Vec::new();
    let mut desktop = to.desktop;
    // Unless that would collide with another desktop; then the plain rename
    // stands.
    let numbered = restored_number(old, new).map(DesktopName);
    if let Some(numbered) = numbered.filter(|numbered| world.desktop(numbered).next().is_none()) {
        pre.push(Command::Rename {
            from: new.to_owned(),
            to: numbered.0.clone(),
        });
        desktop = numbered;
    }
    // Only the satellites follow. A workspace that carries the old origin's
    // name by now is not the one that was renamed.
    let mut satellites: Vec<&Workspace> = (world.desktop(&from.desktop))
        .filter(|w| w.address.location != ORIGIN)
        .collect();
    pre.extend(renames(&mut satellites, &from.desktop, &desktop));
    let Some(first) = satellites.first() else {
        return Ok((!pre.is_empty()).then(|| Plan {
            pre,
            want: Wanted::default(),
        }));
    };

    let current = world.focused()?;
    let focus = if current.name == new {
        desktop.0.clone()
    } else if current.address.desktop == from.desktop && current.address.location != ORIGIN {
        renamed(&current.name, &from.desktop, &desktop)
    } else {
        current.name.clone()
    };
    let want = mode(config, world).settled(desktop, first.output.clone(), focus);
    Ok(Some(Plan { pre, want }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use crate::world::OutputName;
    use clap::Parser;

    fn texts(commands: &[Command]) -> Vec<String> {
        commands.iter().map(Command::to_string).collect()
    }

    fn config(args: &[&str]) -> (Config, Cmd) {
        Cli::try_parse_from(std::iter::once("i3-spatial-desktop").chain(args.iter().copied()))
            .unwrap()
            .into_parts()
            .unwrap()
    }

    /// The plan for a command line in a world.
    fn plan_for(world: &str, args: &[&str]) -> Result<Option<Plan>> {
        let (config, command) = config(args);
        plan(&World::of(world), &config, command)
    }

    fn gathered(desktop: &str, output: &str, focus: Option<&str>) -> Wanted {
        Wanted::gathered(
            desktop.into(),
            OutputName(output.into()),
            focus.map(str::to_owned),
        )
    }

    #[test]
    fn focus_sends_one_command_and_then_needs_nothing() {
        let plan = plan_for("fake-0: >4<, 6", &["focus", "up"]).unwrap().unwrap();
        assert_eq!(
            texts(&plan.pre),
            [r#"workspace --no-auto-back-and-forth "4: ↑""#]
        );
        // Once i3 has done that, the wanted state is already there.
        let after = World::of("fake-0: 4, >4: ↑<, 6");
        assert!(reconcile(&after, &plan.want).is_empty());
        assert!(tidy(&after, &desktops(&after, &plan.want)).is_empty());
        // Already there.
        assert_eq!(plan_for("fake-0: >4<", &["focus", "origin"]).unwrap(), None);
    }

    #[test]
    fn move_carries_the_container_and_keeps_the_focus() {
        let plan = plan_for("fake-0: >4<, 6", &["move", "left"]).unwrap().unwrap();
        assert_eq!(texts(&plan.pre), [r#"move container to workspace "4: ←""#]);
        assert_eq!(plan.want, gathered("4", "fake-0", Some("4")));
        assert_eq!(plan_for("fake-0: >4<", &["move", "origin"]).unwrap(), None);
    }

    #[test]
    fn send_goes_where_goto_would() {
        let world = "fake-0: [4:db] / fake-1: >6<";
        let plan = plan_for(world, &["send", "4", "--location=1,0"]).unwrap().unwrap();
        assert_eq!(
            texts(&plan.pre),
            [r#"move container to workspace "4:db: →""#]
        );
        assert_eq!(plan.want, gathered("4:db", "fake-0", Some("6")));
        // The focused workspace: nothing to do, where `goto` would still ask.
        assert_eq!(plan_for(world, &["send", "6"]).unwrap(), None);
        let plan = plan_for(world, &["goto", "6"]).unwrap().unwrap();
        assert_eq!(texts(&plan.pre), [r#"workspace "6""#]);
    }

    #[test]
    fn rename_desktop_renames_in_bar_order() {
        let world = "fake-0: 4:Other, 4:Foo: ↑, 4:Foo, >4:Foo: ←<";
        let plan = plan_for(world, &["rename-desktop", "Bar"]).unwrap().unwrap();
        assert_eq!(
            texts(&plan.pre),
            [
                r#"rename workspace "4:Foo: ←" to "4:Bar: ←""#,
                r#"rename workspace "4:Foo" to "4:Bar""#,
                r#"rename workspace "4:Foo: ↑" to "4:Bar: ↑""#,
            ]
        );
        assert_eq!(plan.want, gathered("4:Bar", "fake-0", Some("4:Bar: ←")));
        assert_eq!(plan_for(world, &["rename-desktop", "4:Foo"]).unwrap(), None);
        assert!(matches!(
            plan_for(world, &["rename-desktop", "Other"]),
            Err(Error::DesktopExists(_))
        ));
        assert!(matches!(
            plan_for(world, &["rename-desktop", "7:Bar"]),
            Err(Error::NumberChange { .. })
        ));
        assert!(matches!(
            plan_for(world, &["rename-desktop", "4:Bar: ←"]),
            Err(Error::SatelliteName(_))
        ));
    }

    #[test]
    fn a_new_workspace_joins_its_desktop() {
        let (config, _) = config(&["daemon"]);
        // `4: →` was made on the focused output.
        let world = World::of("fake-0: [4] / fake-1: 4: →, >6<");
        let plan = adopt(&world, &config, "4: →").unwrap().unwrap();
        assert!(plan.pre.is_empty());
        assert_eq!(plan.want, gathered("4", "fake-0", Some("6")));
        // The first of its desktop: nowhere to join.
        assert_eq!(adopt(&world, &config, "6").unwrap(), None);
    }

    #[test]
    fn a_plain_rename_is_followed() {
        let (config, _) = config(&["daemon"]);
        let follow = |world: &str, old: &str, new: &str| {
            follow_rename(&World::of(world), &config, old, new).unwrap()
        };
        // The number is put back, and the satellites follow.
        let plan = follow("fake-0: 4:Foo: ←, >Baz<, 4:Foo: ↑", "4:Foo", "Baz").unwrap();
        assert_eq!(
            texts(&plan.pre),
            [
                r#"rename workspace "Baz" to "4:Baz""#,
                r#"rename workspace "4:Foo: ←" to "4:Baz: ←""#,
                r#"rename workspace "4:Foo: ↑" to "4:Baz: ↑""#,
            ]
        );
        assert_eq!(plan.want, gathered("4:Baz", "fake-0", Some("4:Baz")));
        // A different number is deliberate.
        let plan = follow("fake-0: >4:Foo: ↑<, 7:Qux", "4:Foo", "7:Qux").unwrap();
        assert_eq!(
            texts(&plan.pre),
            [r#"rename workspace "4:Foo: ↑" to "7:Qux: ↑""#]
        );
        assert_eq!(plan.want, gathered("7:Qux", "fake-0", Some("7:Qux: ↑")));
        // The number would collide: the plain rename stands.
        assert_eq!(follow("fake-0: 4:Taken, >Taken<", "4:Foo", "Taken"), None);
        // No satellites: only the number.
        let plan = follow("fake-0: >Baz<", "4:Foo", "Baz").unwrap();
        assert_eq!(texts(&plan.pre), [r#"rename workspace "Baz" to "4:Baz""#]);
        assert_eq!(plan.want, Wanted::default());
        // A satellite's rename is nobody's business.
        assert_eq!(follow("fake-0: >4<, elsewhere", "4: ↑", "elsewhere"), None);
        assert_eq!(follow("fake-0: >4<, 4: ↑", "4: ←", "4: ↑"), None);
        // Nor is a pin, which renames an origin without renaming its desktop.
        assert_eq!(follow("fake-0: >4 📌<, 4: ↑", "4", "4 📌"), None);
        assert_eq!(follow("fake-0: >4<, 4: ↑", "4 📌", "4"), None);
    }

    #[test]
    fn pins_are_tended_first() {
        let tend = |world: &str| texts(&tend(&World::of(world)));
        // On screen and alone: nothing to do.
        assert!(tend("fake-0: [4: ← 📌] / fake-1: >5<").is_empty());
        // Off screen, a pin is over.
        assert_eq!(
            tend("fake-0: [9], 4: ← 📌 / fake-1: >5<"),
            [r#"rename workspace "4: ← 📌" to "4: ←""#]
        );
        // A rule made a second workspace under the plain name.
        assert_eq!(
            tend("fake-0: [10:com 📌] / fake-1: 10:com, >5<"),
            [r#"[workspace="^10:com$"] move container to workspace "10:com 📌""#]
        );
        // Both, with the pinned one hidden: the plain one takes over.
        assert_eq!(
            tend("fake-0: [9], 10:com 📌 / fake-1: 10:com, >5<"),
            [r#"[workspace="^10:com 📌$"] move container to workspace "10:com""#]
        );
    }

    #[test]
    fn pin_monitor_needs_its_mode() {
        let world = "fake-0: >4<";
        assert!(matches!(
            plan_for(world, &["pin-monitor"]),
            Err(Error::NeedsMode { .. })
        ));
        let mode = "--multimonitor-mode=location-per-monitor";
        let plan = plan_for(world, &[mode, "pin-monitor"]).unwrap().unwrap();
        assert_eq!(texts(&plan.pre), [r#"rename workspace "4" to "4 📌""#]);
        assert_eq!(plan_for(world, &[mode, "pin-monitor", "off"]).unwrap(), None);
    }

    /// Renaming an origin away and straight back used to make the daemon
    /// rename it to and fro without end: by the time the first rename was
    /// handled, a workspace carried the old name again and was taken for
    /// one to bring along.
    #[test]
    fn a_rename_and_back_is_left_alone() {
        let (config, _) = config(&["daemon"]);
        let world = World::of("fake-0: >4:Foo<");
        assert_eq!(follow_rename(&world, &config, "4:Foo", "4:Bar").unwrap(), None);
        assert_eq!(follow_rename(&world, &config, "4:Bar", "4:Foo").unwrap(), None);
    }
}
