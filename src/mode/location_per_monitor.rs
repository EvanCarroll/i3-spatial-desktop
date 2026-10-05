//! Each monitor shows its own location of the current desktop: the monitor
//! grid stays pinned to the desktop's origin. Arrows move the focus across
//! the monitors.

use std::collections::BTreeMap;

use crate::command::Command;
use crate::error::{Error, Result};
use crate::grid::{Direction, Location, ORIGIN};
use crate::mode::{MonitorGrid, MonitorMode, Pin, Plan, Route, Wanted};
use crate::naming::{Address, DesktopName, PIN, unpinned};
use crate::navigation::{Destination, Navigation};
use crate::world::{OutputName, World};

pub struct LocationPerMonitor<'a> {
    pub grid: MonitorGrid,
    pub navigation: &'a Navigation,
}

impl LocationPerMonitor<'_> {
    /// Whether a monitor changes with the desktop. One that is pinned or
    /// off the grid does not.
    fn follows(&self, world: &World, monitor: &OutputName) -> bool {
        self.grid.coord(monitor).is_some() && !world.is_pinned(monitor)
    }

    /// The desktop the following monitors show. A monitor that doesn't
    /// follow doesn't lead either: focusing it leaves this as it was.
    fn current_desktop<'w>(&self, world: &'w World) -> Result<&'w DesktopName> {
        let current = world.focused()?;
        if self.follows(world, &current.output) {
            return Ok(&current.address.desktop);
        }
        let leader = (self.grid.ordered().into_iter())
            .filter(|(monitor, _)| self.follows(world, monitor))
            .find_map(|(monitor, _)| world.visible_on(monitor));
        Ok(&leader.unwrap_or(current).address.desktop)
    }

    /// `desktop` laid over the monitors that follow, each at its own
    /// coordinate. A workspace on a monitor that may not change stays
    /// there, and the monitor it would go to keeps what it shows.
    fn project(&self, world: &World, desktop: &DesktopName) -> BTreeMap<OutputName, String> {
        let mut shown = BTreeMap::new();
        for (monitor, coord) in self.grid.ordered() {
            let address = Address {
                desktop: desktop.clone(),
                location: ORIGIN + coord,
            };
            let taken = (world.at(&address))
                .is_some_and(|w| w.visible && !self.follows(world, &w.output));
            if self.follows(world, monitor) && !taken {
                shown.insert(monitor.clone(), world.name_of(&address));
            }
        }
        shown
    }

    /// The route that puts the focus on `target`, changing as little on
    /// screen as it can.
    fn reach(&self, world: &World, target: &Address) -> Result<Option<Route>> {
        let current = world.focused()?;
        // Already on screen: go there.
        if let Some(shown) = world.at(target).filter(|w| w.visible) {
            return Ok(Some(Route::showing(world, BTreeMap::new(), shown.name.clone())));
        }
        let monitor = match self.grid.monitor_at(target.location - ORIGIN) {
            // The monitor whose coordinate this is shows it.
            Some(monitor) if self.follows(world, monitor) => monitor,
            // One that may not change takes the focus as it is.
            Some(monitor) => {
                let there = world.visible_on(monitor);
                return Ok(there.map(|w| Route::showing(world, BTreeMap::new(), w.name.clone())));
            }
            // No monitor has it: the one you are on shows it.
            None if self.follows(world, &current.output) => &current.output,
            None => return Ok(None),
        };
        let name = world.name_of(target);
        let shown = BTreeMap::from([(monitor.clone(), name.clone())]);
        Ok(Some(Route::showing(world, shown, name)))
    }
}

impl MonitorMode for LocationPerMonitor<'_> {
    fn travel(&self, world: &World, to: Destination) -> Result<Option<Route>> {
        let current = world.focused()?;
        let Some(location) = self.navigation.target(current.address.location, to)? else {
            return Ok(None);
        };
        let target = Address {
            desktop: self.current_desktop(world)?.clone(),
            location,
        };
        self.reach(world, &target)
    }

    fn switch(&self, world: &World, resolved: &str, at: Option<Location>) -> Result<Option<Route>> {
        let current = world.focused()?;
        if let Some(location) = at.filter(|&location| !self.navigation.grid.contains(location)) {
            return Err(Error::OffGrid(location));
        }
        // An exact satellite name asks for that satellite.
        let named = world.address_of(resolved);
        let target = named.at(at.unwrap_or(named.location));
        let mut shown = self.project(world, &target.desktop);
        let name = world.name_of(&target);

        let pinned = (world.at(&target))
            .filter(|w| w.visible && !self.follows(world, &w.output))
            .map(|w| w.name.clone());
        let focus = if let Some(pinned) = pinned {
            // On screen on a monitor that may not change: go there.
            pinned
        } else if shown.values().any(|shown| *shown == name) {
            name
        } else {
            // No monitor that follows has it. A location that was asked
            // for shows on the monitor you are on; otherwise you stay
            // there. From a monitor that doesn't follow, you move to the
            // first that does.
            let monitor = if self.follows(world, &current.output) {
                Some(&current.output)
            } else {
                (self.grid.ordered().into_iter())
                    .map(|(monitor, _)| monitor)
                    .find(|monitor| shown.contains_key(*monitor))
            };
            let Some(monitor) = monitor.cloned() else {
                return Ok(None);
            };
            if target.location != ORIGIN {
                shown.insert(monitor.clone(), name);
            }
            let Some(focus) = shown.get(&monitor) else {
                return Ok(None);
            };
            focus.clone()
        };
        Ok(Some(Route::showing(world, shown, focus)))
    }

    fn relocate(&self, _world: &World, _side: Direction) -> Result<Option<Plan>> {
        Err(Error::NeedsMode {
            command: "output",
            mode: "desktop-per-monitor",
        })
    }

    /// Hidden workspaces stay where they are.
    fn settled(&self, _desktop: DesktopName, _output: OutputName, focus: String) -> Wanted {
        Wanted {
            focus: Some(focus),
            ..Wanted::default()
        }
    }

    /// Monitors that show another desktop than the focused one come along,
    /// each to its own coordinate.
    fn follow(&self, world: &World) -> Option<Wanted> {
        let current = world.focused().ok()?;
        if !self.follows(world, &current.output) {
            return None;
        }
        let desktop = &current.address.desktop;
        let elsewhere = |monitor: &OutputName| {
            world.visible_on(monitor).filter(|w| w.address.desktop != *desktop)
        };
        let mut shown = self.project(world, desktop);
        // The focused workspace stays on the monitor it is on, unless it is
        // another monitor's own location: then it belongs there, as after
        // a desktop key.
        let home = (self.grid.monitor_at(current.address.location - ORIGIN))
            .filter(|home| self.follows(world, home) && **home != current.output);
        match home {
            Some(_) => shown.retain(|monitor, name| {
                world.visible_on(monitor).is_none_or(|w| w.name != *name)
            }),
            None => shown.retain(|monitor, name| {
                *monitor != current.output && *name != current.name && elsewhere(monitor).is_some()
            }),
        }
        // From here, back and forth leads to the desktop they are leaving,
        // on the monitor the focus is on.
        let coord = self.grid.coord(home.unwrap_or(&current.output))?;
        let back = shown.keys().find_map(elsewhere).map(|leaving| {
            world.name_of(&Address {
                desktop: leaving.address.desktop.clone(),
                location: ORIGIN + coord,
            })
        });
        (!shown.is_empty()).then(|| Wanted {
            shown,
            focus: Some(current.name.clone()),
            back,
            ..Wanted::default()
        })
    }

    /// Pinning is a rename: the marker goes on the workspace the monitor
    /// shows. Let go, the monitor rejoins the desktop the others show.
    fn pin(&self, world: &World, pin: Pin) -> Result<Option<Plan>> {
        let current = world.focused()?;
        let (plain, pinned) = unpinned(&current.name);
        let pin = match pin {
            Pin::On => true,
            Pin::Off => false,
            Pin::Toggle => !pinned,
        };
        if pin == pinned {
            return Ok(None);
        }
        let rename = |to: String| Command::Rename {
            from: current.name.clone(),
            to,
        };
        if pin {
            return Ok(Some(Plan {
                pre: vec![rename(format!("{plain}{PIN}"))],
                want: Wanted::default(),
            }));
        }

        let mut want = Wanted::default();
        let others = (self.grid.ordered().into_iter())
            .filter(|(monitor, _)| self.follows(world, monitor))
            .find_map(|(monitor, _)| world.visible_on(monitor));
        if let (Some(leader), Some(coord)) = (others, self.grid.coord(&current.output)) {
            let own = Address {
                desktop: leader.address.desktop.clone(),
                location: ORIGIN + coord,
            };
            if own != current.address {
                let name = world.name_of(&own);
                want.shown.insert(current.output.clone(), name.clone());
                want.focus = Some(name);
            }
        }
        Ok(Some(Plan {
            pre: vec![rename(plain.to_owned())],
            want,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{Grid, Shape};
    use crate::navigation::{OriginHotkey, RelativeTo};
    use Destination::{Origin, Step};
    use Direction::{Down, Left, Right, Up};

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

    /// Three monitors in a row, the middle one at the origin.
    fn three(navigation: &Navigation) -> LocationPerMonitor<'_> {
        LocationPerMonitor {
            grid: MonitorGrid::of(&[("left", -1, 0), ("centre", 0, 0), ("right", 1, 0)]),
            navigation,
        }
    }

    fn want(shown: &[(&str, &str)], focus: &str) -> Wanted {
        Wanted {
            shown: shown
                .iter()
                .map(|(monitor, name)| (OutputName((*monitor).into()), (*name).to_owned()))
                .collect(),
            focus: Some(focus.to_owned()),
            ..Wanted::default()
        }
    }

    /// Where a route leads; what back and forth is pointed at is checked
    /// on its own.
    fn wanted(route: Result<Option<Route>>) -> Wanted {
        Wanted {
            back: None,
            ..route.unwrap().unwrap().plan.want
        }
    }

    #[test]
    fn a_desktop_key_changes_every_monitor_and_lands_on_the_origin() {
        let navigation = classic();
        let mode = three(&navigation);
        for world in [
            "left: >4: ←< / centre: [4] / right: [4: →]",
            "left: [4: ←] / centre: >4< / right: [4: →]",
            "left: [4: ←] / centre: [4] / right: >4: →<",
        ] {
            assert_eq!(
                wanted(mode.switch(&World::of(world), "5", None)),
                want(
                    &[("left", "5: ←"), ("centre", "5"), ("right", "5: →")],
                    "5"
                ),
                "{world}"
            );
        }
    }

    #[test]
    fn arrows_move_the_focus_between_monitors() {
        let navigation = classic();
        let mode = three(&navigation);
        let world = World::of("left: [4: ←] / centre: >4< / right: [4: →]");
        assert_eq!(wanted(mode.travel(&world, Step(Left))), want(&[], "4: ←"));
        assert_eq!(wanted(mode.travel(&world, Step(Right))), want(&[], "4: →"));
        // Down is the origin hotkey.
        let world = World::of("left: [4: ←] / centre: [4] / right: >4: →<");
        assert_eq!(wanted(mode.travel(&world, Step(Down))), want(&[], "4"));
        assert_eq!(wanted(mode.travel(&world, Origin)), want(&[], "4"));
        assert_eq!(mode.travel(&world, Step(Right)).unwrap(), None);
    }

    #[test]
    fn a_location_no_monitor_has_shows_on_yours() {
        let navigation = classic();
        let mode = three(&navigation);
        let world = World::of("left: [4: ←] / centre: >4< / right: [4: →]");
        assert_eq!(
            wanted(mode.travel(&world, Step(Up))),
            want(&[("centre", "4: ↑")], "4: ↑")
        );
        // On another monitor, that one shows it.
        let world = World::of("left: >4: ←< / centre: [4] / right: [4: →]");
        assert_eq!(
            wanted(mode.travel(&world, Step(Up))),
            want(&[("left", "4: ↑")], "4: ↑")
        );
        // Once it is on screen, the focus goes to it wherever it is.
        let world = World::of("left: >4: ←< / centre: [4: ↑], 4 / right: [4: →]");
        assert_eq!(wanted(mode.travel(&world, Step(Up))), want(&[], "4: ↑"));
        // Navigating back gives the monitor its own location again.
        let world = World::of("left: [4: ←] / centre: >4: ↑<, 4 / right: [4: →]");
        assert_eq!(
            wanted(mode.travel(&world, Step(Down))),
            want(&[("centre", "4")], "4")
        );
        // So does changing desktop.
        assert_eq!(
            wanted(mode.switch(&world, "5", None)),
            want(
                &[("left", "5: ←"), ("centre", "5"), ("right", "5: →")],
                "5"
            )
        );
    }

    #[test]
    fn a_location_can_be_asked_for() {
        let navigation = classic();
        let mode = three(&navigation);
        let world = World::of("left: [4: ←] / centre: [4] / right: >4: →<");
        // One a monitor has: the focus goes there.
        let at = Some(Location::new(-1, 0));
        assert_eq!(
            wanted(mode.switch(&world, "5", at)),
            want(
                &[("left", "5: ←"), ("centre", "5"), ("right", "5: →")],
                "5: ←"
            )
        );
        // One no monitor has: yours shows it.
        let at = Some(Location::new(0, 1));
        assert_eq!(
            wanted(mode.switch(&world, "5", at)),
            want(
                &[("left", "5: ←"), ("centre", "5"), ("right", "5: ↑")],
                "5: ↑"
            )
        );
        assert!(matches!(
            mode.switch(&world, "5", Some(Location::new(2, 0))),
            Err(Error::OffGrid(_))
        ));
        // An exact satellite name asks for that satellite.
        let world = World::of("left: [4: ←], 5: ← / centre: [4] / right: >4: →<");
        assert_eq!(
            wanted(mode.switch(&world, "5: ←", None)).focus.as_deref(),
            Some("5: ←")
        );
    }

    /// The first request's example: only (-1,0) and (1,0) have monitors.
    #[test]
    fn two_monitors_with_none_at_the_origin() {
        let navigation = classic();
        let mode = LocationPerMonitor {
            grid: MonitorGrid::of(&[("left", -1, 0), ("right", 1, 0)]),
            navigation: &navigation,
        };
        let world = World::of("left: >4: ←< / right: [4: →]");
        // The origin shows on the monitor you are on.
        assert_eq!(
            wanted(mode.travel(&world, Step(Down))),
            want(&[("left", "4")], "4")
        );
        // A desktop key leaves you where you are.
        assert_eq!(
            wanted(mode.switch(&world, "5", None)),
            want(&[("left", "5: ←"), ("right", "5: →")], "5: ←")
        );
        let world = World::of("left: >4<, 4: ← / right: [4: →]");
        assert_eq!(
            wanted(mode.switch(&world, "5", None)),
            want(&[("left", "5: ←"), ("right", "5: →")], "5: ←")
        );
    }

    #[test]
    fn a_monitor_off_the_grid_is_left_alone() {
        let navigation = classic();
        let mode = LocationPerMonitor {
            grid: MonitorGrid::of(&[("centre", 0, 0), ("right", 1, 0)]),
            navigation: &navigation,
        };
        // `left` shows desktop 9 and has no coordinate.
        let world = World::of("left: >9< / centre: [4] / right: [4: →]");
        // A desktop key switches the others and moves the focus to them.
        assert_eq!(
            wanted(mode.switch(&world, "5", None)),
            want(&[("centre", "5"), ("right", "5: →")], "5")
        );
        // Arrows lead from it to the current desktop, not its own.
        assert_eq!(wanted(mode.travel(&world, Step(Right))), want(&[], "4: →"));
        // An arrow that would change what it shows does nothing.
        assert_eq!(mode.travel(&world, Step(Up)).unwrap(), None);
        // Focusing it does not make its desktop the current one.
        let world = World::of("left: [9] / centre: >4< / right: [4: →]");
        assert_eq!(wanted(mode.travel(&world, Step(Right))), want(&[], "4: →"));
        // A workspace it shows is not taken from it: going there focuses it.
        let world = World::of("left: [5] / centre: >4< / right: [4: →]");
        assert_eq!(
            wanted(mode.switch(&world, "5", None)),
            want(&[("right", "5: →")], "5")
        );
    }

    #[test]
    fn back_and_forth_leads_to_the_desktop_just_left() {
        let navigation = classic();
        let mode = three(&navigation);
        let world = World::of("left: [4: ←] / centre: [4] / right: >4: →<");
        let route = mode.switch(&world, "5", None).unwrap().unwrap();
        assert_eq!(route.plan.want.back.as_deref(), Some("4"));
        // An arrow to a monitor is one step, which i3 remembers itself.
        let route = mode.travel(&world, Step(Left)).unwrap().unwrap();
        assert_eq!(route.plan.want.back, None);
    }

    #[test]
    fn the_other_monitors_follow_the_focus() {
        let navigation = classic();
        let mode = three(&navigation);
        // The centre monitor was switched to desktop 5 by something else.
        let world = World::of("left: [4: ←] / centre: >5<, 4 / right: [4: →]");
        let follow = mode.follow(&world).unwrap();
        assert_eq!(follow.focus.as_deref(), Some("5"));
        assert_eq!(follow.back.as_deref(), Some("4"));
        assert_eq!(
            follow.shown,
            want(&[("left", "5: ←"), ("right", "5: →")], "5").shown
        );
        // A monitor showing another location of the same desktop is fine.
        let world = World::of("left: [5: ↑] / centre: >5< / right: [5: →]");
        assert_eq!(mode.follow(&world), None);
        // So is the focused one showing a location no monitor has.
        let world = World::of("left: >5: ↑< / centre: [5] / right: [5: →]");
        assert_eq!(mode.follow(&world), None);
        // A workspace opened on the wrong monitor for its location goes to
        // its own, as after a desktop key.
        let world = World::of("left: >9<, 4: ← / centre: [4] / right: [4: →]");
        let follow = mode.follow(&world).unwrap();
        assert_eq!(follow.focus.as_deref(), Some("9"));
        assert_eq!(follow.back.as_deref(), Some("4"));
        assert_eq!(
            follow.shown,
            want(
                &[("left", "9: ←"), ("centre", "9"), ("right", "9: →")],
                "9"
            )
            .shown
        );
        // A monitor off the grid does not lead.
        let mode = LocationPerMonitor {
            grid: MonitorGrid::of(&[("centre", 0, 0), ("right", 1, 0)]),
            navigation: &navigation,
        };
        let world = World::of("left: >9< / centre: [4] / right: [4: →]");
        assert_eq!(mode.follow(&world), None);
    }

    /// Following has to come to rest, or the daemon would answer its own
    /// changes without end: one pass settles any world, and a route from a
    /// settled world leaves a settled one.
    #[test]
    fn following_comes_to_rest() {
        let navigation = classic();
        let mode = three(&navigation);
        let worlds = [
            "left: [4: ←] / centre: >4< / right: [4: →]",
            "left: >4: ←< / centre: [4] / right: [4: →]",
            "left: [4: ↑], 4: ← / centre: [4] / right: >4: →<",
            // Three desktops at once, as after i3 starts.
            "left: [1] / centre: >2< / right: [3]",
            "left: >1< / centre: [2] / right: [3]",
        ];
        for world in worlds.map(World::of) {
            let settled = match mode.follow(&world) {
                Some(want) => world.after(&want),
                None => world.clone(),
            };
            assert_eq!(mode.follow(&settled), None);
            let routes = [
                mode.travel(&settled, Step(Left)),
                mode.travel(&settled, Step(Up)),
                mode.travel(&settled, Origin),
                mode.switch(&settled, "5", None),
                mode.switch(&settled, "5", Some(Location::new(0, 1))),
            ];
            for route in routes.into_iter().filter_map(|route| route.unwrap()) {
                let after = settled.after(&route.plan.want);
                assert_eq!(mode.follow(&after), None, "{route:?}");
            }
        }
    }

    #[test]
    fn a_pinned_monitor_neither_follows_nor_leads() {
        let navigation = classic();
        let mode = three(&navigation);
        // `left` is pinned on a workspace of desktop 4.
        let world = World::of("left: [4: ← 📌] / centre: >5< / right: [5: →]");
        // It keeps its workspace when the desktop changes.
        assert_eq!(
            wanted(mode.switch(&world, "6", None)),
            want(&[("centre", "6"), ("right", "6: →")], "6")
        );
        assert_eq!(mode.follow(&world), None);
        // Left moves the focus onto it; nothing on screen changes.
        assert_eq!(
            wanted(mode.travel(&world, Step(Left))),
            want(&[], "4: ← 📌")
        );
        // From it, arrows lead back to the current desktop, not its own.
        let world = World::of("left: >4: ← 📌< / centre: [5] / right: [5: →]");
        assert_eq!(wanted(mode.travel(&world, Step(Right))), want(&[], "5: →"));
        assert_eq!(wanted(mode.travel(&world, Step(Down))), want(&[], "5"));
        // An arrow that would change what it shows does nothing.
        assert_eq!(mode.travel(&world, Step(Up)).unwrap(), None);
        // Focusing it does not drag the others to its desktop.
        assert_eq!(mode.follow(&world), None);
        // A desktop key pressed on it switches the others and moves the
        // focus to the origin.
        assert_eq!(
            wanted(mode.switch(&world, "6", None)),
            want(&[("centre", "6"), ("right", "6: →")], "6")
        );
    }

    #[test]
    fn a_pinned_workspace_stays_where_it_is() {
        let navigation = classic();
        let mode = three(&navigation);
        // The origin's own monitor is pinned: you stay on your monitor.
        let world = World::of("left: [4: ←] / centre: [4 📌] / right: >4: →<");
        assert_eq!(
            wanted(mode.switch(&world, "5", None)),
            want(&[("left", "5: ←"), ("right", "5: →")], "5: →")
        );
        // Going to the desktop a pinned workspace belongs to lands on it.
        let world = World::of("left: [5: ←] / centre: [4 📌] / right: >5: →<");
        assert_eq!(
            wanted(mode.switch(&world, "4", None)),
            want(&[("left", "4: ←"), ("right", "4: →")], "4 📌")
        );
        // A pinned workspace that is some other monitor's location is not
        // taken from the monitor it is pinned on.
        let world = World::of("left: [4 📌] / centre: >5< / right: [5: →]");
        assert_eq!(
            wanted(mode.switch(&world, "4", None)),
            want(&[("right", "4: →")], "4 📌")
        );
    }

    #[test]
    fn pinning_is_a_rename() {
        let navigation = classic();
        let mode = three(&navigation);
        let pre = |world: &str, pin| {
            let plan = mode.pin(&World::of(world), pin).unwrap();
            plan.map(|plan| plan.pre.iter().map(Command::to_string).collect::<Vec<_>>())
        };
        let free = "left: >4: ←< / centre: [4] / right: [4: →]";
        let held = "left: >4: ← 📌< / centre: [5] / right: [5: →]";
        let rename = r#"rename workspace "4: ←" to "4: ← 📌""#;
        assert_eq!(pre(free, Pin::On).unwrap(), [rename]);
        assert_eq!(pre(free, Pin::Toggle).unwrap(), [rename]);
        assert_eq!(pre(free, Pin::Off), None);
        assert_eq!(pre(held, Pin::On), None);
        let rename = r#"rename workspace "4: ← 📌" to "4: ←""#;
        assert_eq!(pre(held, Pin::Off).unwrap(), [rename]);
        assert_eq!(pre(held, Pin::Toggle).unwrap(), [rename]);

        // Let go, the monitor rejoins the desktop the others show.
        let plan = mode.pin(&World::of(held), Pin::Off).unwrap().unwrap();
        assert_eq!(plan.want, want(&[("left", "5: ←")], "5: ←"));
        // Nothing to rejoin when it shows its own location of it already.
        let same = "left: >5: ← 📌< / centre: [5] / right: [5: →]";
        let plan = mode.pin(&World::of(same), Pin::Off).unwrap().unwrap();
        assert_eq!(plan.want, Wanted::default());
    }

    #[test]
    fn output_needs_the_other_mode() {
        let navigation = classic();
        let world = World::of("left: >4<");
        assert!(matches!(
            three(&navigation).relocate(&world, Right),
            Err(Error::NeedsMode { .. })
        ));
    }
}
