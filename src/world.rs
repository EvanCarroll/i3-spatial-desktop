//! i3's state at one moment, parsed once: every workspace with the address
//! its name stands for, and the active monitors.

use std::fmt;

use crate::error::{Error, Result};
use crate::grid::ORIGIN;
use crate::ipc::{self, Connection, Rect};
use crate::naming::{Address, DesktopName, Naming, leading_num, unpinned};

/// An output (a monitor), by its name in i3.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OutputName(pub String);

impl OutputName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OutputName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    /// The name as i3 has it.
    pub name: String,
    /// Leading number of the name, `-1` if there is none.
    pub num: i32,
    pub address: Address,
    pub output: OutputName,
    /// Shown on its output, focused or not.
    pub visible: bool,
    pub focused: bool,
    /// Its name carries the pin marker.
    pub pinned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    pub name: OutputName,
    pub rect: Rect,
    pub primary: bool,
}

#[derive(Debug, Clone)]
pub struct World {
    naming: Naming,
    /// In i3's order.
    pub workspaces: Vec<Workspace>,
    /// The active outputs.
    pub monitors: Vec<Monitor>,
}

impl World {
    pub fn new(naming: &Naming, workspaces: Vec<ipc::Workspace>, outputs: Vec<ipc::Output>) -> Self {
        World {
            workspaces: workspaces
                .into_iter()
                .map(|w| Workspace {
                    address: naming.parse(&w.name),
                    pinned: unpinned(&w.name).1,
                    name: w.name,
                    num: w.num,
                    output: OutputName(w.output),
                    visible: w.visible,
                    focused: w.focused,
                })
                .collect(),
            monitors: outputs
                .into_iter()
                .filter(|o| o.active)
                .map(|o| Monitor {
                    name: OutputName(o.name),
                    rect: o.rect,
                    primary: o.primary,
                })
                .collect(),
            naming: naming.clone(),
        }
    }

    pub fn snapshot(conn: &mut Connection, naming: &Naming) -> Result<Self> {
        Ok(Self::new(naming, conn.workspaces()?, conn.outputs()?))
    }

    pub fn focused(&self) -> Result<&Workspace> {
        self.workspaces
            .iter()
            .find(|w| w.focused)
            .ok_or(Error::NoFocusedWorkspace)
    }

    pub fn named(&self, name: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.name == name)
    }

    /// The address a workspace name stands for, whether it exists or not.
    pub fn address_of(&self, name: &str) -> Address {
        self.naming.parse(name)
    }

    /// The workspace an output shows.
    pub fn visible_on(&self, output: &OutputName) -> Option<&Workspace> {
        self.workspaces
            .iter()
            .find(|w| w.visible && w.output == *output)
    }

    /// The workspace at `address`, if there is one. Two can share an
    /// address (a pinned workspace and one a rule made under its plain
    /// name, or the same location in both renderings): the pinned one
    /// counts, then the one on screen, then the first.
    pub fn at(&self, address: &Address) -> Option<&Workspace> {
        (self.workspaces.iter().rev())
            .filter(|w| w.address == *address)
            .max_by_key(|w| (w.pinned, w.visible))
    }

    /// Whether a monitor is pinned: the workspace it shows carries the
    /// marker.
    pub fn is_pinned(&self, monitor: &OutputName) -> bool {
        self.visible_on(monitor).is_some_and(|w| w.pinned)
    }

    /// The name for `address`: the workspace already there (in whichever
    /// rendering it was created), else a new name.
    pub fn name_of(&self, address: &Address) -> String {
        self.at(address)
            .map_or_else(|| self.naming.name(address), |w| w.name.clone())
    }

    /// The workspaces of a desktop, in i3's order.
    pub fn desktop<'a>(&'a self, desktop: &'a DesktopName) -> impl Iterator<Item = &'a Workspace> {
        self.workspaces
            .iter()
            .filter(move |w| w.address.desktop == *desktop)
    }

    /// The output a desktop lives on, if any of its workspaces exist.
    pub fn desktop_output(&self, desktop: &DesktopName) -> Option<&OutputName> {
        self.workspaces
            .iter()
            .find(|w| w.address.desktop == *desktop)
            .map(|w| &w.output)
    }

    pub fn monitor(&self, name: &OutputName) -> Option<&Monitor> {
        self.monitors.iter().find(|m| m.name == *name)
    }

    /// Resolves a `goto`/`send` argument to a workspace name: `query` itself
    /// if it exists, else an existing origin with the same number (`1`
    /// finds `1:be`), else the origin named by a surviving satellite (`1`
    /// finds `1:be` through `1:be: ←`), else `query`, which i3 will create.
    pub fn resolve(&self, query: &str) -> String {
        if self.named(query).is_some() {
            return query.to_owned();
        }
        let Some(num) = leading_num(query) else {
            return query.to_owned();
        };
        let same_num = || self.workspaces.iter().filter(move |w| w.num == num);
        same_num()
            .find(|w| w.address.location == ORIGIN)
            .or_else(|| same_num().next())
            .map_or_else(|| query.to_owned(), |w| w.address.desktop.0.clone())
    }
}

#[cfg(test)]
impl World {
    /// A world written the way the end-to-end tests print one: outputs
    /// separated by ` / `, each `name: workspace, workspace` in i3's order,
    /// with `>name<` for the focused workspace and `[name]` for a visible
    /// one. The outputs sit side by side, left to right; a `*` after an
    /// output's name makes it the primary.
    pub fn of(spec: &str) -> World {
        let mut workspaces = Vec::new();
        let mut outputs = Vec::new();
        for (index, part) in spec.split(" / ").enumerate() {
            let (output, names) = part.split_once(": ").unwrap_or((part, ""));
            let primary = output.ends_with('*');
            let output = output.trim_end_matches('*');
            outputs.push(ipc::Output {
                name: output.to_owned(),
                active: true,
                primary,
                rect: Rect {
                    x: 1000 * index as i32,
                    y: 0,
                    width: 1000,
                    height: 1000,
                },
            });
            for name in names.split(", ").filter(|name| !name.is_empty()) {
                let focused = name.starts_with('>') && name.ends_with('<');
                let visible = focused || (name.starts_with('[') && name.ends_with(']'));
                let name = if visible {
                    &name[1..name.len() - 1]
                } else {
                    name
                };
                workspaces.push(ipc::Workspace {
                    id: workspaces.len() as u64,
                    name: name.to_owned(),
                    num: leading_num(name).unwrap_or(-1),
                    focused,
                    visible,
                    output: output.to_owned(),
                });
            }
        }
        World::new(&Naming::classic(), workspaces, outputs)
    }
}

#[cfg(test)]
impl World {
    /// The world once `want` holds: each listed monitor shows its
    /// workspace, made there if need be, and the focus is where it should
    /// be.
    pub fn after(&self, want: &crate::mode::Wanted) -> World {
        let mut world = self.clone();
        for (monitor, name) in &want.shown {
            for workspace in &mut world.workspaces {
                if workspace.output == *monitor {
                    workspace.visible = false;
                }
            }
            world.workspaces.retain(|w| w.name != *name);
            world.workspaces.push(Workspace {
                name: name.clone(),
                num: leading_num(name).unwrap_or(-1),
                address: self.address_of(name),
                output: monitor.clone(),
                visible: true,
                focused: false,
                pinned: unpinned(name).1,
            });
        }
        if let Some(focus) = &want.focus {
            for workspace in &mut world.workspaces {
                workspace.focused = workspace.name == *focus;
            }
        }
        world
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::Location;
    use crate::naming::RenderLocation;

    fn address(desktop: &str, x: i32, y: i32) -> Address {
        Address {
            desktop: desktop.into(),
            location: Location::new(x, y),
        }
    }

    #[test]
    fn worlds_from_text() {
        let w = World::of("fake-0: 4: ←, >4<, 4: ↑ / fake-1: [6]");
        let names: Vec<&str> = w.workspaces.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["4: ←", "4", "4: ↑", "6"]);
        assert_eq!(w.focused().unwrap().name, "4");
        let visible = |output: &str| w.visible_on(&OutputName(output.into())).unwrap();
        assert_eq!(visible("fake-0").name, "4");
        assert_eq!(visible("fake-1").name, "6");
        assert_eq!(w.named("6").unwrap().output.as_str(), "fake-1");
        assert_eq!(w.named("4: ←").unwrap().address, address("4", -1, 0));
        assert_eq!(w.monitors.len(), 2);
        assert!(World::of("fake-0: 4").focused().is_err());
    }

    #[test]
    fn names_reuse_any_rendering() {
        let mut w = World::of("fake-0: 6, 6: ←, 7: (1,0)");
        w.naming.render = RenderLocation::Coordinates;
        assert_eq!(w.name_of(&address("6", -1, 0)), "6: ←");
        assert_eq!(w.name_of(&address("6", 1, 0)), "6: (1,0)");
        assert_eq!(w.name_of(&address("7", 1, 0)), "7: (1,0)");
        assert_eq!(w.name_of(&address("6", 0, 0)), "6");
        // Two workspaces at one address: the first in i3's order.
        let w = World::of("fake-0: 6: (-1,0), 6: ←");
        assert_eq!(w.name_of(&address("6", -1, 0)), "6: (-1,0)");
    }

    #[test]
    fn pinned_workspaces() {
        let w = World::of("fake-0: [4: ← 📌], 4 / fake-1: >5<, 6 📌");
        assert!(w.is_pinned(&OutputName("fake-0".into())));
        // A marker on a hidden workspace pins nothing.
        assert!(!w.is_pinned(&OutputName("fake-1".into())));
        assert_eq!(w.named("4: ← 📌").unwrap().address, address("4", -1, 0));
        assert_eq!(w.name_of(&address("4", -1, 0)), "4: ← 📌");
        // With a twin under the plain name, the pinned one counts.
        let w = World::of("fake-0: 10:com, [10:com 📌] / fake-1: >5<");
        assert_eq!(w.name_of(&address("10:com", 0, 0)), "10:com 📌");
        assert_eq!(w.resolve("10"), "10:com");
    }

    #[test]
    fn desktops() {
        let w = World::of("fake-0: 4: ←, 4, 16: ↑ / fake-1: 4: ↑, 6");
        let four = DesktopName::from("4");
        let names: Vec<&str> = w.desktop(&four).map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["4: ←", "4", "4: ↑"]);
        // Its first workspace in i3's order decides.
        assert_eq!(w.desktop_output(&four).unwrap().as_str(), "fake-0");
        assert_eq!(w.desktop_output(&"5".into()), None);
        assert_eq!(w.at(&address("4", 0, 1)).unwrap().output.as_str(), "fake-1");
        assert_eq!(w.at(&Address { desktop: four, location: ORIGIN }).unwrap().name, "4");
    }

    #[test]
    fn resolve_order() {
        let w = World::of("fake-0: 1:be: ←, 1:be, 2:web: →, 2:web: ←, mail");
        // The name itself, satellite or not.
        assert_eq!(w.resolve("1:be"), "1:be");
        assert_eq!(w.resolve("1:be: ←"), "1:be: ←");
        assert_eq!(w.resolve("mail"), "mail");
        // An origin with the same number.
        assert_eq!(w.resolve("1"), "1:be");
        assert_eq!(w.resolve("1:other"), "1:be");
        // Else the origin its first satellite names.
        assert_eq!(w.resolve("2"), "2:web");
        // Else as typed.
        assert_eq!(w.resolve("3:new"), "3:new");
        assert_eq!(w.resolve("news"), "news");
    }

    #[test]
    fn only_active_outputs_are_monitors() {
        let output = |name: &str, active| ipc::Output {
            name: name.into(),
            active,
            primary: false,
            rect: Rect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
        };
        let w = World::new(
            &Naming::classic(),
            Vec::new(),
            vec![output("A", true), output("B", false)],
        );
        assert_eq!(w.monitors.len(), 1);
        assert!(w.monitor(&OutputName("B".into())).is_none());
    }
}
