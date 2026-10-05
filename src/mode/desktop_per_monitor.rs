//! The original mode: each monitor walks the desktop it shows, and a
//! desktop's workspaces all live on one monitor.

use crate::command::Command;
use crate::error::{Error, Result};
use crate::grid::{Direction, Location};
use crate::mode::{MonitorMode, Plan, Route, Wanted};
use crate::naming::DesktopName;
use crate::navigation::{Destination, Navigation};
use crate::outputs::neighbour;
use crate::world::{OutputName, World};

pub struct DesktopPerMonitor<'a> {
    pub navigation: &'a Navigation,
}

impl MonitorMode for DesktopPerMonitor<'_> {
    fn travel(&self, world: &World, to: Destination) -> Result<Option<Route>> {
        let current = world.focused()?;
        let Some(location) = self.navigation.target(current.address.location, to)? else {
            return Ok(None);
        };
        let target = world.name_of(&current.address.at(location));
        let want = Wanted::gathered(
            current.address.desktop.clone(),
            current.output.clone(),
            Some(target.clone()),
        );
        Ok(Some(Route {
            plan: Plan {
                pre: vec![Command::Show(target.clone())],
                want,
            },
            target,
        }))
    }

    fn switch(&self, world: &World, resolved: &str, at: Option<Location>) -> Result<Option<Route>> {
        let current = world.focused()?;
        let target = match at {
            // The workspace there under whatever name it has; else the name
            // as it was given.
            None => (world.at(&world.address_of(resolved)))
                .map_or_else(|| resolved.to_owned(), |w| w.name.clone()),
            Some(location) if self.navigation.grid.contains(location) => {
                world.name_of(&world.address_of(resolved).at(location))
            }
            Some(location) => return Err(Error::OffGrid(location)),
        };
        let desktop = world.address_of(&target).desktop;
        let output = world.desktop_output(&desktop).unwrap_or(&current.output);
        // Plain `workspace`, so `workspace_auto_back_and_forth` still
        // applies: asked for the workspace it is on, i3 goes back, and the
        // focus is then left wherever that is.
        let focus = (target != current.name).then(|| target.clone());
        Ok(Some(Route {
            plan: Plan {
                pre: vec![Command::Goto(target.clone())],
                want: Wanted::gathered(desktop, output.clone(), focus),
            },
            target,
        }))
    }

    /// Moves the whole desktop.
    fn relocate(&self, world: &World, side: Direction) -> Result<Option<Plan>> {
        let current = world.focused()?;
        let here = world
            .monitor(&current.output)
            .ok_or_else(|| Error::UnknownOutput(current.output.to_string()))?;
        let Some(there) = neighbour(&world.monitors, &here.rect, side) else {
            return Ok(None);
        };

        let desktop = &current.address.desktop;
        let mut pre: Vec<Command> = world
            .desktop(desktop)
            .filter(|w| !w.focused && w.output != there.name)
            .flat_map(|w| {
                [
                    Command::Show(w.name.clone()),
                    Command::MoveWorkspaceTo(there.name.clone()),
                ]
            })
            .collect();
        // The focused workspace goes last so it ends up focused on `there`.
        if !pre.is_empty() {
            pre.push(Command::Show(current.name.clone()));
        }
        pre.push(Command::MoveWorkspaceTo(there.name.clone()));
        let want = Wanted::gathered(
            desktop.clone(),
            there.name.clone(),
            Some(current.name.clone()),
        );
        Ok(Some(Plan { pre, want }))
    }

    fn settled(&self, desktop: DesktopName, output: OutputName, focus: String) -> Wanted {
        Wanted::gathered(desktop, output, Some(focus))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{Grid, Shape};
    use crate::navigation::{OriginHotkey, RelativeTo};
    use Destination::{Origin, Step};

    fn texts(commands: &[Command]) -> Vec<String> {
        commands.iter().map(Command::to_string).collect()
    }

    /// The example configuration: fixed satellites, Down returns.
    fn classic() -> Navigation {
        Navigation {
            relative_to: RelativeTo::Origin,
            origin_hotkey: OriginHotkey::Down,
            grid: Grid {
                shape: Shape::Plus,
                bounds: Some(1),
            },
        }
    }

    fn gathered(desktop: &str, output: &str, focus: Option<&str>) -> Wanted {
        Wanted::gathered(
            desktop.into(),
            OutputName(output.into()),
            focus.map(str::to_owned),
        )
    }

    #[test]
    fn an_arrow_is_one_command() {
        let navigation = classic();
        let mode = DesktopPerMonitor { navigation: &navigation };
        let world = World::of("fake-0: 4: (-1,0), >4< / fake-1: [6]");
        // An existing workspace is reused under the name it has.
        let route = mode.travel(&world, Step(Direction::Left)).unwrap().unwrap();
        assert_eq!(route.target, "4: (-1,0)");
        assert_eq!(
            texts(&route.plan.pre),
            [r#"workspace --no-auto-back-and-forth "4: (-1,0)""#]
        );
        assert_eq!(route.plan.want, gathered("4", "fake-0", Some("4: (-1,0)")));
        // A new one gets the configured rendering.
        let route = mode.travel(&world, Step(Direction::Up)).unwrap().unwrap();
        assert_eq!(route.target, "4: ↑");
        // Already there.
        assert_eq!(mode.travel(&world, Origin).unwrap(), None);
        assert_eq!(mode.travel(&world, Step(Direction::Down)).unwrap(), None);
    }

    #[test]
    fn a_desktop_key_leads_to_the_desktops_output() {
        let navigation = classic();
        let mode = DesktopPerMonitor { navigation: &navigation };
        let world = World::of("fake-0: [4: ←], 4 / fake-1: >6<");
        let route = mode.switch(&world, "4", None).unwrap().unwrap();
        assert_eq!(texts(&route.plan.pre), [r#"workspace "4""#]);
        assert_eq!(route.plan.want, gathered("4", "fake-0", Some("4")));
        // A location on the desktop.
        let at = Some(Location::new(0, 1));
        let route = mode.switch(&world, "4", at).unwrap().unwrap();
        assert_eq!(route.target, "4: ↑");
        assert_eq!(route.plan.want, gathered("4", "fake-0", Some("4: ↑")));
        // A new desktop opens on the focused output.
        let route = mode.switch(&world, "9", None).unwrap().unwrap();
        assert_eq!(route.plan.want, gathered("9", "fake-1", Some("9")));
        // The workspace it is on: i3 may go back, so the focus is left alone.
        let route = mode.switch(&world, "6", None).unwrap().unwrap();
        assert_eq!(texts(&route.plan.pre), [r#"workspace "6""#]);
        assert_eq!(route.plan.want, gathered("6", "fake-1", None));
        // Off the grid.
        let off = Some(Location::new(2, 0));
        assert!(matches!(
            mode.switch(&world, "4", off),
            Err(Error::OffGrid(_))
        ));
    }

    #[test]
    fn output_moves_every_workspace_of_the_desktop() {
        let navigation = classic();
        let mode = DesktopPerMonitor { navigation: &navigation };
        let world = World::of("fake-0: 4: ←, >4<, 4: ↑, 5 / fake-1: [6]");
        let plan = mode.relocate(&world, Direction::Right).unwrap().unwrap();
        assert_eq!(
            texts(&plan.pre),
            [
                r#"workspace --no-auto-back-and-forth "4: ←""#,
                r#"move workspace to output "fake-1""#,
                r#"workspace --no-auto-back-and-forth "4: ↑""#,
                r#"move workspace to output "fake-1""#,
                r#"workspace --no-auto-back-and-forth "4""#,
                r#"move workspace to output "fake-1""#,
            ]
        );
        assert_eq!(plan.want, gathered("4", "fake-1", Some("4")));
        // A lone workspace needs no visiting.
        let world = World::of("fake-0: >4<, 5 / fake-1: [6]");
        let plan = mode.relocate(&world, Direction::Right).unwrap().unwrap();
        assert_eq!(texts(&plan.pre), [r#"move workspace to output "fake-1""#]);
        // The edge.
        assert_eq!(mode.relocate(&world, Direction::Left).unwrap(), None);
        assert_eq!(mode.relocate(&world, Direction::Up).unwrap(), None);
    }
}
