# Multi-monitor modes

Design notes for `--multimonitor-mode`. Status: agreed on 2026-10-05 and
implemented. This replaces the earlier two-mode `--multimonitor` note.

## The problem

Until now a desktop was the unit that lives on a monitor: all of its
workspaces share an output, and every action ended by restoring that. That
suits one way of working. Two others need the opposite:

- the desktop spread across the monitors, `←` on the left one, the origin in
  the centre, `→` on the right, for whichever desktop is current;
- two monitors used as one wide window that slides over the desktop's grid.

## The monitor grid

The monitors sit on a grid of their own, with an origin. Each monitor has a
coordinate on it.

```
 physical layout                  monitor grid
 [ DP-1 ][ eDP-1 * ][ DP-2 ]      DP-1 (-1,0)   eDP-1 (0,0)   DP-2 (1,0)
           * primary
```

- The coordinates come from the physical layout: i3's primary output is the
  origin, and the others are found by walking to each neighbour in turn.
- `--monitor-location=NAME=X,Y` overrides one monitor's coordinate.
- With no primary output (sway never has one), the top-left monitor is the
  origin.
- A monitor that ends up with no coordinate is left alone: the tool never
  changes what it shows.

A desktop is **projected** onto this grid: the grid's origin is laid on some
location of the desktop, and each monitor shows the location its coordinate
away from there.

## The three modes

`--multimonitor-mode={desktop-per-monitor,location-per-monitor,pan}`, default
`desktop-per-monitor`. It is a flag like `--grid`: set on the daemon line,
inherited by every binding, and repeated on one-shot commands.

In every mode `goto N` lands on desktop N's origin.

| Mode | `goto N` | Arrows |
|---|---|---|
| `desktop-per-monitor` | Focus goes to the monitor desktop N lives on. A new desktop opens on the focused monitor. | Walk the desktop on the focused monitor. |
| `location-per-monitor` | Every unpinned monitor shows N at its grid coordinate. Focus goes to the monitor at the grid's origin. | Focus goes to the monitor already showing the target, else to the monitor at that coordinate. If neither exists, the focused monitor shows it. |
| `pan` | The grid's origin returns to N's origin. Focus goes to the monitor there. | Every monitor moves one location. Focus stays on its monitor. |

### `desktop-per-monitor`

Exactly the behaviour before this work. A desktop's workspaces share an
output, `output <side>` moves the whole desktop to the neighbouring monitor,
and the monitor grid is not used.

### `location-per-monitor`

The grid stays pinned to the desktop's origin, so a monitor always shows its
own coordinate of the current desktop.

```
 monitor grid   LEFT (-1,0)   CENTRE (0,0)   RIGHT (1,0)

 on desktop 4   [ 4: ← ]      [>4<]          [ 4: → ]
 goto 5         [ 5: ← ]      [>5<]          [ 5: → ]
 focus left     [>5: ←<]      [ 5 ]          [ 5: → ]
 focus up       [>5: ↑<]      [ 5 ]          [ 5: → ]     no monitor has ↑
```

- Arrows work out a target location with the usual navigation options, then
  find its monitor as the table says.
- A monitor showing a location that is not its own keeps it until the user
  navigates back or changes desktop. Moving the focus to another monitor does
  not change it.
- If no unpinned monitor can show the desktop's origin, `goto` leaves the
  focus where it is.
- The current desktop is the one the unpinned monitors show.

### `pan`

The grid slides. An arrow adds one step to every monitor's address and the
desktop is projected again.

```
 monitor grid   LEFT (0,0)    RIGHT (1,0)

 on desktop 4   [ 4: ← ]      [>4<]         grid origin on (-1,0)
 focus right    [ 4 ]         [>4: →<]      grid origin on (0,0)
 focus up       [ 4: ↑ ]      [>4: ↗<]      grid origin on (0,1)
 goto 5         [>5<]         [ 5: → ]      grid origin on (0,0)
```

- `--grid` and `--grid-bounds` do not apply: the grid can slide anywhere.
- The origin hotkey still takes its arrow. It and `focus origin` put the
  grid's origin back on the desktop's origin, as `goto` does.
- Where the grid sits is read back from what the focused monitor shows.

### Empty locations

Nothing special is needed. i3 cannot show nothing on a monitor, so a monitor
over a location with no workspace gets an empty one, and i3 deletes it when
it is hidden unless a window was put there.

### `move`, `send`, `output`

- `move` and `send` put the container on the workspace the matching `focus`
  or `goto` would have focused. Nothing on screen changes.
- `output <side>` exists only in `desktop-per-monitor`. In the other modes it
  reports an error.

## Pins

`location-per-monitor` only. `pin-monitor [on|off|toggle]` acts on the focused
monitor.

- A monitor is pinned when the workspace it shows ends in the marker ` 📌`.
  Pinning is a rename: `4:db: ←` becomes `4:db: ← 📌`. The marker is a
  constant, not a flag.
- A pin freezes what the monitor already shows. It keeps that workspace when
  the desktop changes.
- A pinned monitor neither follows nor leads. Arrows can move the focus onto
  it; from it they lead back to the current desktop; an arrow that would
  change what it shows does nothing.
- A desktop key pressed on a pinned monitor switches the other monitors and
  moves the focus off it.
- Let go, the monitor rejoins the desktop the others show.
- **Lifetime:** the marker is removed once its workspace is no longer on
  screen. Turning a monitor off therefore unpins it.
- **Twins:** an i3 `assign` or `for_window` rule that names the plain
  workspace makes i3 create a second workspace at the same address. The tool
  folds its windows into the pinned one. This needs the daemon.

## Following the focus

When the focus lands on another desktop by other means (a click on the bar, a
plain i3 command), the daemon brings the other monitors along. The focus stays
on the workspace it is on. In `location-per-monitor`, a workspace that was
opened on the wrong monitor for its location is moved to its own, as after
`goto`. Without the daemon only the tool's own commands do this.

Each pass only acts on differences, so the events the daemon's own changes
cause find nothing left to do. As a backstop it stops following after five
passes in a row that each found something, until the next key press.

`workspace back_and_forth` keeps meaning "the previous desktop": the tool
orders its commands so that i3's previous workspace is on the old desktop, and
the follow rule then brings the other monitors back with it.

## Design

```
key press -> Plan (the mode decides) -> i3 commands (one reconciler)
```

- `World` is one parsed snapshot of i3's workspaces and outputs.
- A `MonitorMode` (`src/mode.rs`, one file per mode under `src/mode/`) turns
  a key press into a `Plan`: the command the action itself sends, and the
  state it must leave behind (`Wanted`: which workspace each changed monitor
  shows, which desktops must sit on one output, which workspace is focused,
  and where `workspace back_and_forth` should return).
- `reconcile(world, wanted)` turns that state into one batch of i3 commands.
  It takes workspace names, not a mode, and emits nothing when i3 already
  matches. Because of that, running it again on the daemon's own events does
  nothing.
- `desktop-per-monitor` fills in only the action's command and the desktop to
  gather, which is what the tool always did.

All of that is pure. I/O happens in a fixed shape around it: snapshot, plan,
send the action's command, snapshot, reconcile, snapshot, restore the bar
order, and reconcile once more if that renamed anything.

### No state

Nothing is stored between commands.

| What | Where it lives |
|---|---|
| Mode, monitor coordinates | flags on the daemon line |
| Pins | the workspace's name |
| Current desktop, where the pan grid sits | read back from what the monitors show |

So a daemon restart loses nothing, and the daemon and a one-shot command
reach the same answer from the same state.

## Limits

- One-shot commands need the same flags as the daemon, as before.
- Checked against i3 4.24. Sway is untested in the two new modes.
- A `workspace N output X` line in the i3 config fights the two new modes.
- Hidden workspaces stay on whatever output they are on in the two new
  modes; they are moved when they are next shown.

## Verification

- Unit tests for every pure part: the name codec, navigation, the modes, the
  reconciler.
- `tests/i3`: end-to-end tests against a nested i3 on fake outputs, started
  with a cleared environment and its own socket so they cannot reach the
  session they are run from. `cargo test -- --include-ignored`.
