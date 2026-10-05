//! A nested i3 in its own Xvfb. Nothing here may reach the session the
//! tests are run from: every child gets a cleared environment, the nested
//! i3 has its own socket, and that socket is always passed explicitly.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use crate::ipc::{Connection, Workspace};

const BIN: &str = env!("CARGO_BIN_EXE_i3-spatial-desktop");
const TIMEOUT: Duration = Duration::from_secs(10);
/// A workspace the daemon tests rename to learn that the daemon has caught
/// up. It is never shown, and is left out of [`Session::state`].
const BARRIER: &str = "99:barrier";
const BARRIER_RENAMED: &str = "barrier";

static SESSIONS: AtomicU32 = AtomicU32::new(0);

pub struct Session {
    dir: PathBuf,
    display: String,
    socket: String,
    /// Options put before every command, and given to the daemon.
    flags: Vec<String>,
    /// In start order; stopped in reverse.
    children: Vec<Child>,
}

fn path() -> String {
    std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into())
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

impl Session {
    /// Starts Xvfb and an i3 with the given `fake-outputs` and extra config
    /// lines.
    pub fn start(outputs: &str, config: &str) -> Self {
        let id = SESSIONS.fetch_add(1, Ordering::Relaxed);
        // Short, because socket paths are limited to ~108 bytes.
        let dir = PathBuf::from(format!("/tmp/i3sd-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        // Xvfb picks a free display and reports it once it accepts
        // connections.
        let mut xvfb = Command::new("Xvfb")
            .args(["-displayfd", "1", "-screen", "0", "3200x1200x24"])
            .args(["-nolisten", "tcp"])
            .env_clear()
            .env("PATH", path())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("these tests need Xvfb");
        let mut line = String::new();
        BufReader::new(xvfb.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let display = format!(":{}", line.trim());
        let socket = format!("{}/i3.sock", dir.display());
        let mut session = Session {
            dir,
            display,
            socket,
            flags: Vec::new(),
            children: vec![xvfb],
        };
        assert!(session.display.len() > 1, "Xvfb did not report a display");

        let config_path = session.dir.join("config");
        std::fs::write(
            &config_path,
            format!(
                "# i3 config file (v4)\nfont pango:monospace 8\nipc-socket {}\nfake-outputs {outputs}\n{config}\n",
                session.socket
            ),
        )
        .unwrap();
        let log = std::fs::File::create(session.dir.join("i3.log")).unwrap();
        let i3 = Command::new("i3")
            .arg("-c")
            .arg(&config_path)
            .env_clear()
            .env("PATH", path())
            .env("DISPLAY", &session.display)
            .env("HOME", &session.dir)
            .env("XDG_RUNTIME_DIR", &session.dir)
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .expect("these tests need i3");
        session.children.push(i3);
        wait_until("the nested i3", || {
            Path::new(&session.socket).exists() && Connection::connect_to(&session.socket).is_ok()
        });
        session.conn();
        session
    }

    pub fn flags(mut self, flags: &[&str]) -> Self {
        self.flags = flags.iter().map(|f| (*f).to_owned()).collect();
        self
    }

    /// A connection to the nested i3, refused unless it really is the
    /// nested one.
    fn conn(&self) -> Connection {
        let mut conn = Connection::connect_to(&self.socket).expect("the nested i3 is gone");
        let outputs = conn.outputs().unwrap();
        assert!(
            outputs
                .iter()
                .filter(|o| o.active)
                .all(|o| o.name.starts_with("fake-")),
            "this is not the nested i3; refusing to continue"
        );
        conn
    }

    /// Runs an i3 command, which must succeed.
    pub fn i3(&self, command: &str) {
        let replies = self.conn().replies(command).unwrap();
        if let Some(failed) = replies.iter().find(|reply| !reply.success) {
            panic!("{command}: {}", failed.error.as_deref().unwrap_or_default());
        }
    }

    pub fn workspaces(&self) -> Vec<Workspace> {
        self.conn().workspaces().unwrap()
    }

    /// Every output with its workspaces in i3's order: `>name<` has the
    /// focus, `[name]` is visible, a bare name is hidden.
    pub fn state(&self) -> String {
        self.render()
    }

    fn render(&self) -> String {
        let mut by_output: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for w in self.workspaces() {
            if w.name == BARRIER || w.name == BARRIER_RENAMED {
                continue;
            }
            let shown = if w.focused {
                format!(">{}<", w.name)
            } else if w.visible {
                format!("[{}]", w.name)
            } else {
                w.name
            };
            by_output.entry(w.output).or_default().push(shown);
        }
        by_output
            .iter()
            .map(|(output, names)| format!("{output}: {}", names.join(", ")))
            .collect::<Vec<_>>()
            .join(" / ")
    }

    /// Focuses each workspace in turn and puts an empty container in it, so
    /// i3 keeps it when it is hidden. The last one keeps the focus.
    pub fn fill(&self, names: &[&str]) {
        for name in names {
            self.i3(&format!(
                "workspace --no-auto-back-and-forth \"{name}\"; open"
            ));
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(BIN);
        command
            .env_clear()
            .env("I3SOCK", &self.socket)
            .env("DISPLAY", &self.display)
            .env("XDG_RUNTIME_DIR", &self.dir)
            .args(&self.flags);
        command
    }

    /// Runs the tool once.
    pub fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }

    /// Runs the tool, which must succeed.
    pub fn ok(&self, args: &[&str]) {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Runs the tool, which must exit with `code`; returns what it printed
    /// on stderr.
    pub fn fails(&self, code: i32, args: &[&str]) -> String {
        let out = self.run(args);
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert_eq!(out.status.code(), Some(code), "{args:?}: {stderr}");
        stderr
    }

    /// Starts the daemon and waits until it answers.
    pub fn daemon(&mut self) {
        let log = std::fs::File::create(self.dir.join("daemon.log")).unwrap();
        let daemon = self
            .command()
            .arg("daemon")
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .unwrap();
        self.children.push(daemon);
        // Its control socket appears just before it subscribes to i3.
        wait_until("the daemon", || {
            std::fs::read_dir(&self.dir).unwrap().flatten().any(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                name.starts_with("i3-spatial-desktop-") && name.ends_with(".sock")
            })
        });
        std::thread::sleep(Duration::from_millis(50));
        // Made without ever being shown: the daemon acts on what is on
        // screen, and renaming a workspace it is acting on would race with
        // it.
        self.i3(&format!("open; move container to workspace \"{BARRIER}\""));
        self.barrier();
    }

    fn daemon_log(&self) -> String {
        std::fs::read_to_string(self.dir.join("daemon.log")).unwrap_or_default()
    }

    /// Returns once the daemon has handled every event i3 has sent so far.
    /// A plain rename that drops a number makes the daemon put it back;
    /// seeing that means everything before it was handled too.
    pub fn barrier(&self) {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.i3(&format!(
                "rename workspace \"{BARRIER}\" to {BARRIER_RENAMED}"
            ));
            let attempt = Instant::now() + Duration::from_secs(1);
            while Instant::now() < attempt {
                if self.workspaces().iter().any(|w| w.name == BARRIER) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            // The daemon was not listening yet. Put the name back and retry,
            // slowly: it must see each rename on its own.
            assert!(
                Instant::now() < deadline,
                "the daemon never answered:\n{}",
                self.daemon_log()
            );
            self.i3(&format!(
                "rename workspace {BARRIER_RENAMED} to \"{BARRIER}\""
            ));
            std::thread::sleep(Duration::from_millis(300));
        }
    }

    /// Waits for the daemon to bring i3 to `expected`.
    pub fn becomes(&self, expected: &str) {
        self.barrier();
        assert_eq!(self.state(), expected, "daemon log:\n{}", self.daemon_log());
    }

    /// Starts an X program in the nested server and waits for its window,
    /// or returns false when it or xdotool is not installed.
    pub fn open(&mut self, program: &str, class: &str) -> bool {
        let mut command = Command::new(program);
        command
            .env_clear()
            .env("PATH", path())
            .env("DISPLAY", &self.display)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let Ok(child) = command.spawn() else {
            return false;
        };
        self.children.push(child);
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let found = Command::new("xdotool")
                .args(["search", "--class", class])
                .env_clear()
                .env("PATH", path())
                .env("DISPLAY", &self.display)
                .output();
            match found {
                Err(_) => return false,
                Ok(found) if !found.stdout.is_empty() => return true,
                Ok(_) => assert!(Instant::now() < deadline, "{program} never opened a window"),
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Presses a key combination in the nested X server, or returns false
    /// when xdotool is not installed.
    pub fn press(&self, keys: &str) -> bool {
        Command::new("xdotool")
            .args(["key", keys])
            .env_clear()
            .env("PATH", path())
            .env("DISPLAY", &self.display)
            .status()
            .is_ok_and(|status| status.success())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        for child in self.children.iter_mut().rev() {
            // SIGTERM, so Xvfb removes its lock file.
            let _ = Command::new("kill").arg(child.id().to_string()).status();
            let deadline = Instant::now() + Duration::from_secs(3);
            while matches!(child.try_wait(), Ok(None)) {
                if Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
