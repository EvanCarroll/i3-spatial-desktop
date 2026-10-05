//! Workspace names. A satellite's name is `<origin><sep><location>`, with
//! the location rendered as arrows (`4:db: ←`, `4:db: ←2↑`) or coordinates
//! (`4:db: (-1,0)`, `4:db: (-2,1)`). An origin is its plain name (`6`,
//! `mail`, `4:db`).

use std::fmt;

use clap::ValueEnum;

use crate::grid::{Location, ORIGIN};

/// A desktop, named after its origin workspace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DesktopName(pub String);

impl DesktopName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DesktopName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for DesktopName {
    fn from(name: &str) -> Self {
        DesktopName(name.to_owned())
    }
}

/// A place a workspace can be: a desktop, and a location on its grid.
/// Nothing has to exist there yet.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Address {
    pub desktop: DesktopName,
    pub location: Location,
}

impl Address {
    /// The place at `location` on the same desktop.
    pub fn at(&self, location: Location) -> Address {
        Address {
            desktop: self.desktop.clone(),
            location,
        }
    }
}

/// What the name of a pinned workspace ends in. The monitor showing such a
/// workspace keeps it when the desktop changes.
pub const PIN: &str = " 📌";

/// A workspace name without the pin marker, and whether it had one.
pub fn unpinned(name: &str) -> (&str, bool) {
    match name.strip_suffix(PIN) {
        Some(plain) if !plain.is_empty() => (plain, true),
        _ => (name, false),
    }
}

/// How a satellite's location appears in its workspace name.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum RenderLocation {
    /// `←`, `↖`, `←2↑`: arrow names, with a count above one step.
    #[default]
    Arrows,
    /// `(-1,0)`, `(-1,1)`, `(-2,1)`.
    Coordinates,
}

/// Names of the arrows used by [`RenderLocation::Arrows`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arrows {
    pub up: String,
    pub down: String,
    pub left: String,
    pub right: String,
    pub up_left: String,
    pub up_right: String,
    pub down_left: String,
    pub down_right: String,
}

impl Arrows {
    fn diagonals(&self) -> [(&str, Location); 4] {
        [
            (&self.up_left, Location::new(-1, 1)),
            (&self.up_right, Location::new(1, 1)),
            (&self.down_left, Location::new(-1, -1)),
            (&self.down_right, Location::new(1, -1)),
        ]
    }

    fn diagonal(&self, location: Location) -> Option<&str> {
        self.diagonals()
            .into_iter()
            .find(|&(_, at)| at == location)
            .map(|(name, _)| name)
    }

    /// `←`, `↖`, `←2`, `←2↑`, `→↑3`. Unit diagonals use their own arrow.
    fn render(&self, location: Location) -> String {
        if let Some(diagonal) = self.diagonal(location) {
            return diagonal.to_owned();
        }
        let mut out = String::new();
        let mut part = |n: i32, negative: &str, positive: &str| {
            if n != 0 {
                out.push_str(if n < 0 { negative } else { positive });
                if n.abs() > 1 {
                    out.push_str(&n.abs().to_string());
                }
            }
        };
        part(location.x, &self.left, &self.right);
        part(location.y, &self.down, &self.up);
        out
    }

    fn parse(&self, text: &str) -> Option<Location> {
        if let Some(&(_, location)) = self.diagonals().iter().find(|(name, _)| *name == text) {
            return Some(location);
        }
        let mut rest = text;
        let mut step = |negative: &str, positive: &str| -> Option<i32> {
            let sign = if let Some(r) = rest.strip_prefix(negative) {
                rest = r;
                -1
            } else if let Some(r) = rest.strip_prefix(positive) {
                rest = r;
                1
            } else {
                return Some(0);
            };
            let digits = rest
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(rest.len());
            let count = if digits == 0 {
                1
            } else {
                rest[..digits].parse().ok()?
            };
            rest = &rest[digits..];
            Some(sign * count)
        };
        let x = step(&self.left, &self.right)?;
        let y = step(&self.down, &self.up)?;
        let location = Location::new(x, y);
        (rest.is_empty() && location != ORIGIN).then_some(location)
    }
}

fn parse_coordinates(text: &str) -> Option<Location> {
    let (x, y) = text.strip_prefix('(')?.strip_suffix(')')?.split_once(',')?;
    Some(Location::new(x.parse().ok()?, y.parse().ok()?))
}

/// How workspace names are written and read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Naming {
    pub sep: String,
    pub arrows: Arrows,
    pub render: RenderLocation,
}

impl Naming {
    /// A satellite's location as written in its name. Both renderings are
    /// accepted whatever `--render-workspace-location` says, but only in
    /// their canonical form, so every location has exactly one name per
    /// rendering.
    fn parse_location(&self, text: &str) -> Option<Location> {
        let arrows = self
            .arrows
            .parse(text)
            .filter(|&l| self.arrows.render(l) == text);
        let coordinates = parse_coordinates(text).filter(|l| l.to_string() == text);
        arrows.or(coordinates).filter(|&l| l != ORIGIN)
    }

    /// Splits a workspace name into its origin and location; an origin is
    /// at [`ORIGIN`]. A pin marker is not part of either.
    pub fn split<'a>(&self, name: &'a str) -> (&'a str, Location) {
        let (name, _) = unpinned(name);
        name.rsplit_once(self.sep.as_str())
            .filter(|(origin, _)| !origin.is_empty())
            .and_then(|(origin, text)| Some((origin, self.parse_location(text)?)))
            .unwrap_or((name, ORIGIN))
    }

    /// The address a workspace name stands for.
    pub fn parse(&self, name: &str) -> Address {
        let (origin, location) = self.split(name);
        Address {
            desktop: origin.into(),
            location,
        }
    }

    /// The name for `location` on the desktop of `origin`, in the configured
    /// rendering.
    pub fn render(&self, origin: &str, location: Location) -> String {
        if location == ORIGIN {
            return origin.to_owned();
        }
        let text = match self.render {
            RenderLocation::Arrows => self.arrows.render(location),
            RenderLocation::Coordinates => location.to_string(),
        };
        format!("{origin}{}{text}", self.sep)
    }

    /// The name for an address, in the configured rendering.
    pub fn name(&self, address: &Address) -> String {
        self.render(address.desktop.as_str(), address.location)
    }

    /// The default names, for tests.
    #[cfg(test)]
    pub fn classic() -> Self {
        Naming {
            sep: ": ".into(),
            arrows: Arrows {
                up: "↑".into(),
                down: "↓".into(),
                left: "←".into(),
                right: "→".into(),
                up_left: "↖".into(),
                up_right: "↗".into(),
                down_left: "↙".into(),
                down_right: "↘".into(),
            },
            render: RenderLocation::Arrows,
        }
    }
}

/// The workspace number i3/sway derive from a name: its leading digits.
pub fn leading_num(name: &str) -> Option<i32> {
    let end = name
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(name.len());
    name[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i32, y: i32) -> Location {
        Location::new(x, y)
    }

    #[test]
    fn arrows_round_trip() {
        let a = Naming::classic().arrows;
        for x in -4..=4 {
            for y in -4..=4 {
                if at(x, y) != ORIGIN {
                    assert_eq!(a.parse(&a.render(at(x, y))), Some(at(x, y)), "{x},{y}");
                }
            }
        }
        assert_eq!(a.render(at(-1, 0)), "←");
        assert_eq!(a.render(at(-1, 1)), "↖");
        assert_eq!(a.render(at(-3, 0)), "←3");
        assert_eq!(a.render(at(-2, 1)), "←2↑");
        assert_eq!(a.render(at(1, -12)), "→↓12");
    }

    #[test]
    fn coordinates_round_trip() {
        assert_eq!(at(-2, 1).to_string(), "(-2,1)");
        assert_eq!(parse_coordinates("(-2,1)"), Some(at(-2, 1)));
        assert_eq!(parse_coordinates("(-2, 1)"), None);
    }

    #[test]
    fn split_names() {
        let n = Naming::classic();
        assert_eq!(n.split("6"), ("6", ORIGIN));
        assert_eq!(n.split("6: ↑"), ("6", at(0, 1)));
        assert_eq!(n.split("6: www: ←"), ("6: www", at(-1, 0)));
        assert_eq!(n.split("mail: →"), ("mail", at(1, 0)));
        assert_eq!(n.split("6: www"), ("6: www", ORIGIN));
        assert_eq!(n.split("6: ←2↑"), ("6", at(-2, 1)));
        assert_eq!(n.split("4:db: (-1,0)"), ("4:db", at(-1, 0)));
        // A workspace literally named after a satellite is an origin.
        assert_eq!(n.split(": ↑"), (": ↑", ORIGIN));
        assert_eq!(n.split("↑"), ("↑", ORIGIN));
    }

    #[test]
    fn a_pin_marker_is_not_part_of_the_address() {
        let n = Naming::classic();
        assert_eq!(n.split("4:db: ← 📌"), ("4:db", at(-1, 0)));
        assert_eq!(n.split("4:db 📌"), ("4:db", ORIGIN));
        assert_eq!(unpinned("4:db: ← 📌"), ("4:db: ←", true));
        assert_eq!(unpinned("4:db: ←"), ("4:db: ←", false));
        // Only at the very end, and never the whole name.
        assert_eq!(unpinned("4:db 📌 "), ("4:db 📌 ", false));
        assert_eq!(unpinned(PIN), (PIN, false));
    }

    #[test]
    fn split_accepts_only_canonical_names() {
        let n = Naming::classic();
        assert_eq!(n.split("6: ←1"), ("6: ←1", ORIGIN));
        assert_eq!(n.split("6: ←↑"), ("6: ←↑", ORIGIN)); // that's ↖
        assert_eq!(n.split("6: ↑←"), ("6: ↑←", ORIGIN));
        assert_eq!(n.split("6: (0,0)"), ("6: (0,0)", ORIGIN));
        assert_eq!(n.split("6: (-1, 0)"), ("6: (-1, 0)", ORIGIN));
    }

    #[test]
    fn render_location() {
        let mut n = Naming::classic();
        assert_eq!(n.render("4:db", at(-2, 1)), "4:db: ←2↑");
        assert_eq!(n.render("4:db", ORIGIN), "4:db");
        n.render = RenderLocation::Coordinates;
        assert_eq!(n.render("4:db", at(-2, 1)), "4:db: (-2,1)");
        assert_eq!(n.render("6", at(-1, 0)), "6: (-1,0)");
    }

    #[test]
    fn sort_order() {
        let n = Naming::classic();
        let mut names = [
            "6: ↘", "6: ↓", "6: ↖", "6: ↑", "6", "6: →", "6: ↙", "6: ←", "6: ↗", "6: ↑2", "6: ←2",
        ];
        names.sort_by_key(|name| n.parse(name).location.bar_order());
        assert_eq!(
            names,
            [
                "6: ←2", "6: ←", "6", "6: →", "6: ↖", "6: ↑", "6: ↗", "6: ↙", "6: ↓", "6: ↘",
                "6: ↑2"
            ]
        );
    }

    #[test]
    fn leading_numbers() {
        assert_eq!(leading_num("1:be"), Some(1));
        assert_eq!(leading_num("10"), Some(10));
        assert_eq!(leading_num("mail"), None);
        assert_eq!(leading_num(""), None);
    }

    #[test]
    fn desktop_membership() {
        let n = Naming::classic();
        let on_six = |name: &str| n.parse(name).desktop.as_str() == "6";
        assert!(on_six("6"));
        assert!(on_six("6: ↑"));
        assert!(on_six("6: (3,-2)"));
        assert!(!on_six("16: ↑"));
        assert!(!on_six("6: www"));
        assert!(!on_six("7"));
    }
}
