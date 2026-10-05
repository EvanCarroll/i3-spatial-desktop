//! Finding the neighbouring output in a direction, without wrapping.

use crate::grid::Direction;
use crate::ipc::Rect;
use crate::world::Monitor;

/// Overlap of the half-open ranges `[a, a+al)` and `[b, b+bl)`.
fn overlaps(a: i32, al: i32, b: i32, bl: i32) -> bool {
    a < b + bl && b < a + al
}

/// The nearest monitor strictly on `side` of `current` that shares some row
/// (left/right) or column (up/down) with it. `None` at the edge.
pub fn neighbour<'a>(
    monitors: &'a [Monitor],
    current: &Rect,
    side: Direction,
) -> Option<&'a Monitor> {
    let c = current;
    monitors
        .iter()
        .filter_map(|o| {
            let r = &o.rect;
            let distance = match side {
                Direction::Right if overlaps(c.y, c.height, r.y, r.height) => r.x - (c.x + c.width),
                Direction::Left if overlaps(c.y, c.height, r.y, r.height) => c.x - (r.x + r.width),
                Direction::Down if overlaps(c.x, c.width, r.x, r.width) => r.y - (c.y + c.height),
                Direction::Up if overlaps(c.x, c.width, r.x, r.width) => c.y - (r.y + r.height),
                _ => return None,
            };
            (distance >= 0).then_some((distance, o))
        })
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, o)| o)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::OutputName;

    fn out(name: &str, x: i32, y: i32, w: i32, h: i32) -> Monitor {
        Monitor {
            name: OutputName(name.into()),
            rect: Rect {
                x,
                y,
                width: w,
                height: h,
            },
            primary: false,
        }
    }

    fn pick(monitors: &[Monitor], from: usize, side: Direction) -> Option<&str> {
        neighbour(monitors, &monitors[from].rect, side).map(|o| o.name.as_str())
    }

    #[test]
    fn side_by_side_no_wrap() {
        let o = [
            out("A", 0, 0, 1920, 1080),
            out("B", 1920, 0, 1920, 1080),
            out("C", 3840, 0, 1920, 1080),
        ];
        assert_eq!(pick(&o, 0, Direction::Right), Some("B"));
        assert_eq!(pick(&o, 1, Direction::Right), Some("C"));
        assert_eq!(pick(&o, 1, Direction::Left), Some("A"));
        assert_eq!(pick(&o, 2, Direction::Right), None);
        assert_eq!(pick(&o, 0, Direction::Left), None);
        assert_eq!(pick(&o, 0, Direction::Up), None);
    }

    #[test]
    fn stacked_and_offset() {
        let o = [
            out("Top", 0, 0, 2560, 1440),
            out("Bottom", 320, 1440, 1920, 1080),
            out("Side", 2560, 600, 1080, 1920),
        ];
        assert_eq!(pick(&o, 0, Direction::Down), Some("Bottom"));
        assert_eq!(pick(&o, 1, Direction::Up), Some("Top"));
        assert_eq!(pick(&o, 0, Direction::Right), Some("Side"));
        assert_eq!(pick(&o, 1, Direction::Right), Some("Side"));
        assert_eq!(pick(&o, 2, Direction::Left), Some("Top"));
    }
}
