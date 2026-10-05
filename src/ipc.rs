//! Minimal i3/sway IPC client. Both speak the same protocol:
//! `"i3-ipc" <u32 length> <u32 type> <json payload>` in native byte order.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process::Command;

use serde::Deserialize;

use crate::error::{Error, Result};

const MAGIC: &[u8; 6] = b"i3-ipc";
const RUN_COMMAND: u32 = 0;
const GET_WORKSPACES: u32 = 1;
const SUBSCRIBE: u32 = 2;
const GET_OUTPUTS: u32 = 3;

/// Event message types have the high bit set.
pub const EVENT_WORKSPACE: u32 = 0x8000_0000;
pub const EVENT_OUTPUT: u32 = 0x8000_0001;
pub const EVENT_BINDING: u32 = 0x8000_0005;
pub const EVENT_SHUTDOWN: u32 = 0x8000_0006;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Workspace {
    /// Stable for the workspace's lifetime, unlike its name.
    pub id: u64,
    pub name: String,
    /// Leading number of the name, `-1` if there is none.
    pub num: i32,
    pub focused: bool,
    /// Shown on its output, focused or not.
    pub visible: bool,
    pub output: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Output {
    pub name: String,
    pub active: bool,
    /// Never set by sway.
    #[serde(default)]
    pub primary: bool,
    pub rect: Rect,
}

/// The window manager's answer to one command.
#[derive(Debug, Deserialize)]
pub struct CommandReply {
    pub success: bool,
    #[serde(default)]
    pub error: Option<String>,
}

pub struct Connection {
    stream: UnixStream,
}

impl Connection {
    pub fn connect() -> Result<Self> {
        Self::connect_to(&socket_path()?)
    }

    pub fn connect_to(path: &str) -> Result<Self> {
        Ok(Self {
            stream: UnixStream::connect(path)?,
        })
    }

    fn send(&mut self, kind: u32, payload: &[u8]) -> Result<()> {
        let len = u32::try_from(payload.len()).map_err(|_| Error::BadReply)?;
        let mut msg = Vec::with_capacity(14 + payload.len());
        msg.extend_from_slice(MAGIC);
        msg.extend_from_slice(&len.to_ne_bytes());
        msg.extend_from_slice(&kind.to_ne_bytes());
        msg.extend_from_slice(payload);
        Ok(self.stream.write_all(&msg)?)
    }

    /// Reads one message: a reply, or an event on a subscribed connection.
    pub fn recv(&mut self) -> Result<(u32, Vec<u8>)> {
        let mut header = [0u8; 14];
        self.stream.read_exact(&mut header)?;
        if &header[..6] != MAGIC {
            return Err(Error::BadReply);
        }
        let len = u32::from_ne_bytes(header[6..10].try_into().map_err(|_| Error::BadReply)?);
        let kind = u32::from_ne_bytes(header[10..14].try_into().map_err(|_| Error::BadReply)?);
        let mut body = vec![0u8; len as usize];
        self.stream.read_exact(&mut body)?;
        Ok((kind, body))
    }

    fn request(&mut self, kind: u32, payload: &[u8]) -> Result<Vec<u8>> {
        self.send(kind, payload)?;
        Ok(self.recv()?.1)
    }

    /// Subscribes to `events`; afterwards only use [`Self::recv`].
    pub fn subscribe(&mut self, events: &[&str]) -> Result<()> {
        let reply: CommandReply =
            serde_json::from_slice(&self.request(SUBSCRIBE, &serde_json::to_vec(events)?)?)?;
        if reply.success {
            Ok(())
        } else {
            Err(Error::BadReply)
        }
    }

    pub fn workspaces(&mut self) -> Result<Vec<Workspace>> {
        Ok(serde_json::from_slice(&self.request(GET_WORKSPACES, b"")?)?)
    }

    pub fn outputs(&mut self) -> Result<Vec<Output>> {
        Ok(serde_json::from_slice(&self.request(GET_OUTPUTS, b"")?)?)
    }

    /// Runs a (possibly `;`-chained) command and returns the answer to each
    /// part of it.
    pub fn replies(&mut self, command: &str) -> Result<Vec<CommandReply>> {
        Ok(serde_json::from_slice(
            &self.request(RUN_COMMAND, command.as_bytes())?,
        )?)
    }
}

/// `SWAYSOCK`, then `I3SOCK`, then ask the binaries themselves.
pub fn socket_path() -> Result<String> {
    for var in ["SWAYSOCK", "I3SOCK"] {
        if let Ok(path) = std::env::var(var)
            && !path.is_empty()
        {
            return Ok(path);
        }
    }
    for wm in ["i3", "sway"] {
        if let Ok(out) = Command::new(wm).arg("--get-socketpath").output()
            && out.status.success()
        {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            if !path.is_empty() {
                return Ok(path);
            }
        }
    }
    Err(Error::NoSocket)
}
