//! Bringing i3 to a wanted state. The commands are worked out from one
//! snapshot and only cover what differs, so reconciling again right after
//! does nothing.

use std::collections::{BTreeMap, BTreeSet};

use crate::command::Command;
use crate::mode::Wanted;
use crate::naming::DesktopName;
use crate::world::{OutputName, Workspace, World};

/// The commands that bring i3 from `world` to `want`. Empty when it is
/// there already.
pub fn reconcile(world: &World, want: &Wanted) -> Vec<Command> {
    let mut commands = Vec::new();
    // Workspaces that change output. i3 only moves the focused workspace,
    // so each one is visited, which changes what two outputs show: the one
    // it leaves and the one it lands on.
    let mut moved: BTreeMap<&str, &OutputName> = BTreeMap::new();
    let mut disturbed: BTreeSet<&OutputName> = BTreeSet::new();
    let strays = want.gather.iter().flat_map(|(desktop, output)| {
        world
            .desktop(desktop)
            .filter(move |w| w.output != *output)
            .map(move |w| (w, output))
    });
    let misplaced = want.shown.iter().filter_map(|(monitor, name)| {
        let workspace = world.named(name)?;
        (workspace.output != *monitor).then_some((workspace, monitor))
    });
    for (workspace, output) in strays.chain(misplaced) {
        if moved.insert(&workspace.name, output).is_none() {
            commands.push(Command::Show(workspace.name.clone()));
            commands.push(Command::MoveWorkspaceTo(output.clone()));
            disturbed.extend([&workspace.output, output]);
        }
    }

    // New workspaces normally appear on the focused output, hence the
    // `focus output` first.
    for (monitor, name) in &want.shown {
        let shown = world.visible_on(monitor).is_some_and(|w| w.name == *name);
        if !shown || disturbed.contains(monitor) {
            commands.push(Command::FocusOutput(monitor.clone()));
            commands.push(Command::Show(name.clone()));
        }
    }

    // Where the focus goes afterwards. If that workspace is empty, i3
    // deletes it while the others are visited, and `workspace` re-creates
    // it on the focused output, so return to the right output first.
    let focus = want
        .focus
        .as_deref()
        .or_else(|| world.focused().ok().map(|w| w.name.as_str()));
    let focus_output = focus.and_then(|name| {
        let desktop = world.address_of(name).desktop;
        (want.shown.iter())
            .find(|(_, shown)| *shown == name)
            .map(|(monitor, _)| monitor)
            .or_else(|| moved.get(name).copied())
            .or_else(|| {
                (want.gather.iter())
                    .find(|(gathered, _)| *gathered == desktop)
                    .map(|(_, output)| output)
            })
            .or_else(|| world.named(name).map(|w| &w.output))
    });

    // Outputs a visit left showing something else get back what they had.
    for output in disturbed {
        if want.shown.contains_key(output) || focus_output == Some(output) {
            continue;
        }
        let before = world
            .visible_on(output)
            .filter(|w| !moved.contains_key(w.name.as_str()));
        if let Some(workspace) = before {
            commands.push(Command::FocusOutput(output.clone()));
            commands.push(Command::Show(workspace.name.clone()));
        }
    }

    let unfocused = want
        .focus
        .as_deref()
        .is_some_and(|name| world.focused().map_or(true, |w| w.name != name));
    let visited = !commands.is_empty();
    if let Some(name) = focus
        && (visited || unfocused)
    {
        commands.extend(focus_output.cloned().map(Command::FocusOutput));
        // i3 remembers the workspace the focus last came from, so come from
        // the right one. Only when the monitors were visited, and only if
        // that workspace is on this output, or would be made on it.
        let back = (want.back.as_deref())
            .filter(|back| visited && *back != name)
            .filter(|back| {
                let lives = world.named(back).map(|w| moved.get(*back).copied().unwrap_or(&w.output));
                lives.is_none_or(|output| Some(output) == focus_output)
            });
        commands.extend(back.map(|back| Command::Show(back.to_owned())));
        commands.push(Command::Show(name.to_owned()));
    }
    commands
}

/// i3 orders workspaces sharing a number by creation, and renaming one
/// (even to its own name) moves it to the end of that run on its output.
/// Renaming a desktop's workspaces in the wanted order therefore sorts
/// them. Empty when `desktops` are in bar order already.
pub fn tidy(world: &World, desktops: &BTreeSet<DesktopName>) -> Vec<Command> {
    let mut renames = Vec::new();
    for desktop in desktops {
        // i3 keeps an order per output.
        let mut by_output: BTreeMap<&OutputName, Vec<&Workspace>> = BTreeMap::new();
        for workspace in world.desktop(desktop) {
            by_output.entry(&workspace.output).or_default().push(workspace);
        }
        for members in by_output.values_mut() {
            if members.is_sorted_by_key(|w| w.address.location.bar_order()) {
                continue;
            }
            members.sort_by_key(|w| w.address.location.bar_order());
            renames.extend(members.iter().map(|w| Command::Rename {
                from: w.name.clone(),
                to: w.name.clone(),
            }));
        }
    }
    renames
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(name: &str) -> OutputName {
        OutputName(name.into())
    }

    fn texts(commands: &[Command]) -> Vec<String> {
        commands.iter().map(Command::to_string).collect()
    }

    fn gathered(desktop: &str, onto: &str, focus: &str) -> Wanted {
        Wanted::gathered(desktop.into(), output(onto), Some(focus.to_owned()))
    }

    fn shown(monitors: &[(&str, &str)], focus: &str) -> Wanted {
        Wanted {
            shown: monitors
                .iter()
                .map(|(monitor, name)| (output(monitor), (*name).to_owned()))
                .collect(),
            focus: Some(focus.to_owned()),
            ..Wanted::default()
        }
    }

    #[test]
    fn nothing_to_do_when_the_world_matches() {
        let w = World::of("fake-0: 4: ←, >4<, 6 / fake-1: [7]");
        assert!(reconcile(&w, &gathered("4", "fake-0", "4")).is_empty());
        assert!(reconcile(&w, &gathered("9", "fake-0", "4")).is_empty());
        assert!(reconcile(&w, &shown(&[("fake-0", "4"), ("fake-1", "7")], "4")).is_empty());
        assert!(reconcile(&w, &Wanted::default()).is_empty());
    }

    #[test]
    fn strays_are_gathered_and_the_focus_returns() {
        let w = World::of("fake-0: >4<, 6 / fake-1: 4: →, 4: ↑, [7]");
        assert_eq!(
            texts(&reconcile(&w, &gathered("4", "fake-0", "4"))),
            [
                r#"workspace --no-auto-back-and-forth "4: →""#,
                r#"move workspace to output "fake-0""#,
                r#"workspace --no-auto-back-and-forth "4: ↑""#,
                r#"move workspace to output "fake-0""#,
                // What the other output showed is put back.
                r#"focus output "fake-1""#,
                r#"workspace --no-auto-back-and-forth "7""#,
                r#"focus output "fake-0""#,
                r#"workspace --no-auto-back-and-forth "4""#,
            ]
        );
    }

    #[test]
    fn a_stray_that_was_on_screen_leaves_its_output_to_i3() {
        // `4: ↑` was just made on the focused output; nothing else was
        // shown there that could be put back.
        let w = World::of("fake-0: [4] / fake-1: 6, >4: ↑<");
        assert_eq!(
            texts(&reconcile(&w, &gathered("4", "fake-0", "4: ↑"))),
            [
                r#"workspace --no-auto-back-and-forth "4: ↑""#,
                r#"move workspace to output "fake-0""#,
                r#"focus output "fake-0""#,
                r#"workspace --no-auto-back-and-forth "4: ↑""#,
            ]
        );
    }

    #[test]
    fn the_focus_returns_to_its_own_output() {
        // The focus is on another desktop, on another output.
        let w = World::of("fake-0: [4] / fake-1: 4: →, >6<");
        assert_eq!(
            texts(&reconcile(&w, &gathered("4", "fake-0", "6"))),
            [
                r#"workspace --no-auto-back-and-forth "4: →""#,
                r#"move workspace to output "fake-0""#,
                r#"focus output "fake-0""#,
                r#"workspace --no-auto-back-and-forth "4""#,
                r#"focus output "fake-1""#,
                r#"workspace --no-auto-back-and-forth "6""#,
            ]
        );
        // A focus workspace that does not exist yet belongs on its
        // desktop's output.
        let commands = texts(&reconcile(&w, &gathered("4", "fake-0", "4: ↑")));
        assert_eq!(
            commands[commands.len() - 2..],
            [
                r#"focus output "fake-0""#,
                r#"workspace --no-auto-back-and-forth "4: ↑""#,
            ]
        );
    }

    #[test]
    fn no_focus_named_means_the_focus_stays() {
        let w = World::of("fake-0: [4] / fake-1: 4: →, >6<");
        let want = Wanted::gathered("4".into(), output("fake-0"), None);
        let commands = texts(&reconcile(&w, &want));
        assert_eq!(
            commands[commands.len() - 2..],
            [
                r#"focus output "fake-1""#,
                r#"workspace --no-auto-back-and-forth "6""#,
            ]
        );
        // And with nothing astray, nothing is sent at all.
        let w = World::of("fake-0: [4], 4: → / fake-1: >6<");
        assert!(reconcile(&w, &want).is_empty());
    }

    #[test]
    fn a_wanted_focus_is_enforced() {
        let w = World::of("fake-0: [4] / fake-1: >6<");
        assert_eq!(
            texts(&reconcile(&w, &gathered("4", "fake-0", "4"))),
            [
                r#"focus output "fake-0""#,
                r#"workspace --no-auto-back-and-forth "4""#,
            ]
        );
    }

    #[test]
    fn monitors_show_what_is_wanted() {
        // Panning right: fake-0 takes what fake-1 shows, fake-1 a new one.
        let w = World::of("fake-0: >4: ←< / fake-1: [4]");
        assert_eq!(
            texts(&reconcile(
                &w,
                &shown(&[("fake-0", "4"), ("fake-1", "4: →")], "4")
            )),
            [
                r#"workspace --no-auto-back-and-forth "4""#,
                r#"move workspace to output "fake-0""#,
                r#"focus output "fake-0""#,
                r#"workspace --no-auto-back-and-forth "4""#,
                r#"focus output "fake-1""#,
                r#"workspace --no-auto-back-and-forth "4: →""#,
                r#"focus output "fake-0""#,
                r#"workspace --no-auto-back-and-forth "4""#,
            ]
        );
        // A desktop switch where everything already lives on its monitor.
        let w = World::of("fake-0: [4: ←], 5: ← / fake-1: >4<");
        assert_eq!(
            texts(&reconcile(
                &w,
                &shown(&[("fake-0", "5: ←"), ("fake-1", "5")], "5")
            )),
            [
                r#"focus output "fake-0""#,
                r#"workspace --no-auto-back-and-forth "5: ←""#,
                r#"focus output "fake-1""#,
                r#"workspace --no-auto-back-and-forth "5""#,
                r#"focus output "fake-1""#,
                r#"workspace --no-auto-back-and-forth "5""#,
            ]
        );
        // Only the focus moves.
        let w = World::of("fake-0: [5: ←] / fake-1: >5<");
        assert_eq!(
            texts(&reconcile(&w, &shown(&[], "5: ←"))),
            [
                r#"focus output "fake-0""#,
                r#"workspace --no-auto-back-and-forth "5: ←""#,
            ]
        );
    }

    #[test]
    fn back_and_forth_is_pointed_at_the_right_workspace() {
        // A desktop switch: afterwards i3 should go back to `4`.
        let w = World::of("fake-0: [4: ←], 5: ← / fake-1: >4<");
        let mut want = shown(&[("fake-0", "5: ←"), ("fake-1", "5")], "5");
        want.back = Some("4".into());
        let commands = texts(&reconcile(&w, &want));
        assert_eq!(
            commands[commands.len() - 3..],
            [
                r#"focus output "fake-1""#,
                r#"workspace --no-auto-back-and-forth "4""#,
                r#"workspace --no-auto-back-and-forth "5""#,
            ]
        );
        // Not when only the focus moves: i3 remembers the right one itself.
        let w = World::of("fake-0: [5: ←] / fake-1: >5<");
        let mut want = shown(&[], "5: ←");
        want.back = Some("4".into());
        assert_eq!(reconcile(&w, &want).len(), 2);
        // Nor when that workspace is on another output, which showing it
        // would disturb.
        let w = World::of("fake-0: [4: ←], 5: ←, 4 / fake-1: >9<");
        let mut want = shown(&[("fake-0", "5: ←"), ("fake-1", "5")], "5");
        want.back = Some("4".into());
        let commands = texts(&reconcile(&w, &want));
        assert!(!commands.contains(&r#"workspace --no-auto-back-and-forth "4""#.to_owned()));
    }

    #[test]
    fn a_monitor_left_out_is_put_back_as_it_was() {
        // fake-1 wants a workspace that is hidden on fake-2, which is not
        // part of the request.
        let w = World::of("fake-0: >4< / fake-1: [4: →] / fake-2: [9], 5: →");
        assert_eq!(
            texts(&reconcile(&w, &shown(&[("fake-1", "5: →")], "4"))),
            [
                r#"workspace --no-auto-back-and-forth "5: →""#,
                r#"move workspace to output "fake-1""#,
                r#"focus output "fake-1""#,
                r#"workspace --no-auto-back-and-forth "5: →""#,
                r#"focus output "fake-2""#,
                r#"workspace --no-auto-back-and-forth "9""#,
                r#"focus output "fake-0""#,
                r#"workspace --no-auto-back-and-forth "4""#,
            ]
        );
    }

    #[test]
    fn tidy_sorts_each_output_on_its_own() {
        let desktops = |names: &[&str]| -> BTreeSet<DesktopName> {
            names.iter().map(|name| (*name).into()).collect()
        };
        let w = World::of("fake-0: 4: ↑, >4<, 4: ←, 5: →, 5");
        assert_eq!(
            texts(&tidy(&w, &desktops(&["4"]))),
            [
                r#"rename workspace "4: ←" to "4: ←""#,
                r#"rename workspace "4" to "4""#,
                r#"rename workspace "4: ↑" to "4: ↑""#,
            ]
        );
        assert_eq!(tidy(&w, &desktops(&["4", "5"])).len(), 5);
        // In order already.
        let w = World::of("fake-0: 4: ←, >4<, 4: ↑");
        assert!(tidy(&w, &desktops(&["4"])).is_empty());
        // A desktop spread over the monitors is in order when each
        // monitor's part is.
        let w = World::of("fake-0: [4: ←] / fake-1: >4<, 4: ↑ / fake-2: [4: →]");
        assert!(tidy(&w, &desktops(&["4"])).is_empty());
        let w = World::of("fake-0: [4: ←] / fake-1: 4: ↑, >4< / fake-2: [4: →]");
        assert_eq!(
            texts(&tidy(&w, &desktops(&["4"]))),
            [
                r#"rename workspace "4" to "4""#,
                r#"rename workspace "4: ↑" to "4: ↑""#,
            ]
        );
    }
}
