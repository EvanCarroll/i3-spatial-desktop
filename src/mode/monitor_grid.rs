//! The monitors arranged on a grid of their own, which a desktop's grid is
//! laid over.

use std::collections::{BTreeMap, VecDeque};

use crate::grid::{Direction, Offset};
use crate::outputs::neighbour;
use crate::world::{Monitor, OutputName};

/// Each monitor's coordinate: its offset from the grid's origin. A monitor
/// without one is not on the grid, and is left alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorGrid {
    coords: BTreeMap<OutputName, Offset>,
}

impl MonitorGrid {
    /// Reads the grid off the physical layout: the primary monitor is the
    /// origin (the top-left one when none is primary), and the others are
    /// found by walking to each neighbour in turn. `overrides` are
    /// coordinates given by hand, which win; a monitor that would land on a
    /// taken coordinate gets none.
    pub fn new(monitors: &[Monitor], overrides: &BTreeMap<OutputName, Offset>) -> Self {
        let mut grid = MonitorGrid {
            coords: BTreeMap::new(),
        };
        let mut queue: VecDeque<&Monitor> = VecDeque::new();
        for monitor in monitors {
            if let Some(&coord) = overrides.get(&monitor.name)
                && grid.place(monitor, coord)
            {
                queue.push_back(monitor);
            }
        }
        let origin = (monitors.iter().find(|m| m.primary))
            .or_else(|| monitors.iter().min_by_key(|m| (m.rect.y, m.rect.x)))
            .filter(|m| !overrides.contains_key(&m.name));
        if let Some(origin) = origin
            && grid.place(origin, Offset::NONE)
        {
            queue.push_front(origin);
        }

        while let Some(monitor) = queue.pop_front() {
            let coord = grid.coords[&monitor.name];
            use Direction::{Down, Left, Right, Up};
            for direction in [Left, Right, Up, Down] {
                let Some(next) = neighbour(monitors, &monitor.rect, direction) else {
                    continue;
                };
                let known =
                    grid.coords.contains_key(&next.name) || overrides.contains_key(&next.name);
                if !known && grid.place(next, coord + direction.step()) {
                    queue.push_back(next);
                }
            }
        }
        grid
    }

    /// Puts a monitor on a coordinate, unless another one has it.
    fn place(&mut self, monitor: &Monitor, coord: Offset) -> bool {
        let free = self.monitor_at(coord).is_none();
        if free {
            self.coords.insert(monitor.name.clone(), coord);
        }
        free
    }

    pub fn coord(&self, monitor: &OutputName) -> Option<Offset> {
        self.coords.get(monitor).copied()
    }

    pub fn monitor_at(&self, coord: Offset) -> Option<&OutputName> {
        (self.coords.iter())
            .find(|(_, at)| **at == coord)
            .map(|(monitor, _)| monitor)
    }

    /// The monitors and their coordinates, nearest the origin first, then
    /// top to bottom and left to right.
    pub fn ordered(&self) -> Vec<(&OutputName, Offset)> {
        let mut monitors: Vec<(&OutputName, Offset)> =
            self.coords.iter().map(|(m, c)| (m, *c)).collect();
        monitors.sort_by_key(|(_, c)| (c.dx.abs() + c.dy.abs(), -c.dy, c.dx));
        monitors
    }
}

#[cfg(test)]
impl MonitorGrid {
    pub fn of(coords: &[(&str, i32, i32)]) -> Self {
        MonitorGrid {
            coords: coords
                .iter()
                .map(|(name, dx, dy)| (OutputName((*name).into()), Offset { dx: *dx, dy: *dy }))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::Rect;

    fn monitor(name: &str, x: i32, y: i32, w: i32, h: i32, primary: bool) -> Monitor {
        Monitor {
            name: OutputName(name.into()),
            rect: Rect {
                x,
                y,
                width: w,
                height: h,
            },
            primary,
        }
    }

    fn overrides(coords: &[(&str, i32, i32)]) -> BTreeMap<OutputName, Offset> {
        MonitorGrid::of(coords).coords
    }

    fn grid(monitors: &[Monitor], by_hand: &[(&str, i32, i32)]) -> MonitorGrid {
        MonitorGrid::new(monitors, &overrides(by_hand))
    }

    #[test]
    fn the_primary_monitor_is_the_origin() {
        let row = [
            monitor("A", 0, 0, 1920, 1080, false),
            monitor("B", 1920, 0, 1920, 1080, true),
            monitor("C", 3840, 0, 1920, 1080, false),
        ];
        assert_eq!(
            grid(&row, &[]),
            MonitorGrid::of(&[("A", -1, 0), ("B", 0, 0), ("C", 1, 0)])
        );
        // The laptop panel with an external monitor on either side of it.
        let laptop = |x| monitor("eDP-1", x, 240, 1920, 1200, true);
        assert_eq!(
            grid(&[laptop(2560), monitor("DP-1", 0, 0, 2560, 1440, false)], &[]),
            MonitorGrid::of(&[("DP-1", -1, 0), ("eDP-1", 0, 0)])
        );
        assert_eq!(
            grid(&[laptop(0), monitor("DP-1", 1920, 0, 2560, 1440, false)], &[]),
            MonitorGrid::of(&[("eDP-1", 0, 0), ("DP-1", 1, 0)])
        );
    }

    #[test]
    fn without_a_primary_the_top_left_monitor_is() {
        let stacked = [
            monitor("Bottom", 320, 1440, 1920, 1080, false),
            monitor("Top", 0, 0, 2560, 1440, false),
            monitor("Side", 2560, 600, 1080, 1920, false),
        ];
        assert_eq!(
            grid(&stacked, &[]),
            MonitorGrid::of(&[("Top", 0, 0), ("Bottom", 0, -1), ("Side", 1, 0)])
        );
    }

    #[test]
    fn coordinates_given_by_hand_win() {
        let row = [
            monitor("A", 0, 0, 1920, 1080, false),
            monitor("B", 1920, 0, 1920, 1080, true),
            monitor("C", 3840, 0, 1920, 1080, false),
        ];
        // One monitor put elsewhere: the rest keep their places.
        assert_eq!(
            grid(&row, &[("C", 0, 1)]),
            MonitorGrid::of(&[("A", -1, 0), ("B", 0, 0), ("C", 0, 1)])
        );
        // The primary moved: its neighbours follow it.
        assert_eq!(
            grid(&row, &[("B", 1, 0)]),
            MonitorGrid::of(&[("A", 0, 0), ("B", 1, 0), ("C", 2, 0)])
        );
        // Two monitors with nothing at the origin.
        let pair = [
            monitor("L", 0, 0, 1920, 1080, true),
            monitor("R", 1920, 0, 1920, 1080, false),
        ];
        assert_eq!(
            grid(&pair, &[("L", -1, 0), ("R", 1, 0)]),
            MonitorGrid::of(&[("L", -1, 0), ("R", 1, 0)])
        );
        // A monitor that is not there is no trouble.
        assert_eq!(
            grid(&pair, &[("Gone", 5, 5)]),
            MonitorGrid::of(&[("L", 0, 0), ("R", 1, 0)])
        );
    }

    #[test]
    fn a_taken_coordinate_leaves_a_monitor_off_the_grid() {
        let row = [
            monitor("A", 0, 0, 1920, 1080, true),
            monitor("B", 1920, 0, 1920, 1080, false),
            monitor("C", 3840, 0, 1920, 1080, false),
        ];
        // C is put where B would be: B gets no coordinate.
        let g = grid(&row, &[("C", 1, 0)]);
        assert_eq!(g, MonitorGrid::of(&[("A", 0, 0), ("C", 1, 0)]));
        assert_eq!(g.coord(&OutputName("B".into())), None);
        // Two by hand on one coordinate: the first keeps it.
        let g = grid(&row, &[("B", 2, 2), ("C", 2, 2)]);
        assert_eq!(g, MonitorGrid::of(&[("A", 0, 0), ("B", 2, 2)]));
    }

    #[test]
    fn lookups() {
        let g = MonitorGrid::of(&[("L", -1, 0), ("C", 0, 0), ("R", 1, 0), ("T", 0, 1)]);
        assert_eq!(g.monitor_at(Offset { dx: 1, dy: 0 }).unwrap().as_str(), "R");
        assert_eq!(g.monitor_at(Offset { dx: 5, dy: 0 }), None);
        let order: Vec<&str> = g.ordered().iter().map(|(m, _)| m.as_str()).collect();
        assert_eq!(order, ["C", "T", "L", "R"]);
    }
}
