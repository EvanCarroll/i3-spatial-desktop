//! Where the arrows lead on a desktop's grid.

use clap::ValueEnum;

use crate::error::{Error, Result};
use crate::grid::{Direction, Grid, Location, ORIGIN, Shape};

/// Which arrow, if any, always returns to the origin.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum OriginHotkey {
    Up,
    Down,
    #[default]
    None,
}

/// What the arrows are relative to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum RelativeTo {
    /// Each arrow always leads to the same satellite, one step from the
    /// origin.
    #[default]
    Origin,
    /// Arrows step from the focused workspace across the grid.
    Focus,
}

impl RelativeTo {
    /// The default for a grid: `focus` when it reaches past the four
    /// satellites one step away, since relative to the origin each arrow
    /// only reaches those.
    pub fn implied_by(grid: &Grid) -> Self {
        if grid.shape == Shape::Square || grid.bounds != Some(1) {
            RelativeTo::Focus
        } else {
            RelativeTo::Origin
        }
    }
}

/// Where `focus` and `move` go on the current desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    /// Wherever the arrow leads under the navigation options.
    Step(Direction),
    /// The origin workspace, from anywhere on the desktop.
    Origin,
    /// An explicit location, which must be on the grid.
    At(Location),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Navigation {
    pub relative_to: RelativeTo,
    pub origin_hotkey: OriginHotkey,
    pub grid: Grid,
}

impl Navigation {
    /// Whether `direction` is the arrow that returns to the origin.
    pub fn is_hotkey(&self, direction: Direction) -> bool {
        matches!(
            (self.origin_hotkey, direction),
            (OriginHotkey::Up, Direction::Up) | (OriginHotkey::Down, Direction::Down)
        )
    }

    /// Where `to` leads from `here`, or `None` when that is nowhere new.
    pub fn target(&self, here: Location, to: Destination) -> Result<Option<Location>> {
        let next = match to {
            // The origin hotkey doesn't matter here; it only stops the
            // arrows from stepping that way.
            Destination::At(location) if self.grid.contains(location) => location,
            Destination::At(location) => return Err(Error::OffGrid(location)),
            Destination::Origin => ORIGIN,
            Destination::Step(direction) if self.is_hotkey(direction) => ORIGIN,
            Destination::Step(direction) => match self.relative_to {
                RelativeTo::Origin => ORIGIN + direction.step(),
                RelativeTo::Focus => {
                    let next = here + direction.step();
                    if !self.grid.contains(next) {
                        return Ok(None);
                    }
                    next
                }
            },
        };
        Ok((next != here).then_some(next))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::naming::{Naming, RenderLocation};
    use Destination::{Origin, Step};
    use Direction::{Down, Left, Right, Up};

    fn navigation(
        origin_hotkey: OriginHotkey,
        relative_to: RelativeTo,
        shape: Shape,
        bounds: Option<u32>,
    ) -> Navigation {
        Navigation {
            relative_to,
            origin_hotkey,
            grid: Grid { shape, bounds },
        }
    }

    /// The example configuration: fixed satellites, Down returns.
    fn classic() -> Navigation {
        navigation(
            OriginHotkey::Down,
            RelativeTo::Origin,
            Shape::Plus,
            Some(1),
        )
    }

    fn focus(shape: Shape, bounds: Option<u32>) -> Navigation {
        navigation(OriginHotkey::None, RelativeTo::Focus, shape, bounds)
    }

    /// The workspace an arrow leads to from workspace `current`; `current`
    /// itself when there is nowhere to go.
    fn go_in(names: &Naming, n: &Navigation, current: &str, to: Destination) -> String {
        let (origin, here) = names.split(current);
        let location = n.target(here, to).unwrap().unwrap_or(here);
        names.render(origin, location)
    }

    fn go(n: &Navigation, current: &str, to: Destination) -> String {
        go_in(&Naming::classic(), n, current, to)
    }

    /// Walks `keys` from `start`, returning every workspace visited.
    fn walk_in(names: &Naming, n: &Navigation, start: &str, keys: &[Direction]) -> Vec<String> {
        keys.iter()
            .scan(start.to_owned(), |at, &key| {
                *at = go_in(names, n, at, Step(key));
                Some(at.clone())
            })
            .collect()
    }

    fn walk(n: &Navigation, start: &str, keys: &[Direction]) -> Vec<String> {
        walk_in(&Naming::classic(), n, start, keys)
    }

    #[test]
    fn origin_navigation_with_down_hotkey() {
        let n = classic();
        assert_eq!(go(&n, "6", Step(Up)), "6: ↑");
        assert_eq!(go(&n, "6", Step(Left)), "6: ←");
        assert_eq!(go(&n, "6", Step(Right)), "6: →");
        assert_eq!(go(&n, "6", Step(Down)), "6");
        assert_eq!(go(&n, "mail", Step(Up)), "mail: ↑");
        // Fixed, not cumulative.
        assert_eq!(go(&n, "6: →", Step(Right)), "6: →");
        assert_eq!(go(&n, "6: →", Step(Left)), "6: ←");
        assert_eq!(go(&n, "6: ↑", Step(Down)), "6");
        assert_eq!(go(&n, "6: ←", Origin), "6");
    }

    #[test]
    fn origin_navigation_without_hotkey() {
        let n = navigation(
            OriginHotkey::None,
            RelativeTo::Origin,
            Shape::Plus,
            Some(1),
        );
        assert_eq!(go(&n, "6", Step(Down)), "6: ↓");
        assert_eq!(go(&n, "6: ↓", Step(Down)), "6: ↓");
        assert_eq!(go(&n, "6: ↓", Step(Up)), "6: ↑");
        assert_eq!(go(&n, "6: ↓", Origin), "6");
    }

    #[test]
    fn origin_navigation_with_up_hotkey() {
        let n = navigation(OriginHotkey::Up, RelativeTo::Origin, Shape::Plus, Some(1));
        assert_eq!(go(&n, "6", Step(Up)), "6");
        assert_eq!(go(&n, "6: ←", Step(Up)), "6");
        assert_eq!(go(&n, "6", Step(Down)), "6: ↓");
    }

    #[test]
    fn focus_plus() {
        let n = focus(Shape::Plus, Some(1));
        assert_eq!(
            walk(&n, "6", &[Left, Left, Right, Right]),
            ["6: ←", "6: ←", "6", "6: →"]
        );
        assert_eq!(
            walk(&n, "6", &[Down, Down, Up, Up, Up]),
            ["6: ↓", "6: ↓", "6", "6: ↑", "6: ↑"]
        );
        // Off the plus.
        assert_eq!(walk(&n, "6: ←", &[Up, Down]), ["6: ←", "6: ←"]);
    }

    #[test]
    fn focus_square() {
        let n = focus(Shape::Square, Some(1));
        assert_eq!(
            walk(
                &n,
                "6",
                &[Left, Up, Right, Right, Down, Down, Left, Left, Up, Right]
            ),
            [
                "6: ←", "6: ↖", "6: ↑", "6: ↗", "6: →", "6: ↘", "6: ↓", "6: ↙", "6: ←", "6"
            ]
        );
        assert_eq!(walk(&n, "6: ↖", &[Up, Left]), ["6: ↖", "6: ↖"]);
    }

    #[test]
    fn hotkey_captures_its_arrow() {
        let n = navigation(OriginHotkey::Down, RelativeTo::Focus, Shape::Square, None);
        assert_eq!(go(&n, "6: ↖", Step(Down)), "6");
        assert_eq!(go(&n, "6: →3↑2", Step(Down)), "6");
        // A stale satellite below the origin can still walk around.
        assert_eq!(go(&n, "6: ↙", Step(Up)), "6: ←");
    }

    #[test]
    fn bounded_grids() {
        let n = focus(Shape::Plus, Some(3));
        assert_eq!(
            walk(&n, "6", &[Left, Left, Left, Left]),
            ["6: ←", "6: ←2", "6: ←3", "6: ←3"]
        );
        assert_eq!(go(&n, "6: ←2", Step(Up)), "6: ←2");
        let n = focus(Shape::Square, Some(2));
        assert_eq!(
            walk(&n, "6", &[Left, Left, Up, Up, Up]),
            ["6: ←", "6: ←2", "6: ←2↑", "6: ←2↑2", "6: ←2↑2"]
        );
    }

    #[test]
    fn unbounded_grids() {
        let n = focus(Shape::Plus, None);
        let keys = [Left; 12];
        assert_eq!(walk(&n, "6", &keys).last().unwrap(), "6: ←12");
        assert_eq!(go(&n, "6: ←12", Step(Up)), "6: ←12");
        let n = focus(Shape::Square, None);
        assert_eq!(
            walk(&n, "6", &[Left, Left, Left, Up, Up]).last().unwrap(),
            "6: ←3↑2"
        );
    }

    #[test]
    fn coordinates_navigation() {
        let mut names = Naming::classic();
        names.render = RenderLocation::Coordinates;
        let n = focus(Shape::Square, None);
        assert_eq!(
            walk_in(&names, &n, "6", &[Left, Left, Up]),
            ["6: (-1,0)", "6: (-2,0)", "6: (-2,1)"]
        );
        assert_eq!(go_in(&names, &n, "6: (-1,0)", Step(Right)), "6");
    }

    #[test]
    fn explicit_locations_obey_the_grid() {
        let at = |n: &Navigation, x, y| n.target(ORIGIN, Destination::At(Location::new(x, y)));
        let n = classic();
        assert_eq!(at(&n, 0, 0).unwrap(), None);
        assert_eq!(at(&n, -1, 0).unwrap(), Some(Location::new(-1, 0)));
        // The hotkey blocks the arrow, not the location.
        assert_eq!(at(&n, 0, -1).unwrap(), Some(Location::new(0, -1)));
        assert!(matches!(at(&n, -1, 1), Err(Error::OffGrid(_))));
        assert!(matches!(at(&n, -2, 0), Err(Error::OffGrid(_))));
        let n = focus(Shape::Square, Some(2));
        assert!(at(&n, -2, 2).is_ok());
        assert!(at(&n, -3, 0).is_err());
        assert!(at(&focus(Shape::Plus, None), 40, 0).is_ok());
        assert!(at(&focus(Shape::Plus, None), 1, 1).is_err());
    }
}
