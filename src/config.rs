//! The settings every command runs under.

use std::collections::BTreeMap;

use clap::ValueEnum;

use crate::grid::Offset;
use crate::mode::MultiMonitorMode;
use crate::naming::Naming;
use crate::navigation::{Navigation, RelativeTo};
use crate::world::OutputName;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub naming: Naming,
    pub navigation: Navigation,
    pub monitors: Monitors,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Monitors {
    pub mode: MultiMonitorMode,
    /// Places on the monitor grid given by hand, which override the
    /// physical layout.
    pub locations: BTreeMap<OutputName, Offset>,
}

impl Config {
    /// These settings as command line flags, the inverse of parsing them.
    pub fn to_args(&self) -> Vec<String> {
        let a = &self.naming.arrows;
        let navigation = &self.navigation;
        let mut args: Vec<String> = [
            ("--sep", self.naming.sep.as_str()),
            ("--up", &a.up),
            ("--down", &a.down),
            ("--left", &a.left),
            ("--right", &a.right),
            ("--up-left", &a.up_left),
            ("--up-right", &a.up_right),
            ("--down-left", &a.down_left),
            ("--down-right", &a.down_right),
        ]
        .into_iter()
        .flat_map(|(flag, value)| [flag.to_owned(), value.to_owned()])
        .collect();
        args.extend([
            "--render-workspace-location".to_owned(),
            value_name(&self.naming.render),
            "--desktop-origin-hotkey".to_owned(),
            value_name(&navigation.origin_hotkey),
            "--grid".to_owned(),
            value_name(&navigation.grid.shape),
        ]);
        // Only when it isn't implied, so a binding that changes the grid
        // still gets the navigation that grid implies.
        if navigation.relative_to != RelativeTo::implied_by(&navigation.grid) {
            args.extend([
                "--navigation-relative-to".to_owned(),
                value_name(&navigation.relative_to),
            ]);
        }
        match navigation.grid.bounds {
            Some(n) => args.extend(["--grid-bounds".to_owned(), n.to_string()]),
            None => args.push("--no-grid-bounds".to_owned()),
        }
        args.extend([
            "--multimonitor-mode".to_owned(),
            value_name(&self.monitors.mode),
        ]);
        for (monitor, at) in &self.monitors.locations {
            args.extend([
                "--monitor-location".to_owned(),
                format!("{monitor}={},{}", at.dx, at.dy),
            ]);
        }
        args
    }
}

/// A [`ValueEnum`] value as typed on the command line.
fn value_name<T: ValueEnum>(value: &T) -> String {
    value
        .to_possible_value()
        .map(|v| v.get_name().to_owned())
        .unwrap_or_default()
}
