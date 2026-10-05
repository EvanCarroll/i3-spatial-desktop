//! What the monitors show. A mode works out where a request leads and what
//! has to be true afterwards; it never talks to i3.

use std::collections::BTreeMap;

use clap::ValueEnum;

use crate::command::Command;
use crate::error::{Error, Result};
use crate::grid::{Direction, Location};
use crate::naming::DesktopName;
use crate::navigation::Destination;
use crate::world::{OutputName, World};

pub mod desktop_per_monitor;
pub mod location_per_monitor;
mod monitor_grid;
pub mod pan;

pub use monitor_grid::MonitorGrid;

/// What each monitor is given to show.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum MultiMonitorMode {
    /// A desktop's workspaces share a monitor, and each monitor walks the
    /// desktop it shows.
    #[default]
    DesktopPerMonitor,
    /// Each monitor shows its own location of the current desktop.
    LocationPerMonitor,
    /// The monitors are one window that the arrows slide over the current
    /// desktop.
    Pan,
}

/// What `pin-monitor` is asked to do.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum Pin {
    /// The focused monitor keeps the workspace it shows.
    On,
    /// It follows the desktop again.
    Off,
    #[default]
    Toggle,
}

/// The state an action must leave behind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Wanted {
    /// Monitors that must change, and the workspace each must show.
    pub shown: BTreeMap<OutputName, String>,
    /// Desktops whose workspaces must all be on one output.
    pub gather: Vec<(DesktopName, OutputName)>,
    /// The workspace that must end up focused. `None` leaves the focus
    /// wherever it is.
    pub focus: Option<String>,
    /// The workspace `workspace back_and_forth` should lead to afterwards.
    /// Showing several monitors a new workspace leaves i3 remembering
    /// whichever was visited last.
    pub back: Option<String>,
}

impl Wanted {
    /// A desktop kept together on `output`.
    pub fn gathered(desktop: DesktopName, output: OutputName, focus: Option<String>) -> Self {
        Wanted {
            gather: vec![(desktop, output)],
            focus,
            ..Wanted::default()
        }
    }
}

/// What an action sends itself, and the state it must leave behind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub pre: Vec<Command>,
    pub want: Wanted,
}

/// Where a request leads: the workspace that gets the focus, and the plan
/// that takes it there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub target: String,
    pub plan: Plan,
}

impl Route {
    /// The route to `target` on monitors that must show `shown` for it.
    /// From there, `workspace back_and_forth` leads to what the monitor
    /// `target` appears on shows now.
    pub fn showing(world: &World, shown: BTreeMap<OutputName, String>, target: String) -> Self {
        let back = (shown.iter())
            .find(|(_, name)| **name == target)
            .and_then(|(monitor, _)| world.visible_on(monitor))
            .map(|before| before.name.clone())
            .filter(|before| *before != target);
        Route {
            plan: Plan {
                pre: Vec::new(),
                want: Wanted {
                    shown,
                    focus: Some(target.clone()),
                    back,
                    ..Wanted::default()
                },
            },
            target,
        }
    }
}

pub trait MonitorMode {
    /// `focus <arrow|origin|--location>`: where it leads on the current
    /// desktop, or `None` when that is nowhere new.
    fn travel(&self, world: &World, to: Destination) -> Result<Option<Route>>;

    /// `goto`: where a desktop key leads. `resolved` is the workspace the
    /// name stands for, see [`World::resolve`].
    fn switch(&self, world: &World, resolved: &str, at: Option<Location>) -> Result<Option<Route>>;

    /// `output <side>`: the plan that moves what is focused to the
    /// neighbouring monitor, or `None` at the edge.
    fn relocate(&self, world: &World, side: Direction) -> Result<Option<Plan>>;

    /// What must be true of a desktop whose workspaces were just renamed or
    /// created by something else: `focus` keeps the focus, `output` is
    /// where the desktop was found.
    fn settled(&self, desktop: DesktopName, output: OutputName, focus: String) -> Wanted;

    /// What the other monitors must show now that the focus is where it
    /// is, having got there by other means than this tool. The focus does
    /// not move. `None` when they already do.
    fn follow(&self, _world: &World) -> Option<Wanted> {
        None
    }

    /// `pin-monitor`: the plan that pins the focused monitor to the
    /// workspace it shows, or lets it go. `None` when it is so already.
    fn pin(&self, _world: &World, _pin: Pin) -> Result<Option<Plan>> {
        Err(Error::NeedsMode {
            command: "pin-monitor",
            mode: "location-per-monitor",
        })
    }
}
