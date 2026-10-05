//! End-to-end tests against a nested i3. Ignored by default, because they
//! need Xvfb and i3:
//!
//! ```sh
//! cargo test -- --include-ignored
//! ```

#[allow(dead_code)]
#[path = "../../src/error.rs"]
mod error;
#[allow(dead_code)]
#[path = "../../src/grid.rs"]
mod grid;
mod harness;
#[allow(dead_code)]
#[path = "../../src/ipc.rs"]
mod ipc;

use harness::Session;

const HOTKEY_DOWN: &str = "--desktop-origin-hotkey=down";

fn one() -> Session {
    Session::start("800x600+0+0P", "")
}

fn two() -> Session {
    Session::start("800x600+0+0P,800x600+800+0", "")
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn arrows_lead_to_fixed_satellites_and_the_hotkey_returns() {
    let s = one().flags(&[HOTKEY_DOWN]);
    s.fill(&["6"]);
    s.ok(&["focus", "up"]);
    assert_eq!(s.state(), "fake-0: 6, >6: ↑<");
    // The empty ↑ is gone as soon as it is left.
    s.ok(&["focus", "left"]);
    assert_eq!(s.state(), "fake-0: >6: ←<, 6");
    s.ok(&["focus", "right"]);
    assert_eq!(s.state(), "fake-0: 6, >6: →<");
    // Already there.
    s.ok(&["focus", "right"]);
    assert_eq!(s.state(), "fake-0: 6, >6: →<");
    s.ok(&["focus", "down"]);
    assert_eq!(s.state(), "fake-0: >6<");
    s.ok(&["focus", "down"]);
    s.ok(&["focus", "origin"]);
    assert_eq!(s.state(), "fake-0: >6<");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn arrows_walk_a_square_grid() {
    let s = one().flags(&["--grid=square"]);
    s.fill(&["6"]);
    s.ok(&["focus", "left"]);
    s.ok(&["focus", "up"]);
    assert_eq!(s.state(), "fake-0: 6, >6: ↖<");
    // Off the grid.
    s.ok(&["focus", "up"]);
    s.ok(&["focus", "left"]);
    assert_eq!(s.state(), "fake-0: 6, >6: ↖<");
    s.ok(&["focus", "right"]);
    s.ok(&["focus", "down"]);
    assert_eq!(s.state(), "fake-0: >6<");
    s.ok(&["focus", "--location=1,-1"]);
    assert_eq!(s.state(), "fake-0: 6, >6: ↘<");
    let err = s.fails(1, &["focus", "--location=2,0"]);
    assert!(err.contains("location (2,0) is not on the grid"), "{err}");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn an_explicit_location_must_be_on_the_grid_under_either_navigation() {
    let s = one();
    s.fill(&["6"]);
    // Relative to the origin, on a plus: a diagonal is not on it.
    let err = s.fails(1, &["focus", "--location=-1,1"]);
    assert!(err.contains("location (-1,1) is not on the grid"), "{err}");
    let err = s.fails(1, &["goto", "6", "--location=0,2"]);
    assert!(err.contains("location (0,2) is not on the grid"), "{err}");
    // The origin hotkey blocks an arrow, not a location.
    s.ok(&[HOTKEY_DOWN, "focus", "--location=0,-1"]);
    assert_eq!(s.state(), "fake-0: 6, >6: ↓<");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn move_takes_the_window_and_leaves_the_focus() {
    let s = one().flags(&[HOTKEY_DOWN]);
    s.fill(&["6"]);
    s.i3("open");
    s.ok(&["move", "left"]);
    assert_eq!(s.state(), "fake-0: 6: ←, >6<");
    s.ok(&["move", "--location=0,1"]);
    assert_eq!(s.state(), "fake-0: 6: ←, >6<, 6: ↑");
    // The origin from the origin: already there, even with nothing to move.
    s.ok(&["move", "down"]);
    assert_eq!(s.state(), "fake-0: 6: ←, >6<, 6: ↑");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn goto_finds_the_desktop() {
    let s = one();
    s.fill(&["6"]);
    // New.
    s.ok(&["goto", "4:db"]);
    assert_eq!(s.state(), "fake-0: >4:db<, 6");
    s.i3("open");
    // By number.
    s.ok(&["goto", "6"]);
    s.ok(&["goto", "4"]);
    assert_eq!(s.state(), "fake-0: >4:db<, 6");
    s.ok(&["goto", "4", "--location=-1,0"]);
    assert_eq!(s.state(), "fake-0: >4:db: ←<, 4:db, 6");
    s.i3("open");
    // An exact name is taken as it is, satellite or not.
    s.ok(&["goto", "6"]);
    s.ok(&["goto", "4:db: ←"]);
    assert_eq!(s.state(), "fake-0: >4:db: ←<, 4:db, 6");
    // From a satellite, the number still means the origin.
    s.ok(&["goto", "4"]);
    assert_eq!(s.state(), "fake-0: 4:db: ←, >4:db<, 6");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn goto_recreates_an_origin_from_its_satellite() {
    let s = one();
    s.fill(&["4:db: ←", "6"]);
    s.ok(&["goto", "4"]);
    assert_eq!(s.state(), "fake-0: 4:db: ←, >4:db<, 6");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn send_moves_the_window_to_another_desktop() {
    let s = one();
    s.fill(&["6"]);
    s.i3("open");
    s.ok(&["send", "7"]);
    assert_eq!(s.state(), "fake-0: >6<, 7");
    s.ok(&["send", "7", "--location=1,0"]);
    assert_eq!(s.state(), "fake-0: >6<, 7, 7: →");
    // Already here, even with nothing to move.
    s.ok(&["send", "6"]);
    assert_eq!(s.state(), "fake-0: >6<, 7, 7: →");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn a_failed_move_is_reported_with_its_command() {
    let s = one().flags(&[HOTKEY_DOWN]);
    s.i3("workspace 6");
    let err = s.fails(1, &["move", "left"]);
    assert!(
        err.contains(r#"command "move container to workspace \"6: ←\"" failed"#),
        "{err}"
    );
    let err = s.fails(1, &["send", "7"]);
    assert!(
        err.contains(r#"command "move container to workspace \"7\"" failed"#),
        "{err}"
    );
    assert_eq!(s.state(), "fake-0: >6<");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn satellites_sort_around_the_origin() {
    let s = one();
    s.fill(&["6"]);
    for arrow in ["down", "right", "up", "left"] {
        s.ok(&["focus", arrow]);
        s.i3("open");
    }
    assert_eq!(s.state(), "fake-0: >6: ←<, 6, 6: →, 6: ↑, 6: ↓");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn rename_desktop_takes_the_satellites_along() {
    let s = one();
    s.fill(&["4:Other", "4:Foo", "4:Foo: ↑"]);
    s.ok(&["rename-desktop", "Bar"]);
    assert_eq!(s.state(), "fake-0: 4:Other, 4:Bar, >4:Bar: ↑<");
    // The same name.
    s.ok(&["rename-desktop", "4:Bar"]);
    assert_eq!(s.state(), "fake-0: 4:Other, 4:Bar, >4:Bar: ↑<");

    let err = s.fails(1, &["rename-desktop", "7:Baz"]);
    assert!(err.contains("would change the desktop's number"), "{err}");
    let err = s.fails(1, &["rename-desktop", "4:Baz: ←"]);
    assert!(err.contains("names a satellite"), "{err}");
    let err = s.fails(1, &["rename-desktop", "Other"]);
    assert!(err.contains(r#"desktop "4:Other" already exists"#), "{err}");
    assert_eq!(s.state(), "fake-0: 4:Other, 4:Bar, >4:Bar: ↑<");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn bad_arguments_exit_with_2() {
    let s = one();
    s.fails(2, &["focus"]);
    s.fails(2, &["focus", "sideways"]);
    s.fails(2, &["--grid=square", "--navigation-relative-to=origin", "focus", "up"]);
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn a_desktop_stays_on_its_monitor() {
    let s = two();
    s.fill(&["4"]);
    s.i3("focus output fake-1");
    s.fill(&["6"]);
    assert_eq!(s.state(), "fake-0: [4] / fake-1: >6<");
    s.ok(&["goto", "4"]);
    assert_eq!(s.state(), "fake-0: >4< / fake-1: [6]");

    // A satellite asked for from the other monitor appears on the
    // desktop's monitor.
    s.i3("focus output fake-1");
    s.ok(&["goto", "4", "--location=0,1"]);
    assert_eq!(s.state(), "fake-0: 4, >4: ↑< / fake-1: [6]");
    s.i3("open");

    // So does one a window is sent to.
    s.i3("focus output fake-1");
    s.i3("open");
    s.ok(&["send", "4", "--location=1,0"]);
    // And fake-0 goes on showing what it showed.
    assert_eq!(s.state(), "fake-0: 4, 4: →, [4: ↑] / fake-1: >6<");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn output_moves_the_whole_desktop() {
    let s = two().flags(&[HOTKEY_DOWN]);
    s.fill(&["4: ←", "4: ↑", "4"]);
    // i3's own order, until the tool sorts it.
    assert_eq!(s.state(), "fake-0: 4: ←, 4: ↑, >4< / fake-1: [2]");
    s.ok(&["output", "right"]);
    assert_eq!(s.state(), "fake-0: [1] / fake-1: 4: ←, >4<, 4: ↑");
    // At the edge.
    s.ok(&["output", "right"]);
    s.ok(&["output", "up"]);
    assert_eq!(s.state(), "fake-0: [1] / fake-1: 4: ←, >4<, 4: ↑");
    // From a satellite.
    s.ok(&["focus", "left"]);
    s.ok(&["output", "left"]);
    assert_eq!(s.state(), "fake-0: >4: ←<, 4, 4: ↑ / fake-1: [1]");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn goto_bounces_back_with_auto_back_and_forth() {
    let s = Session::start("800x600+0+0P", "workspace_auto_back_and_forth yes");
    s.fill(&["6", "4"]);
    // Already there: i3 goes back to where it came from.
    s.ok(&["goto", "4"]);
    assert_eq!(s.state(), "fake-0: 4, >6<");
    s.ok(&["goto", "6"]);
    assert_eq!(s.state(), "fake-0: >4<, 6");
    // Arrows never bounce.
    s.ok(&["focus", "origin"]);
    s.ok(&[HOTKEY_DOWN, "focus", "down"]);
    assert_eq!(s.state(), "fake-0: >4<, 6");
    s.ok(&["focus", "up"]);
    s.ok(&["focus", "up"]);
    assert_eq!(s.state(), "fake-0: 4, >4: ↑<, 6");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn back_and_forth_returns_to_where_you_came_from() {
    let s = two();
    s.fill(&["6", "4"]);
    s.ok(&["goto", "6"]);
    s.i3("workspace back_and_forth");
    assert_eq!(s.state(), "fake-0: >4<, 6 / fake-1: [2]");
    // After an arrow.
    s.ok(&["focus", "up"]);
    s.i3("open");
    s.i3("workspace back_and_forth");
    assert_eq!(s.state(), "fake-0: >4<, 4: ↑, 6 / fake-1: [2]");
    // Across monitors.
    s.i3("focus output fake-1");
    s.fill(&["8"]);
    s.ok(&["goto", "6"]);
    assert_eq!(s.state(), "fake-0: 4, 4: ↑, >6< / fake-1: [8]");
    s.i3("workspace back_and_forth");
    assert_eq!(s.state(), "fake-0: 4, 4: ↑, [6] / fake-1: >8<");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn an_assigned_satellite_is_brought_back_to_its_desktop() {
    let s = Session::start(
        "800x600+0+0P,800x600+800+0",
        "workspace \"4: ←\" output fake-1",
    );
    s.fill(&["4"]);
    s.ok(&["focus", "left"]);
    // It lands on its desktop's monitor. The bar order loses: each sorting
    // rename sends the workspace back to its assigned output.
    assert_eq!(s.state(), "fake-0: 4, >4: ←< / fake-1: [1]");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn the_daemon_follows_a_plain_rename() {
    let mut s = one();
    s.daemon();
    s.fill(&["4:Foo: ↑", "4:Foo"]);
    // The number is put back.
    s.i3("rename workspace to Baz");
    s.becomes("fake-0: >4:Baz<, 4:Baz: ↑");
    // A different number is deliberate.
    s.i3("rename workspace to \"7:Qux\"");
    s.becomes("fake-0: >7:Qux<, 7:Qux: ↑");
    // A satellite's rename is nobody's business.
    s.i3("rename workspace \"7:Qux: ↑\" to elsewhere");
    s.becomes("fake-0: >7:Qux<, elsewhere");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn the_daemon_leaves_a_number_that_would_collide() {
    let mut s = one();
    s.daemon();
    s.fill(&["4:Taken", "4:Foo"]);
    s.i3("rename workspace to Taken");
    s.becomes("fake-0: 4:Taken, >Taken<");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn the_daemon_adopts_a_workspace_created_elsewhere() {
    let mut s = two();
    s.daemon();
    s.fill(&["4"]);
    s.i3("focus output fake-1");
    s.fill(&["6"]);
    s.i3("open");
    // i3 makes it on the focused monitor.
    s.i3("move container to workspace \"4: →\"");
    s.becomes("fake-0: [4], 4: → / fake-1: >6<");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn the_daemon_survives_a_rename_and_back() {
    let mut s = one();
    s.daemon();
    s.fill(&["4:Foo: ↑", "4:Foo"]);
    // Both are done before the daemon hears of the first.
    s.i3("rename workspace to \"4:Bar\"; rename workspace to \"4:Foo\"");
    s.becomes("fake-0: >4:Foo<, 4:Foo: ↑");
    s.becomes("fake-0: >4:Foo<, 4:Foo: ↑");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn the_daemon_runs_bindings() {
    let mut s = Session::start(
        "800x600+0+0P",
        "bindsym Mod1+u nop i3-spatial-desktop focus up\n\
         bindsym Mod1+d nop i3-spatial-desktop focus down\n\
         bindsym Mod1+g nop i3-spatial-desktop --grid square focus left; nop i3-spatial-desktop --grid square focus up",
    )
    .flags(&[HOTKEY_DOWN]);
    s.daemon();
    s.fill(&["6"]);
    if !s.press("alt+u") {
        eprintln!("xdotool is not installed; skipped");
        return;
    }
    s.becomes("fake-0: 6, >6: ↑<");
    // The binding inherits the daemon's origin hotkey.
    s.press("alt+d");
    s.becomes("fake-0: >6<");
    // Two commands in one binding, each overriding the daemon's grid.
    s.press("alt+g");
    s.becomes("fake-0: 6, >6: ↖<");
}

const LOCATIONS: &str = "--multimonitor-mode=location-per-monitor";
const PAN: &str = "--multimonitor-mode=pan";

/// Left, centre and right; the centre one is primary, so it is the origin
/// of the monitor grid.
fn three() -> Session {
    Session::start("800x600+0+0,800x600+800+0P,800x600+1600+0", "")
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn locations_each_monitor_shows_its_own() {
    let s = three().flags(&[LOCATIONS, HOTKEY_DOWN]);
    s.ok(&["goto", "4"]);
    assert_eq!(s.state(), "fake-0: [4: ←] / fake-1: >4< / fake-2: [4: →]");
    // Arrows move the focus between the monitors.
    s.ok(&["focus", "left"]);
    assert_eq!(s.state(), "fake-0: >4: ←< / fake-1: [4] / fake-2: [4: →]");
    s.ok(&["focus", "right"]);
    assert_eq!(s.state(), "fake-0: [4: ←] / fake-1: [4] / fake-2: >4: →<");
    s.ok(&["focus", "right"]);
    s.ok(&["focus", "down"]);
    assert_eq!(s.state(), "fake-0: [4: ←] / fake-1: >4< / fake-2: [4: →]");
    // A desktop key changes every monitor and lands on the origin.
    s.ok(&["focus", "left"]);
    s.ok(&["goto", "5"]);
    assert_eq!(s.state(), "fake-0: [5: ←] / fake-1: >5< / fake-2: [5: →]");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn locations_no_monitor_has_show_on_yours() {
    let s = three().flags(&[LOCATIONS, HOTKEY_DOWN]);
    s.ok(&["goto", "4"]);
    s.ok(&["focus", "left"]);
    s.i3("open");
    s.ok(&["focus", "up"]);
    assert_eq!(
        s.state(),
        "fake-0: 4: ←, >4: ↑< / fake-1: [4] / fake-2: [4: →]"
    );
    s.i3("open");
    // It stays when the focus moves on, and is found again from elsewhere.
    s.ok(&["focus", "right"]);
    assert_eq!(
        s.state(),
        "fake-0: 4: ←, [4: ↑] / fake-1: [4] / fake-2: >4: →<"
    );
    s.ok(&["focus", "up"]);
    assert_eq!(
        s.state(),
        "fake-0: 4: ←, >4: ↑< / fake-1: [4] / fake-2: [4: →]"
    );
    // Navigating back gives the monitor its own location again.
    s.ok(&["focus", "left"]);
    assert_eq!(
        s.state(),
        "fake-0: >4: ←<, 4: ↑ / fake-1: [4] / fake-2: [4: →]"
    );
    // So does a desktop key.
    s.ok(&["focus", "up"]);
    s.ok(&["goto", "5"]);
    assert_eq!(
        s.state(),
        "fake-0: 4: ←, 4: ↑, [5: ←] / fake-1: >5< / fake-2: [5: →]"
    );
    s.ok(&["goto", "4"]);
    assert_eq!(
        s.state(),
        "fake-0: [4: ←], 4: ↑ / fake-1: >4< / fake-2: [4: →]"
    );
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn locations_move_send_and_rename() {
    let s = three().flags(&[LOCATIONS, HOTKEY_DOWN]);
    s.ok(&["goto", "4"]);
    s.i3("open");
    s.i3("open");
    // The window goes to the workspace on the left monitor; nothing else
    // changes.
    s.ok(&["move", "left"]);
    assert_eq!(s.state(), "fake-0: [4: ←] / fake-1: >4< / fake-2: [4: →]");
    s.ok(&["goto", "5"]);
    assert_eq!(
        s.state(),
        "fake-0: 4: ←, [5: ←] / fake-1: 4, >5< / fake-2: [5: →]"
    );
    // A window sent to a desktop lands on its origin, as `goto` would.
    s.i3("open");
    s.ok(&["send", "4"]);
    assert_eq!(
        s.state(),
        "fake-0: 4: ←, [5: ←] / fake-1: 4, >5< / fake-2: [5: →]"
    );
    s.ok(&["rename-desktop", "five"]);
    assert_eq!(
        s.state(),
        "fake-0: 4: ←, [5:five: ←] / fake-1: 4, >5:five< / fake-2: [5:five: →]"
    );
    let err = s.fails(1, &["output", "left"]);
    assert!(
        err.contains("`output` needs --multimonitor-mode=desktop-per-monitor"),
        "{err}"
    );
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn locations_can_be_given_by_hand() {
    // Two monitors, with nothing at the origin of the grid.
    let s = Session::start("800x600+0+0P,800x600+800+0", "").flags(&[
        LOCATIONS,
        HOTKEY_DOWN,
        "--monitor-location=fake-0=-1,0",
        "--monitor-location=fake-1=1,0",
    ]);
    s.ok(&["goto", "4"]);
    assert_eq!(s.state(), "fake-0: >4: ←< / fake-1: [4: →]");
    // The origin shows on the monitor you are on.
    s.ok(&["focus", "down"]);
    assert_eq!(s.state(), "fake-0: >4< / fake-1: [4: →]");
    s.ok(&["focus", "right"]);
    assert_eq!(s.state(), "fake-0: [4] / fake-1: >4: →<");
    s.ok(&["goto", "5"]);
    assert_eq!(s.state(), "fake-0: [5: ←] / fake-1: >5: →<");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn pan_slides_both_monitors() {
    let s = Session::start("800x600+0+0P,800x600+800+0", "").flags(&[PAN]);
    s.ok(&["goto", "4"]);
    assert_eq!(s.state(), "fake-0: >4< / fake-1: [4: →]");
    // The diagrams: [[L] [C]] R, then L [[C] [R]], then up.
    s.ok(&["focus", "left"]);
    assert_eq!(s.state(), "fake-0: >4: ←< / fake-1: [4]");
    s.ok(&["focus", "right"]);
    assert_eq!(s.state(), "fake-0: >4< / fake-1: [4: →]");
    s.ok(&["focus", "up"]);
    assert_eq!(s.state(), "fake-0: >4: ↑< / fake-1: [4: ↗]");
    // No grid: it goes on.
    s.ok(&["focus", "up"]);
    s.ok(&["focus", "right"]);
    s.ok(&["focus", "right"]);
    assert_eq!(s.state(), "fake-0: >4: →2↑2< / fake-1: [4: →3↑2]");
    // From the other monitor the focus stays on it.
    s.i3("focus output fake-1");
    s.ok(&["focus", "down"]);
    assert_eq!(s.state(), "fake-0: [4: →2↑] / fake-1: >4: →3↑<");
    // A desktop key and the origin key bring the window home.
    s.ok(&["goto", "5"]);
    assert_eq!(s.state(), "fake-0: >5< / fake-1: [5: →]");
    s.ok(&["focus", "left"]);
    s.ok(&["focus", "left"]);
    s.i3("focus output fake-1");
    s.ok(&["focus", "origin"]);
    assert_eq!(s.state(), "fake-0: >5< / fake-1: [5: →]");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn pan_carries_workspaces_with_windows_across() {
    let s = Session::start("800x600+0+0P,800x600+800+0", "").flags(&[PAN, HOTKEY_DOWN]);
    s.ok(&["goto", "4"]);
    s.i3("open");
    s.i3("focus output fake-1");
    s.i3("open");
    s.i3("focus output fake-0");
    assert_eq!(s.state(), "fake-0: >4< / fake-1: [4: →]");
    s.ok(&["focus", "left"]);
    assert_eq!(s.state(), "fake-0: >4: ←< / fake-1: [4], 4: →");
    s.ok(&["focus", "right"]);
    s.ok(&["focus", "right"]);
    assert_eq!(s.state(), "fake-0: 4, >4: →< / fake-1: [4: →2]");
    // The hotkey still takes its arrow: Down is home, not one step down.
    s.ok(&["focus", "up"]);
    s.ok(&["focus", "down"]);
    assert_eq!(s.state(), "fake-0: >4< / fake-1: [4: →]");
    // A window moved a step goes to the next location; the window stays put.
    s.i3("open");
    s.ok(&["move", "right"]);
    s.ok(&["move", "left"]);
    assert_eq!(s.state(), "fake-0: 4: ←, >4< / fake-1: [4: →]");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn the_daemon_brings_the_other_monitors_along() {
    let mut s = three().flags(&[LOCATIONS, HOTKEY_DOWN]);
    s.daemon();
    s.ok(&["goto", "4"]);
    s.i3("open");
    s.ok(&["goto", "5"]);
    s.i3("open");
    s.becomes("fake-0: [5: ←] / fake-1: 4, >5< / fake-2: [5: →]");
    // The focus moves to another desktop without the tool.
    s.i3("workspace 4");
    s.becomes("fake-0: [4: ←] / fake-1: >4<, 5 / fake-2: [4: →]");
    // A satellite on another monitor leads just as well.
    s.i3("focus output fake-2");
    s.i3("open");
    s.i3("workspace 5");
    s.becomes("fake-0: [5: ←] / fake-1: 4, >5< / fake-2: 4: →, [5: →]");
    s.i3("workspace \"4: →\"");
    s.becomes("fake-0: [4: ←] / fake-1: [4], 5 / fake-2: >4: →<");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn back_and_forth_switches_every_monitor() {
    let mut s = three().flags(&[LOCATIONS, HOTKEY_DOWN]);
    s.daemon();
    s.ok(&["goto", "4"]);
    s.i3("open");
    s.ok(&["focus", "right"]);
    s.ok(&["goto", "5"]);
    s.i3("open");
    s.becomes("fake-0: [5: ←] / fake-1: 4, >5< / fake-2: [5: →]");
    // Back to the desktop just left, on every monitor; and forth again.
    for _ in 0..2 {
        s.i3("workspace back_and_forth");
        s.becomes("fake-0: [4: ←] / fake-1: >4<, 5 / fake-2: [4: →]");
        s.i3("workspace back_and_forth");
        s.becomes("fake-0: [5: ←] / fake-1: 4, >5< / fake-2: [5: →]");
    }
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn back_and_forth_while_panning() {
    let mut s = Session::start("800x600+0+0P,800x600+800+0", "").flags(&[PAN]);
    s.daemon();
    s.ok(&["goto", "4"]);
    s.i3("open");
    s.ok(&["focus", "right"]);
    s.i3("open");
    s.becomes("fake-0: 4, >4: →< / fake-1: [4: →2]");
    // Back one step: the window slides back with it.
    s.i3("workspace back_and_forth");
    s.becomes("fake-0: >4< / fake-1: [4: →]");
    // Between desktops, both monitors go back and forth.
    s.ok(&["goto", "5"]);
    s.i3("open");
    s.becomes("fake-0: 4, >5< / fake-1: 4: →, [5: →]");
    for _ in 0..2 {
        s.i3("workspace back_and_forth");
        s.becomes("fake-0: >4<, 5 / fake-1: [4: →]");
        s.i3("workspace back_and_forth");
        s.becomes("fake-0: 4, >5< / fake-1: 4: →, [5: →]");
    }
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn a_pinned_monitor_keeps_its_workspace() {
    let s = three().flags(&[LOCATIONS, HOTKEY_DOWN]);
    s.ok(&["goto", "4"]);
    s.ok(&["focus", "left"]);
    s.i3("open");
    s.ok(&["pin-monitor"]);
    assert_eq!(
        s.state(),
        "fake-0: >4: ← 📌< / fake-1: [4] / fake-2: [4: →]"
    );
    // A desktop key pressed on it switches the others and moves the focus
    // to the origin.
    s.ok(&["goto", "5"]);
    assert_eq!(
        s.state(),
        "fake-0: [4: ← 📌] / fake-1: >5< / fake-2: [5: →]"
    );
    // Arrows can move the focus onto it, and lead back from it to the
    // current desktop.
    s.ok(&["focus", "left"]);
    assert_eq!(
        s.state(),
        "fake-0: >4: ← 📌< / fake-1: [5] / fake-2: [5: →]"
    );
    // One that would change what it shows does nothing.
    s.ok(&["focus", "up"]);
    assert_eq!(
        s.state(),
        "fake-0: >4: ← 📌< / fake-1: [5] / fake-2: [5: →]"
    );
    s.ok(&["focus", "right"]);
    assert_eq!(
        s.state(),
        "fake-0: [4: ← 📌] / fake-1: [5] / fake-2: >5: →<"
    );
    s.ok(&["goto", "6"]);
    assert_eq!(
        s.state(),
        "fake-0: [4: ← 📌] / fake-1: >6< / fake-2: [6: →]"
    );
    // Let go, it rejoins the desktop the others show.
    s.ok(&["focus", "left"]);
    s.ok(&["pin-monitor", "on"]);
    s.ok(&["pin-monitor", "off"]);
    assert_eq!(
        s.state(),
        "fake-0: 4: ←, >6: ←< / fake-1: [6] / fake-2: [6: →]"
    );
    s.ok(&["pin-monitor", "off"]);

    let err = s.fails(1, &["--multimonitor-mode=pan", "pin-monitor"]);
    assert!(
        err.contains("`pin-monitor` needs --multimonitor-mode=location-per-monitor"),
        "{err}"
    );
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn a_pin_ends_when_its_workspace_leaves_the_screen() {
    let mut s = three().flags(&[LOCATIONS, HOTKEY_DOWN]);
    s.ok(&["goto", "4"]);
    s.ok(&["focus", "left"]);
    s.i3("open");
    s.ok(&["pin-monitor", "on"]);
    // Something else puts another workspace on that monitor.
    s.i3("workspace 9");
    assert_eq!(
        s.state(),
        "fake-0: 4: ← 📌, >9< / fake-1: [4] / fake-2: [4: →]"
    );
    // The next command finds the pin over, and the monitor follows again.
    s.ok(&["goto", "5"]);
    assert_eq!(
        s.state(),
        "fake-0: 4: ←, [5: ←] / fake-1: >5< / fake-2: [5: →]"
    );

    // The daemon sees to it by itself.
    s.daemon();
    s.ok(&["goto", "4"]);
    s.ok(&["focus", "left"]);
    s.ok(&["pin-monitor", "on"]);
    s.becomes("fake-0: >4: ← 📌< / fake-1: [4] / fake-2: [4: →]");
    s.i3("workspace 9");
    // Desktop 9 opens as it would after a desktop key: its origin belongs
    // on the centre monitor.
    s.becomes("fake-0: 4: ←, [9: ←] / fake-1: >9< / fake-2: [9: →]");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn a_window_sent_to_the_plain_name_joins_the_pinned_workspace() {
    // The rule knows the workspace by its plain name, which is also full of
    // characters that mean something in a regular expression.
    let mut s = Session::start(
        "800x600+0+0,800x600+800+0P,800x600+1600+0",
        "assign [class=\"^XClock$\"] → \"c++ (x)\"",
    )
    .flags(&[LOCATIONS, HOTKEY_DOWN]);
    s.daemon();
    s.ok(&["goto", "c++ (x)"]);
    s.i3("open");
    s.ok(&["pin-monitor", "on"]);
    s.ok(&["focus", "right"]);
    s.becomes("fake-0: [c++ (x): ←] / fake-1: [c++ (x) 📌] / fake-2: >c++ (x): →<");
    if !s.open("xclock", "XClock") {
        eprintln!("xclock or xdotool is not installed; skipped");
        return;
    }
    // i3 made a second workspace for it; its window is moved over and the
    // empty workspace goes.
    s.becomes("fake-0: [c++ (x): ←] / fake-1: [c++ (x) 📌] / fake-2: >c++ (x): →<");
}

#[test]
#[ignore = "needs Xvfb and i3"]
fn an_empty_twin_of_a_pinned_workspace_is_no_trouble() {
    let s = three().flags(&[LOCATIONS, HOTKEY_DOWN]);
    s.ok(&["goto", "4"]);
    s.i3("open");
    s.ok(&["pin-monitor", "on"]);
    s.ok(&["focus", "right"]);
    // Something asks i3 for the plain name, and gets a second workspace.
    s.i3("workspace 4");
    assert_eq!(s.state(), "fake-0: [4: ←] / fake-1: [4 📌] / fake-2: >4<");
    // It holds no window, so there is nothing to move over. Commands go on
    // working, and it is gone once its monitor shows something else.
    s.ok(&["focus", "left"]);
    assert_eq!(s.state(), "fake-0: >4: ←< / fake-1: [4 📌] / fake-2: [4]");
    s.ok(&["goto", "5"]);
    assert_eq!(s.state(), "fake-0: >5: ←< / fake-1: [4 📌] / fake-2: [5: →]");
}
