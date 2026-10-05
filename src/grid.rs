//! Places and steps on a desktop's grid. A *desktop* is an i3-spatial-desktop
//! abstraction (i3 has no such thing) made of an *origin workspace* and
//! *satellite workspaces* at grid locations around it. The origin is at
//! (0, 0) and up is +y.

use std::fmt;
use std::ops::{Add, Sub};

use clap::ValueEnum;

/// A place on a desktop's grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Location {
    pub x: i32,
    pub y: i32,
}

/// Where a desktop's origin workspace is.
pub const ORIGIN: Location = Location::new(0, 0);

impl Location {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Order on the bar: the origin's row first, then rows by distance from
    /// it (above before below), each left to right. A plus gives
    /// `← | origin | → | ↑ | ↓`.
    pub fn bar_order(self) -> (u32, i32) {
        let row = match self.y {
            0 => 0,
            y if y > 0 => 2 * y.unsigned_abs() - 1,
            y => 2 * y.unsigned_abs(),
        };
        (row, self.x)
    }
}

/// `(x,y)`, as written in workspace names and messages.
impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({},{})", self.x, self.y)
    }
}

/// A displacement: one arrow press, or how far a monitor sits from the
/// origin of the monitor grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Offset {
    pub dx: i32,
    pub dy: i32,
}

impl Offset {
    pub const NONE: Offset = Offset { dx: 0, dy: 0 };
}

impl Add for Offset {
    type Output = Offset;

    fn add(self, other: Offset) -> Offset {
        Offset {
            dx: self.dx.saturating_add(other.dx),
            dy: self.dy.saturating_add(other.dy),
        }
    }
}

impl Add<Offset> for Location {
    type Output = Location;

    fn add(self, offset: Offset) -> Location {
        Location::new(
            self.x.saturating_add(offset.dx),
            self.y.saturating_add(offset.dy),
        )
    }
}

impl Sub<Offset> for Location {
    type Output = Location;

    fn sub(self, offset: Offset) -> Location {
        Location::new(
            self.x.saturating_sub(offset.dx),
            self.y.saturating_sub(offset.dy),
        )
    }
}

impl Sub for Location {
    type Output = Offset;

    fn sub(self, other: Location) -> Offset {
        Offset {
            dx: self.x.saturating_sub(other.x),
            dy: self.y.saturating_sub(other.y),
        }
    }
}

/// An arrow key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    pub fn step(self) -> Offset {
        let (dx, dy) = match self {
            Direction::Up => (0, 1),
            Direction::Down => (0, -1),
            Direction::Left => (-1, 0),
            Direction::Right => (1, 0),
        };
        Offset { dx, dy }
    }
}

/// The shape of a grid.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum Shape {
    /// Straight lines out from the origin: left, right, up and down.
    #[default]
    Plus,
    /// Every location, diagonals included.
    Square,
}

/// Which locations exist when navigating relative to focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    pub shape: Shape,
    /// How far from the origin the grid reaches; `None` for no limit.
    pub bounds: Option<u32>,
}

impl Grid {
    /// Whether `location` is on the grid. The origin always is.
    pub fn contains(&self, location: Location) -> bool {
        let Location { x, y } = location;
        let shape = self.shape == Shape::Square || x == 0 || y == 0;
        let bounded = self
            .bounds
            .is_none_or(|n| x.unsigned_abs().max(y.unsigned_abs()) <= n);
        shape && bounded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(shape: Shape, bounds: Option<u32>) -> Grid {
        Grid { shape, bounds }
    }

    #[test]
    fn locations_on_the_grid() {
        let plus = grid(Shape::Plus, Some(1));
        assert!(plus.contains(ORIGIN));
        assert!(plus.contains(Location::new(-1, 0)));
        assert!(plus.contains(Location::new(0, -1)));
        assert!(!plus.contains(Location::new(-1, 1)));
        assert!(!plus.contains(Location::new(-2, 0)));
        let square = grid(Shape::Square, Some(2));
        assert!(square.contains(Location::new(-2, 2)));
        assert!(!square.contains(Location::new(-3, 0)));
        assert!(grid(Shape::Plus, None).contains(Location::new(40, 0)));
        assert!(!grid(Shape::Plus, None).contains(Location::new(1, 1)));
    }

    #[test]
    fn places_and_steps() {
        let here = Location::new(-1, 0);
        assert_eq!(here + Direction::Right.step(), ORIGIN);
        assert_eq!(here + Direction::Up.step(), Location::new(-1, 1));
        assert_eq!(here - Direction::Up.step(), Location::new(-1, -1));
        assert_eq!(here - ORIGIN, Offset { dx: -1, dy: 0 });
        assert_eq!(here.to_string(), "(-1,0)");
    }
}
