//! The monitors are one window onto the current desktop. An arrow slides
//! the window one location; the focus stays on its monitor. A desktop key
//! or the origin key puts the window back: the monitor grid's origin on
//! the desktop's origin.

use std::collections::BTreeMap;

use crate::error::{Error, Result};
use crate::grid::{Direction, Location, ORIGIN, Offset};
use crate::mode::{MonitorGrid, MonitorMode, Plan, Route, Wanted};
use crate::naming::{Address, DesktopName};
use crate::navigation::{Destination, Navigation};
use crate::world::{OutputName, World};

pub struct Pan<'a> {
    pub grid: MonitorGrid,
    /// Only the origin hotkey matters here: the window can slide anywhere,
    /// whatever the grid's shape and bounds.
    pub navigation: &'a Navigation,
}

impl Pan<'_> {
    /// The route to `desktop` with the monitor grid's origin on `anchor`
    /// and the focus on `monitor`. `None` when that is how things are, or
    /// when `monitor` is off the grid.
    fn route(
        &self,
        world: &World,
        desktop: &DesktopName,
        anchor: Location,
        monitor: &OutputName,
    ) -> Option<Route> {
        let mut shown = BTreeMap::new();
        for (on, coord) in self.grid.ordered() {
            let address = Address {
                desktop: desktop.clone(),
                location: anchor + coord,
            };
            // A workspace on a monitor off the grid stays there.
            let taken = (world.at(&address))
                .is_some_and(|w| w.visible && self.grid.coord(&w.output).is_none());
            if !taken {
                shown.insert(on.clone(), world.name_of(&address));
            }
        }
        let focus = shown.get(monitor)?.clone();
        let there = |(monitor, name): (&OutputName, &String)| {
            world.visible_on(monitor).is_some_and(|w| w.name == *name)
        };
        let settled = world.focused().is_ok_and(|w| w.name == focus) && shown.iter().all(there);
        (!settled).then(|| Route::showing(world, shown, focus))
    }

    /// The window back where a desktop opens: the grid's origin on the
    /// desktop's origin, and the focus on the monitor there.
    fn home(&self, world: &World, desktop: &DesktopName, from: &OutputName) -> Option<Route> {
        let monitor = self.grid.monitor_at(Offset::NONE).unwrap_or(from);
        self.route(world, desktop, ORIGIN, monitor)
    }
}

impl MonitorMode for Pan<'_> {
    fn travel(&self, world: &World, to: Destination) -> Result<Option<Route>> {
        let current = world.focused()?;
        let desktop = &current.address.desktop;
        let here = current.address.location;
        let location = match to {
            Destination::Origin => return Ok(self.home(world, desktop, &current.output)),
            Destination::Step(direction) if self.navigation.is_hotkey(direction) => {
                return Ok(self.home(world, desktop, &current.output));
            }
            // Every monitor moves by the same step.
            Destination::Step(direction) => here + direction.step(),
            Destination::At(location) => location,
        };
        // Slide until the monitor you are on shows it.
        Ok(self.grid.coord(&current.output).and_then(|coord| {
            self.route(world, desktop, location - coord, &current.output)
        }))
    }

    fn switch(&self, world: &World, resolved: &str, at: Option<Location>) -> Result<Option<Route>> {
        let current = world.focused()?;
        // An exact satellite name asks for that satellite.
        let named = world.address_of(resolved);
        let location = at.unwrap_or(named.location);
        if location == ORIGIN {
            return Ok(self.home(world, &named.desktop, &current.output));
        }
        Ok(self.grid.coord(&current.output).and_then(|coord| {
            self.route(world, &named.desktop, location - coord, &current.output)
        }))
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

    /// The window forms again around the monitor the focus is on.
    fn follow(&self, world: &World) -> Option<Wanted> {
        let current = world.focused().ok()?;
        let coord = self.grid.coord(&current.output)?;
        let desktop = &current.address.desktop;
        let anchor = current.address.location - coord;
        let mut want = self.route(world, desktop, anchor, &current.output)?.plan.want;
        // From here, back and forth leads to where the window was, on the
        // monitor the focus is on.
        want.back = (self.grid.ordered().into_iter())
            .filter(|(monitor, _)| **monitor != current.output)
            .find_map(|(monitor, at)| Some((world.visible_on(monitor)?, at)))
            .map(|(other, at)| {
                world.name_of(&Address {
                    desktop: other.address.desktop.clone(),
                    location: other.address.location - at + coord,
                })
            })
            .filter(|back| *back != current.name);
        Some(want)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{Grid, Shape};
    use crate::navigation::{OriginHotkey, RelativeTo};
    use Destination::{At, Origin, Step};
    use Direction::{Down, Left, Right, Up};

    fn navigation(origin_hotkey: OriginHotkey) -> Navigation {
        Navigation {
            relative_to: RelativeTo::Origin,
            origin_hotkey,
            grid: Grid {
                shape: Shape::Plus,
                bounds: Some(1),
            },
        }
    }

    /// `left` is the monitor grid's origin, `right` is one to its right.
    fn pair(navigation: &Navigation) -> Pan<'_> {
        Pan {
            grid: MonitorGrid::of(&[("left", 0, 0), ("right", 1, 0)]),
            navigation,
        }
    }

    fn want(left: &str, right: &str, focus: &str) -> Wanted {
        Wanted {
            shown: [("left", left), ("right", right)]
                .into_iter()
                .map(|(monitor, name)| (OutputName(monitor.into()), name.to_owned()))
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

    /// The diagrams from the first request:
    /// `[[L] [C]] R` -> `L [[C] [R]]` -> `[[T] [*]]`.
    #[test]
    fn an_arrow_moves_every_monitor_one_location() {
        let navigation = navigation(OriginHotkey::None);
        let mode = pair(&navigation);
        let world = World::of("left: >4: ←< / right: [4]");
        assert_eq!(
            wanted(mode.travel(&world, Step(Right))),
            want("4", "4: →", "4")
        );
        let world = World::of("left: [4: ←] / right: >4<");
        assert_eq!(
            wanted(mode.travel(&world, Step(Right))),
            want("4", "4: →", "4: →")
        );
        let world = World::of("left: >4< / right: [4: →]");
        assert_eq!(
            wanted(mode.travel(&world, Step(Up))),
            want("4: ↑", "4: ↗", "4: ↑")
        );
    }

    #[test]
    fn the_grids_shape_and_bounds_do_not_apply() {
        let navigation = navigation(OriginHotkey::None);
        let mode = pair(&navigation);
        // Far off a plus of one step.
        let world = World::of("left: >4: →7↑7< / right: [4: →8↑7]");
        assert_eq!(
            wanted(mode.travel(&world, Step(Up))),
            want("4: →7↑8", "4: →8↑8", "4: →7↑8")
        );
        assert_eq!(
            wanted(mode.travel(&world, At(Location::new(-9, -9)))),
            want("4: ←9↓9", "4: ←8↓9", "4: ←9↓9")
        );
    }

    #[test]
    fn a_desktop_key_pins_the_grids_origin_to_the_desktops() {
        let navigation = navigation(OriginHotkey::None);
        let mode = pair(&navigation);
        // Whichever monitor has the focus, it lands on the origin.
        for world in [
            "left: >4: ↑< / right: [4: ↗]",
            "left: [4: ↑] / right: >4: ↗<",
        ] {
            assert_eq!(
                wanted(mode.switch(&World::of(world), "5", None)),
                want("5", "5: →", "5"),
                "{world}"
            );
        }
        // With `right` as the grid's origin instead.
        let mode = Pan {
            grid: MonitorGrid::of(&[("left", -1, 0), ("right", 0, 0)]),
            navigation: &navigation,
        };
        let world = World::of("left: >4: ↑< / right: [4: ↗]");
        assert_eq!(
            wanted(mode.switch(&world, "5", None)),
            want("5: ←", "5", "5")
        );
        // A location that is asked for shows on the monitor you are on.
        assert_eq!(
            wanted(mode.switch(&world, "5", Some(Location::new(0, 1)))),
            want("5: ↑", "5: ↗", "5: ↑")
        );
    }

    #[test]
    fn the_origin_key_puts_the_window_back() {
        let none = navigation(OriginHotkey::None);
        let world = World::of("left: [4: →3↑2] / right: >4: →4↑2<");
        assert_eq!(
            wanted(pair(&none).travel(&world, Origin)),
            want("4", "4: →", "4")
        );
        // The hotkey still takes its arrow.
        let down = navigation(OriginHotkey::Down);
        assert_eq!(
            wanted(pair(&down).travel(&world, Step(Down))),
            want("4", "4: →", "4")
        );
        assert_eq!(
            wanted(pair(&none).travel(&world, Step(Down))),
            want("4: →3↑", "4: →4↑", "4: →4↑")
        );
        // Already there.
        let world = World::of("left: >4< / right: [4: →]");
        assert_eq!(pair(&none).travel(&world, Origin).unwrap(), None);
        assert_eq!(pair(&none).switch(&world, "4", None).unwrap(), None);
    }

    #[test]
    fn back_and_forth_leads_to_where_the_window_was() {
        let navigation = navigation(OriginHotkey::None);
        let mode = pair(&navigation);
        let world = World::of("left: [4: ↑] / right: >4: ↗<");
        // After a step: the workspace this monitor showed.
        let route = mode.travel(&world, Step(Right)).unwrap().unwrap();
        assert_eq!(route.plan.want.back.as_deref(), Some("4: ↗"));
        // After a desktop key: what the monitor the focus lands on showed.
        let route = mode.switch(&world, "5", None).unwrap().unwrap();
        assert_eq!(route.plan.want.back.as_deref(), Some("4: ↑"));
    }

    #[test]
    fn the_window_forms_again_around_the_focus() {
        let navigation = navigation(OriginHotkey::None);
        let mode = pair(&navigation);
        // `left` was switched to another desktop by something else.
        let world = World::of("left: >4: →<, 5 / right: [5: →]");
        let follow = mode.follow(&world).unwrap();
        assert_eq!(
            Wanted {
                back: None,
                ..follow.clone()
            },
            want("4: →", "4: →2", "4: →")
        );
        assert_eq!(follow.back.as_deref(), Some("5"));
        // Nothing to do when the window is whole.
        let world = World::of("left: >4: →< / right: [4: →2]");
        assert_eq!(mode.follow(&world), None);
        // Nor from a monitor off the grid.
        let world = World::of("left: [4] / right: [4: →] / other: >9<");
        assert_eq!(mode.follow(&world), None);
    }

    /// Following has to come to rest, or the daemon would answer its own
    /// changes without end: one pass settles any world, and a route from a
    /// settled world leaves a settled one.
    #[test]
    fn following_comes_to_rest() {
        let navigation = navigation(OriginHotkey::None);
        let mode = pair(&navigation);
        let worlds = [
            "left: >4< / right: [4: →]",
            "left: [4: ←3] / right: >4: ←2<",
            // Two desktops at once, as after i3 starts.
            "left: >1< / right: [2]",
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
                mode.travel(&settled, At(Location::new(3, 3))),
                mode.switch(&settled, "5", None),
            ];
            for route in routes.into_iter().filter_map(|route| route.unwrap()) {
                let after = settled.after(&route.plan.want);
                assert_eq!(mode.follow(&after), None, "{route:?}");
            }
        }
    }

    #[test]
    fn a_monitor_off_the_grid_is_left_alone() {
        let navigation = navigation(OriginHotkey::None);
        let mode = pair(&navigation);
        // `other` shows a workspace the window would slide onto.
        let world = World::of("left: >4: ←< / right: [4] / other: [4: →]");
        let route = mode.travel(&world, Step(Right)).unwrap().unwrap();
        assert_eq!(
            route.plan.want.shown,
            BTreeMap::from([(OutputName("left".into()), "4".to_owned())])
        );
        // Arrows on it do nothing; a desktop key still brings the window home.
        let world = World::of("left: [4: ←] / right: [4] / other: >9<");
        assert_eq!(mode.travel(&world, Step(Left)).unwrap(), None);
        assert_eq!(
            wanted(mode.switch(&world, "5", None)),
            want("5", "5: →", "5")
        );
    }
}
