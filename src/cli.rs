use clap::error::ErrorKind;
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};

use crate::config::{Config, Monitors};
use crate::grid::{Direction, Grid, Location, Offset, Shape};
use crate::mode::{MultiMonitorMode, Pin};
use crate::naming::{Arrows, Naming, RenderLocation};
use crate::navigation::{Destination, Navigation, OriginHotkey, RelativeTo};
use crate::world::OutputName;

/// Relative workspaces for i3 and sway.
///
/// A desktop is an origin workspace plus satellite workspaces at grid
/// locations around it, named `<origin><sep><location>`, e.g. `4:db: ←`,
/// `4:db: ←2↑` or `4:db: (-2,1)`. A desktop's workspaces share an output,
/// unless `--multimonitor-mode` spreads it over the monitors.
#[derive(Debug, Parser)]
#[command(version, args_override_self = true)]
pub struct Cli {
    #[command(flatten)]
    pub config: ConfigArgs,

    #[command(subcommand)]
    pub command: Cmd,
}

impl Cli {
    /// The settings and the command, or the error for a combination of
    /// options clap can't reject on its own.
    pub fn into_parts(self) -> Result<(Config, Cmd), clap::Error> {
        Ok((Config::try_from(self.config)?, self.command))
    }
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    /// Separator between the origin's name and the satellite's location.
    #[arg(long, global = true, default_value = ": ", value_parser = non_empty, help_heading = "Workspace names")]
    pub sep: String,

    /// How a satellite's location appears in its name.
    #[arg(
        long,
        global = true,
        value_enum,
        default_value_t,
        help_heading = "Workspace names"
    )]
    pub render_workspace_location: RenderLocation,

    #[arg(long, global = true, default_value = "↑", value_parser = non_empty, help_heading = "Arrow names")]
    pub up: String,

    #[arg(long, global = true, default_value = "↓", value_parser = non_empty, help_heading = "Arrow names")]
    pub down: String,

    #[arg(long, global = true, default_value = "←", value_parser = non_empty, help_heading = "Arrow names")]
    pub left: String,

    #[arg(long, global = true, default_value = "→", value_parser = non_empty, help_heading = "Arrow names")]
    pub right: String,

    #[arg(long, global = true, default_value = "↖", value_parser = non_empty, help_heading = "Arrow names")]
    pub up_left: String,

    #[arg(long, global = true, default_value = "↗", value_parser = non_empty, help_heading = "Arrow names")]
    pub up_right: String,

    #[arg(long, global = true, default_value = "↙", value_parser = non_empty, help_heading = "Arrow names")]
    pub down_left: String,

    #[arg(long, global = true, default_value = "↘", value_parser = non_empty, help_heading = "Arrow names")]
    pub down_right: String,

    /// Arrow that returns to the origin from anywhere on the desktop.
    #[arg(
        long,
        global = true,
        value_enum,
        default_value_t,
        help_heading = "Navigation"
    )]
    pub desktop_origin_hotkey: OriginHotkey,

    /// What arrows move relative to. [default: origin; focus with --grid
    /// square, --grid-bounds above 1 or --no-grid-bounds]
    #[arg(long, global = true, value_enum, help_heading = "Navigation")]
    pub navigation_relative_to: Option<RelativeTo>,

    /// Shape of the grid when navigating relative to focus.
    #[arg(
        long,
        global = true,
        value_enum,
        default_value_t,
        help_heading = "Navigation"
    )]
    pub grid: Shape,

    /// How many steps from the origin the grid reaches.
    #[arg(
        long,
        global = true,
        default_value_t = 1,
        value_parser = clap::value_parser!(u32).range(1..),
        overrides_with = "no_grid_bounds",
        help_heading = "Navigation"
    )]
    pub grid_bounds: u32,

    /// Let the grid reach any distance from the origin.
    #[arg(
        long,
        global = true,
        overrides_with = "grid_bounds",
        help_heading = "Navigation"
    )]
    pub no_grid_bounds: bool,

    /// What each monitor is given to show.
    #[arg(
        long,
        global = true,
        value_enum,
        default_value_t,
        help_heading = "Monitors"
    )]
    pub multimonitor_mode: MultiMonitorMode,

    /// A monitor's place on the monitor grid, e.g. `DP-1=-1,0`, instead of
    /// the one its physical position gives it. The primary monitor is at
    /// `0,0`. May be given once per monitor.
    #[arg(
        long,
        global = true,
        value_name = "NAME=X,Y",
        value_parser = monitor_location,
        help_heading = "Monitors"
    )]
    pub monitor_location: Vec<(OutputName, Offset)>,
}

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Focus a workspace of the current desktop: the one an arrow leads to,
    /// or the one at `--location`.
    Focus {
        #[arg(value_enum, required_unless_present = "location")]
        direction: Option<Arrow>,
        /// A location on the grid instead of an arrow, e.g. `--location=-1,0`.
        #[arg(long, value_name = "X,Y", value_parser = location, allow_hyphen_values = true, conflicts_with = "direction")]
        location: Option<Location>,
    },
    /// Move the focused container to a workspace of the current desktop,
    /// chosen like `focus` (focus stays where it is).
    Move {
        #[arg(value_enum, required_unless_present = "location")]
        direction: Option<Arrow>,
        /// A location on the grid instead of an arrow, e.g. `--location=-1,0`.
        #[arg(long, value_name = "X,Y", value_parser = location, allow_hyphen_values = true, conflicts_with = "direction")]
        location: Option<Location>,
    },
    /// Move the current desktop (all its workspaces) to the neighbouring
    /// output. Does nothing at the edge of the layout. Needs
    /// `--multimonitor-mode=desktop-per-monitor`, the default.
    Output {
        #[arg(value_enum)]
        side: Side,
    },
    /// Focus a desktop's origin: NAME if it exists, else an existing origin
    /// with the same number (`1` finds `1:be`), else the origin of a surviving
    /// satellite (`1:be: ←` → `1:be`), else create NAME. Never lands on a
    /// satellite, unlike `workspace number N`.
    Goto {
        name: String,
        /// The desktop's workspace at this location instead of its origin.
        #[arg(long, value_name = "X,Y", value_parser = location, allow_hyphen_values = true)]
        location: Option<Location>,
    },
    /// Move the focused container to a desktop's origin, resolved like
    /// `goto` (focus stays where it is).
    Send {
        name: String,
        /// The desktop's workspace at this location instead of its origin.
        #[arg(long, value_name = "X,Y", value_parser = location, allow_hyphen_values = true)]
        location: Option<Location>,
    },
    /// Rename the current desktop: its origin and every satellite, so
    /// `4:Foo: ↑` follows `4:Foo` to `4:Bar: ↑`. The number stays: `Bar`
    /// on desktop 4 means `4:Bar`, and `7:Bar` is refused.
    RenameDesktop { name: String },
    /// Pin the focused monitor to the workspace it shows, so that changing
    /// desktop leaves it alone, or let it follow again. A pin lasts while
    /// its workspace is on screen. Needs
    /// `--multimonitor-mode=location-per-monitor`.
    PinMonitor {
        #[arg(value_enum, default_value_t)]
        state: Pin,
    },
    /// Run in the background, executing key bindings of the form
    /// `nop i3-spatial-desktop <args>` and keeping desktops settled. Start it with
    /// `exec_always`; a new daemon replaces the running one.
    Daemon,
}

/// An arrow key, or the origin itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Arrow {
    Up,
    Down,
    Left,
    Right,
    /// The origin workspace, from anywhere on the desktop.
    Origin,
}

impl From<Arrow> for Destination {
    fn from(arrow: Arrow) -> Self {
        match arrow {
            Arrow::Up => Destination::Step(Direction::Up),
            Arrow::Down => Destination::Step(Direction::Down),
            Arrow::Left => Destination::Step(Direction::Left),
            Arrow::Right => Destination::Step(Direction::Right),
            Arrow::Origin => Destination::Origin,
        }
    }
}

/// Where `focus` and `move` were asked to go. clap guarantees exactly one
/// of the two.
pub fn destination(direction: Option<Arrow>, location: Option<Location>) -> Destination {
    match (location, direction) {
        (Some(location), _) => Destination::At(location),
        (None, Some(arrow)) => arrow.into(),
        (None, None) => Destination::Origin,
    }
}

/// A side of the focused output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Side {
    Left,
    Right,
    Up,
    Down,
}

impl From<Side> for Direction {
    fn from(side: Side) -> Self {
        match side {
            Side::Left => Direction::Left,
            Side::Right => Direction::Right,
            Side::Up => Direction::Up,
            Side::Down => Direction::Down,
        }
    }
}

/// `X,Y`, e.g. `-1,0`.
fn location(s: &str) -> Result<Location, String> {
    let (x, y) = s
        .split_once(',')
        .ok_or_else(|| format!("expected X,Y, e.g. -1,0; got {s:?}"))?;
    let num = |n: &str| {
        n.trim()
            .parse::<i32>()
            .map_err(|e| format!("{n:?} is not a whole number: {e}"))
    };
    Ok(Location::new(num(x)?, num(y)?))
}

/// `NAME=X,Y`, e.g. `DP-1=-1,0`.
fn monitor_location(s: &str) -> Result<(OutputName, Offset), String> {
    let (name, at) = s
        .rsplit_once('=')
        .filter(|(name, _)| !name.is_empty())
        .ok_or_else(|| format!("expected NAME=X,Y, e.g. DP-1=-1,0; got {s:?}"))?;
    let Location { x, y } = location(at)?;
    Ok((OutputName(name.to_owned()), Offset { dx: x, dy: y }))
}

fn non_empty(s: &str) -> Result<String, String> {
    if s.is_empty() {
        Err("must not be empty".into())
    } else {
        Ok(s.to_owned())
    }
}

impl TryFrom<ConfigArgs> for Config {
    type Error = clap::Error;

    fn try_from(a: ConfigArgs) -> Result<Self, clap::Error> {
        let grid = Grid {
            shape: a.grid,
            bounds: (!a.no_grid_bounds).then_some(a.grid_bounds),
        };
        let implied = RelativeTo::implied_by(&grid);
        if implied == RelativeTo::Focus && a.navigation_relative_to == Some(RelativeTo::Origin) {
            return Err(Cli::command().error(
                ErrorKind::ArgumentConflict,
                "--grid square, --grid-bounds above 1 and --no-grid-bounds need \
                 --navigation-relative-to focus, its default with them (relative \
                 to the origin, each arrow only reaches the satellite one step away)",
            ));
        }
        Ok(Config {
            naming: Naming {
                sep: a.sep,
                arrows: Arrows {
                    up: a.up,
                    down: a.down,
                    left: a.left,
                    right: a.right,
                    up_left: a.up_left,
                    up_right: a.up_right,
                    down_left: a.down_left,
                    down_right: a.down_right,
                },
                render: a.render_workspace_location,
            },
            navigation: Navigation {
                relative_to: a.navigation_relative_to.unwrap_or(implied),
                origin_hotkey: a.desktop_origin_hotkey,
                grid,
            },
            monitors: Monitors {
                mode: a.multimonitor_mode,
                // A later one for the same monitor wins.
                locations: a.monitor_location.into_iter().collect(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<(Config, Cmd), clap::Error> {
        Cli::try_parse_from(std::iter::once("i3-spatial-desktop").chain(args.iter().copied()))?
            .into_parts()
    }

    fn config(args: &[&str]) -> Config {
        let mut args = args.to_vec();
        args.extend(["focus", "up"]);
        parse(&args).unwrap().0
    }

    fn command(args: &[&str]) -> Cmd {
        parse(args).unwrap().1
    }

    #[test]
    fn defaults() {
        let c = config(&[]);
        assert_eq!(c.naming.render, RenderLocation::Arrows);
        assert_eq!(c.navigation.origin_hotkey, OriginHotkey::None);
        assert_eq!(c.navigation.relative_to, RelativeTo::Origin);
        assert_eq!(c.navigation.grid.shape, Shape::Plus);
        assert_eq!(c.navigation.grid.bounds, Some(1));
        assert_eq!(c.naming.arrows.up_left, "↖");
    }

    #[test]
    fn to_args_round_trips() {
        for args in [
            &[
                "--desktop-origin-hotkey=down",
                "--navigation-relative-to=focus",
                "--grid=square",
                "--grid-bounds=3",
                "--render-workspace-location=coordinates",
                "--up-left",
                "NW",
                "--sep",
                " / ",
            ][..],
            &["--navigation-relative-to=focus", "--no-grid-bounds"],
        ] {
            let c = config(args);
            let again = config(&c.to_args().iter().map(String::as_str).collect::<Vec<_>>());
            assert_eq!(again, c);
        }
    }

    #[test]
    fn monitors() {
        let c = config(&[]);
        assert_eq!(c.monitors, Monitors::default());
        let c = config(&[
            "--multimonitor-mode=location-per-monitor",
            "--monitor-location=DP-1=-1,0",
            "--monitor-location",
            "HDMI-A-1=0,1",
            "--monitor-location=DP-1=1,0",
        ]);
        assert_eq!(c.monitors.mode, MultiMonitorMode::LocationPerMonitor);
        let at = |name: &str| c.monitors.locations[&OutputName(name.into())];
        assert_eq!(at("DP-1"), Offset { dx: 1, dy: 0 });
        assert_eq!(at("HDMI-A-1"), Offset { dx: 0, dy: 1 });
        // The daemon hands them to every binding, which can add its own.
        let mut args = c.to_args();
        assert_eq!(config(&args.iter().map(String::as_str).collect::<Vec<_>>()), c);
        args.extend(["--multimonitor-mode=pan".into(), "--monitor-location=eDP-1=0,0".into()]);
        let binding = config(&args.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(binding.monitors.mode, MultiMonitorMode::Pan);
        assert_eq!(binding.monitors.locations.len(), 3);

        assert!(parse(&["--monitor-location=DP-1", "focus", "up"]).is_err());
        assert!(parse(&["--monitor-location==1,0", "focus", "up"]).is_err());
        assert!(parse(&["--monitor-location=DP-1=1", "focus", "up"]).is_err());
        assert!(parse(&["--multimonitor-mode=sideways", "focus", "up"]).is_err());
    }

    #[test]
    fn grid_bounds() {
        let bounds = |args: &[&str]| config(args).navigation.grid.bounds;
        let focus = "--navigation-relative-to=focus";
        assert_eq!(bounds(&[focus, "--grid-bounds=4"]), Some(4));
        assert_eq!(bounds(&[focus, "--no-grid-bounds"]), None);
        // The last one wins, so a binding can override the daemon.
        assert_eq!(
            bounds(&[focus, "--no-grid-bounds", "--grid-bounds=2"]),
            Some(2)
        );
        assert_eq!(bounds(&[focus, "--grid-bounds=2", "--no-grid-bounds"]), None);
        assert!(parse(&[focus, "--grid-bounds=0", "focus", "up"]).is_err());
    }

    #[test]
    fn beyond_one_step_defaults_to_focus_navigation() {
        let relative_to = |args: &[&str]| config(args).navigation.relative_to;
        assert_eq!(relative_to(&[]), RelativeTo::Origin);
        assert_eq!(relative_to(&["--grid-bounds=1"]), RelativeTo::Origin);
        for extra in ["--grid=square", "--grid-bounds=2", "--no-grid-bounds"] {
            assert_eq!(relative_to(&[extra]), RelativeTo::Focus, "{extra}");
            assert_eq!(
                relative_to(&[extra, "--navigation-relative-to=focus"]),
                RelativeTo::Focus
            );
            // Only an explicit contradiction is an error.
            assert!(parse(&[extra, "--navigation-relative-to=origin", "focus", "up"]).is_err());
        }
    }

    #[test]
    fn binding_can_change_grid_on_top_of_daemon_defaults() {
        let daemon = config(&["--desktop-origin-hotkey=down"]);
        let mut args: Vec<String> = daemon.to_args();
        args.push("--grid=square".into());
        let c = config(&args.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(c.navigation.relative_to, RelativeTo::Focus);
    }

    #[test]
    fn users_daemon_line() {
        let c = config(&["--grid=square", "--desktop-origin-hotkey=down"]);
        assert_eq!(c.navigation.relative_to, RelativeTo::Focus);
        assert_eq!(c.navigation.grid.shape, Shape::Square);
        assert_eq!(c.navigation.origin_hotkey, OriginHotkey::Down);
    }

    #[test]
    fn locations() {
        for args in [
            &["focus", "--location=-1,0"][..],
            &["focus", "--location", "-1,0"],
        ] {
            match command(args) {
                Cmd::Focus {
                    direction: None,
                    location: Some(location),
                } => assert_eq!(location, Location::new(-1, 0)),
                other => panic!("{other:?}"),
            }
        }
        match command(&["goto", "4:db", "--location=2,-1"]) {
            Cmd::Goto {
                location: Some(location),
                ..
            } => assert_eq!(location, Location::new(2, -1)),
            other => panic!("{other:?}"),
        }
        assert!(parse(&["focus"]).is_err());
        assert!(parse(&["focus", "up", "--location=1,0"]).is_err());
        assert!(parse(&["move", "--location=1"]).is_err());
        assert!(parse(&["move", "--location=a,0"]).is_err());
    }

    #[test]
    fn later_flags_override_earlier() {
        let c = config(&["--up", "↑", "--up", "Top"]);
        assert_eq!(c.naming.arrows.up, "Top");
    }
}
