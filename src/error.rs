use thiserror::Error;

use crate::grid::Location;

#[derive(Debug, Error)]
pub enum Error {
    #[error("i/o error talking to the window manager: {0}")]
    Io(#[from] std::io::Error),

    #[error("could not decode window manager reply: {0}")]
    Json(#[from] serde_json::Error),

    #[error("could not find the i3/sway IPC socket (set I3SOCK or SWAYSOCK)")]
    NoSocket,

    #[error("malformed IPC reply from the window manager")]
    BadReply,

    #[error("no focused workspace")]
    NoFocusedWorkspace,

    #[error("output {0:?} not found")]
    UnknownOutput(String),

    #[error("command {command:?} failed: {message}")]
    Command { command: String, message: String },

    #[error("location {0} is not on the grid")]
    OffGrid(Location),

    #[error("{0:?} names a satellite; a desktop is named after its origin")]
    SatelliteName(String),

    #[error("renaming {from:?} to {to:?} would change the desktop's number")]
    NumberChange { from: String, to: String },

    #[error("desktop {0:?} already exists")]
    DesktopExists(String),

    #[error("`{command}` needs --multimonitor-mode={mode}")]
    NeedsMode {
        command: &'static str,
        mode: &'static str,
    },
}

pub type Result<T> = std::result::Result<T, Error>;
