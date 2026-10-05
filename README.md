# i3-spatial-desktop

Spatial desktops for i3 and sway.

## The problem

Putting every browser window in one workspace makes it hard to keep separate
projects apart. And a project often needs more than one workspace: one for its
web pages, one for its code, and one for Claude. i3 has no built-in way to group
those workspaces as a project while keeping each one independently accessible.

This project adds a **desktop**: a higher-level project space that groups a set
of related workspaces. Each project can have its own set of workspaces, and you
can move within the desktop spatially to browser the workspaces.

For example, `e` means editor, `c` means code, or claude, or other project tools
(tests, benchmarks, and so on), and `b` means browser. Instead of keeping those
workspaces in one linear sequence:

```
1:e | 2:c | 3:b | 3:e | 4:c | 5:b | 6:derpweb | 7:derpmedia | 8:derpfam | 9:asynccom | 10:syncom
```

group each project's workspaces together, around a central origin.

```
                    [terminal]
                        |
     [serve loops] — [origin] — [browser]
```

Keep your origin the focal point,

* For my coding desktops it's typically my neovim, and my [coding-agent-podman](https://github.com/EvanCarroll/coding-agent-podman).
* For my communication "desktop", it's my signal.

Then find a method that allows you to grow around that origin to accommodate
your needs. To the right of the origin, I usually store my web browser. To the
left of the origin, I usually have my loops for diagnostics,
[mirrord](https://metalbear.com/mirrord/), or [k9s](https://k9scli.io/), or [zola serve](https://www.getzola.org/documentation/getting-started/cli-usage/). Above the origin I usually have a terminal.

## Terminology

- **Desktop**: an abstraction that contains zero or more i3 workspaces arranged spatially.
- **Origin workspace**: the main i3 workspace of a desktop, e.g. `4:db`. It's the
  one `$mod+4` takes you to, and the one the others (satellites) are relative to it.
- **Satellite workspaces**: the desktop's other i3 workspaces, each at a
  **location** on a grid around the origin, which sits at (0,0). The location
  is part of the name, rendered as arrows (`4:db: ←`, `4:db: ←2↑`) or as
  coordinates (`4:db: (-1,0)`, `4:db: (-2,1)`); see
  [Workspace names](#workspace-names).

```
            [4:db: ↑]
[4:db: ←]   [4:db   ]    [4:db: →]
            [4:db: ↓]
```

Whatever workspace you're on, the arrow keys move around *its* desktop.
By default a desktop's workspaces live on the same monitor, move between
monitors together, and sit together on the bar (the other arrangements are
under [Several monitors](#several-monitors)):

```
[1:be] [4:db: ←] [4:db] [4:db: →] [4:db: ↑] [5:cow]
```

Desktops work with numbered (`6`), labelled (`4:db`) and unnumbered (`mail`)
origins. Satellites are ordinary i3 workspaces: i3 deletes them when they're
empty and you leave them, so only the ones in use show up on the bar.

## Install

```sh
cargo install --path .
```

## Setup

The complete configuration is in
[`examples/i3-spatial-desktop.conf`](examples/i3-spatial-desktop.conf). Copy it next to your config
and include it, after `$mod` and `$ws1`…`$ws10` are defined:

```sh
cp examples/i3-spatial-desktop.conf ~/.config/i3/i3-spatial-desktop.conf
```

```
set $mod  Mod4
set $ws1  "1:be"
set $ws2  "2:be-ai"
# … $ws3 – $ws9
set $ws10 "10:com"

include ~/.config/i3/i3-spatial-desktop.conf
```

(`include` needs i3 ≥ 4.20; on older versions paste the file in instead.)

Then remove the bindings it takes over from your own config, or i3 will
complain about duplicates:

- `$mod+1`…`$mod+0` and `$mod+Shift+1`…`$mod+Shift+0`
  (`workspace number N`, `move container to workspace number N`)
- `$mod+arrows`, `$mod+Shift+arrows`
- `$mod+bracketleft`, `$mod+bracketright`
- `$mod+Shift+R` (a rename prompt; the example's is the usual i3-input one)
- `$mod+hjkl`, `$mod+Shift+hjkl`; the example file includes the standard
  ones, since the arrows no longer move focus within a workspace.

Finally **restart** i3 (`i3-msg restart`). A reload is not enough the first
time: i3 only runs `exec_always` on start and restart. Check it's running:

```sh
pgrep -a i3-spatial-desktop     # → … /home/you/.cargo/bin/i3-spatial-desktop … daemon
```

## Navigation

Four options on the daemon line decide what the arrows do. The example
configuration uses `--desktop-origin-hotkey=down`, with the others left at
their defaults: arrows go to the four satellites around the origin, Down
returns.

### `--desktop-origin-hotkey={none,up,down}`, default `none`

An arrow that returns to the origin from **any** workspace of the desktop.
Since that arrow is taken, nothing in its direction can be reached: with
`down` there is no `↓` (or anything else below the origin), with `up` no `↑`.

```
 none                     down                     up

        [↑]                      [↑]
 [←] [origin] [→]         [←] [origin] [→]         [←] [origin] [→]
        [↓]                  Down = origin               [↓]
                                                    Up = origin
```

With `none`, the only ways back to the origin are `goto` (`$mod+4`), a binding
for `focus origin`, or, with `--navigation-relative-to=focus`, stepping back
(Up from `↓`).

### `--navigation-relative-to={origin,focus}`, default `origin` (or `focus`, see below)

- **`origin`**: each arrow always leads to the same satellite, wherever you
  are on the desktop. On `4:db: →`, Right does nothing and Left goes to
  `4:db: ←`.
- **`focus`**: arrows step from the focused workspace across the grid. On
  `4:db: ←`, Right goes back to `4:db`; pressing it again goes to `4:db: →`.
  Moves off the grid do nothing.

```
 origin: from anywhere         focus: one step at a time

   Left  → [←]                   [←] ──Right──▶ [origin] ──Right──▶ [→]
   Right → [→]                   [←] ◀──Left─── [origin] ◀──Left─── [→]
   Up    → [↑]
```

### `--grid={plus,square}`, default `plus`, and `--grid-bounds=N` / `--no-grid-bounds`, default `1`

The locations that exist when navigating relative to focus. `--grid` is the
shape; `--grid-bounds` is how many steps out from the origin it reaches, and
`--no-grid-bounds` removes the limit. Moves off the grid do nothing.

```
 plus, bounds 1            square, bounds 1           plus, bounds 2

        [↑]                [↖] [↑] [↗]                     [↑2]
 [←] [origin] [→]          [←] [o] [→]                     [↑]
        [↓]                [↙] [↓] [↘]         [←2] [←] [origin] [→] [→2]
                                                           [↓]
                                                           [↓2]
```

Anything other than plus with bounds 1 only makes sense relative to focus
(relative to the origin, each arrow only reaches the satellite one step
away), so `--grid=square`, `--grid-bounds` above 1 and `--no-grid-bounds`
make `focus` the default. You only get an error if you explicitly combine
them with `--navigation-relative-to=origin`. So this is enough:

```
exec_always --no-startup-id ~/.cargo/bin/i3-spatial-desktop --grid=square --desktop-origin-hotkey=down daemon
```

With the origin hotkey, the grid still exists in that direction but can't be
reached: `--grid=square --desktop-origin-hotkey=down` lets you walk
`↖ ↑ ↗ / ← o →`, and Down from any of them returns to the origin.

### Combinations at a glance

| hotkey | relative to | grid   | Behaviour |
|--------|-------------|--------|-----------|
| `down` | `origin`    | plus   | Arrows go to `←`/`→`/`↑`, Down to the origin. (Example config.) |
| `none` | `origin`    | plus   | Four fixed satellites; return with `$mod+N`. |
| `up`   | `origin`    | plus   | Mirror of `down`: Up returns, Down goes to `↓`. |
| `none` | `focus`     | plus   | Walk the plus; the opposite arrow steps back. |
| `none` | `focus`     | square | Walk a 3×3 grid. |
| `down` | `focus`     | square | Walk the top two rows; Down returns from anywhere. |
| `none` | `focus`     | plus, `--no-grid-bounds` | Left-left-left: endless lines out from the origin. |
| `down` | `focus`     | square, `--no-grid-bounds` | An endless half-plane above the origin. |

`move` (Shift+arrows) follows exactly the same rules as `focus`.

## Workspace names

Internally a satellite is just its desktop and its location `(x, y)`; the
name is a rendering of that. `--render-workspace-location` picks the
rendering for new workspaces:

| Location | `arrows` (default) | `coordinates` |
|----------|--------------------|---------------|
| (-1, 0)  | `4:db: ←`          | `4:db: (-1,0)` |
| (-1, 1)  | `4:db: ↖`          | `4:db: (-1,1)` |
| (-3, 0)  | `4:db: ←3`         | `4:db: (-3,0)` |
| (-2, 1)  | `4:db: ←2↑`        | `4:db: (-2,1)` |
| (1, -12) | `4:db: →↓12`       | `4:db: (1,-12)` |

In `arrows`, a count follows its arrow and is left out for one step; the
four one-step diagonals use their own arrows. The origin is always its plain
name (`4:db`), so `$mod+N` and your `$ws` variables keep working.

Both renderings are always read back, so switching
`--render-workspace-location` never orphans a workspace: an existing
workspace at a location is reused under the name it already has, and only new
locations get the new rendering. Only the exact form i3-spatial-desktop writes is
recognised (`←2`, not `←02` or `←←`; `(-1,0)`, not `(-1, 0)`), so every
location has one name per rendering.

The separator before the location is `--sep` (`": "`), shared by both
renderings and following i3's `N: name` convention. Coordinates contain a comma, which separates
commands in i3; i3-spatial-desktop quotes every name it sends, but quote them yourself in
hand-written commands (`i3-msg 'workspace "6: (-1,0)"'`).

## Several monitors

`--multimonitor-mode` decides what each monitor is given to show. It goes on
the daemon line, like the navigation options.

| Mode | A monitor shows | `goto N` | Arrows |
|------|-----------------|----------|--------|
| `desktop-per-monitor` (default) | a desktop of its own | the focus goes to the monitor desktop N is on | walk that desktop |
| `location-per-monitor` | its own location of the current desktop | every monitor switches to desktop N | move the focus between the monitors |
| `pan` | its part of one window onto the current desktop | every monitor switches to desktop N | slide the window |

In every mode `goto N` lands on desktop N's origin.

### `desktop-per-monitor`

What the rest of this README describes: a desktop's workspaces share a
monitor, and `output <side>` moves the whole desktop to the next one.

### The monitor grid

The other two modes place the monitors on a grid of their own, with an
origin, and lay the desktop over it.

```
 physical layout                  monitor grid
 [ DP-1 ][ eDP-1 * ][ DP-2 ]      DP-1 (-1,0)   eDP-1 (0,0)   DP-2 (1,0)
           * primary
```

The grid is read off the physical layout: the primary monitor is the origin,
and the others get their places from where they sit around it. Which monitor
is primary is up to `xrandr --primary`. To put a monitor somewhere else on
the grid, give it a place by hand:

```
--monitor-location=DP-2=0,1
```

A monitor that ends up with no place (because another one was given its
place) is left alone: nothing here changes what it shows.

### `location-per-monitor`

The grid stays on the desktop's origin, so each monitor always shows its own
location of the current desktop.

```
 monitor grid   LEFT (-1,0)   CENTRE (0,0)   RIGHT (1,0)

 on desktop 4   [ 4: ← ]      [>4<]          [ 4: → ]
 goto 5         [ 5: ← ]      [>5<]          [ 5: → ]
 focus left     [>5: ←<]      [ 5 ]          [ 5: → ]
 focus up       [>5: ↑<]      [ 5 ]          [ 5: → ]     no monitor has ↑
```

- An arrow works out its location as usual (see [Navigation](#navigation)).
  The focus goes to the monitor showing it, else to the monitor whose
  location it is. If no monitor has it, the monitor you are on shows it,
  until you navigate back or change desktop.
- `move` and `send` put the window on the workspace `focus` or `goto` would
  have focused. Nothing on screen changes.
- With no monitor at the grid's origin, `goto` leaves the focus on the
  monitor you are on.

**Pinning.** `pin-monitor` keeps the focused monitor on the workspace it
shows while the others go on changing desktop. The workspace's name gets a
` 📌` at the end, so the pin shows on the bar.

- A pinned monitor neither follows nor leads. Arrows can move the focus onto
  it; from it they lead back to the current desktop; one that would change
  what it shows does nothing.
- `goto` pressed on a pinned monitor switches the other monitors and moves
  the focus to the origin.
- `pin-monitor` again (or `pin-monitor off`) lets it go, and it rejoins the
  desktop the others show.
- A pin lasts while its workspace is on screen. Turning the monitor off, or
  showing another workspace on it with a plain i3 command, ends it.
- An i3 `assign` or `for_window` rule that names the plain workspace
  (`10:com`, while it is pinned as `10:com 📌`) makes i3 create a second one.
  The daemon moves its windows into the pinned workspace.

### `pan`

The grid slides over the desktop: an arrow moves every monitor one location,
and the focus stays on its monitor.

```
 monitor grid   LEFT (0,0)    RIGHT (1,0)

 on desktop 4   [ 4: ← ]      [>4<]
 focus right    [ 4 ]         [>4: →<]
 focus up       [ 4: ↑ ]      [>4: ↗<]
 goto 5         [>5<]         [ 5: → ]
```

- `--grid` and `--grid-bounds` don't apply: the window slides anywhere. A
  monitor over a location nobody has used shows an empty workspace, which
  i3 drops again once it is hidden, unless you put a window on it.
- `goto` and `focus origin` put the window back, the grid's origin on the
  desktop's origin. So does the origin hotkey, which still takes its arrow.
- `--location=X,Y` slides the window until the monitor you are on shows it.

### In both of these

- `output <side>` and, in `pan`, `pin-monitor` report an error.
- **The daemon follows the focus.** If another desktop gets the focus some
  other way (a click on the bar, `i3-msg workspace 5`), the other monitors
  come along. A workspace opened on the wrong monitor for its location is
  moved to its own. Without the daemon only this tool's commands do that.
- **`workspace back_and_forth`** goes to the desktop you came from, on every
  monitor. `workspace_auto_back_and_forth` has no effect on `goto` here.
- A `workspace N output X` line in the i3 config works against these modes.

## How it works

```
 you press $mod+Up
        │
        ▼
 i3 matches   bindsym $mod+Up nop i3-spatial-desktop focus up
        │
        ├── runs `nop …`: does nothing
        │
        └── sends a `binding` event over its IPC socket
            {"change":"run","binding":{"command":"nop i3-spatial-desktop focus up",…}}
                      │
                      ▼
            i3-spatial-desktop daemon: parses "focus up", sends i3 the real commands
            (`workspace "4:db: ↑"`, …) over a second connection
```

`nop` is i3's "no operation" command: i3 runs it and ignores everything after
it. Its only purpose here is to make a key press produce a `binding` event that
carries our arguments.

The daemon, started once by `exec_always`, keeps two connections to i3's
socket: one subscribed to events, one for sending commands. It handles:

- **`binding`**: the command of every key binding you press. Anything that
  isn't `nop i3-spatial-desktop …` is ignored. Commands chained with `;` or `,` and quoted
  arguments are parsed the way i3 parses them.
- **`workspace`**: when a workspace is created by anything else (a
  `for_window … move to workspace` rule, a script), it is moved to its
  desktop's monitor and the desktop is re-sorted, immediately. When an
  origin is renamed by anything else, its satellites are renamed to match
  (and a dropped number is put back). i3's rename event carries only the new
  name, so the daemon tracks workspace names by id.
- **`workspace`** focus changes and **`output`** changes, in
  `location-per-monitor` and `pan` only: the other monitors are brought in
  line with the one that has the focus; see
  [Several monitors](#several-monitors).
- **`shutdown`**: i3 is exiting or restarting; the daemon exits too.

Events are handled one at a time, in order, so a fast burst of keys can't race.

**What the daemon sees.** i3 reports *bound key combinations*, not typing: key
presses inside applications never reach i3's socket. It does report every
binding, not only i3-spatial-desktop's (i3 can't filter them), and the daemon discards
those without recording them. This isn't new access: any process running as
you can already connect to i3's socket, read your whole config and subscribe
to the same events. Other users can't, because the socket lives in your
private runtime directory.

**Lifecycle.**

| Event              | What happens                                                   |
|--------------------|----------------------------------------------------------------|
| i3 starts          | `exec_always` starts the daemon                                |
| i3 restarts        | the daemon exits on the `shutdown` event; `exec_always` starts a new one |
| i3 reloads         | the daemon keeps running (i3 doesn't re-run `exec_always`)     |
| a daemon starts    | it replaces any running one, through a control socket at `$XDG_RUNTIME_DIR/i3-spatial-desktop-<hash>.sock` |
| the daemon isn't running | the i3-spatial-desktop keys do nothing (`nop` really is a no-op)     |

Errors go to the daemon's stderr, which is i3's stderr (often
`~/.xsession-errors`). To capture them separately:

```
exec_always --no-startup-id ~/.cargo/bin/i3-spatial-desktop daemon 2>>/tmp/i3-spatial-desktop.log
```

Note that `nop` bindings fire only for real key presses; `i3-msg 'nop i3-spatial-desktop …'`
does not produce a binding event. From scripts, use the one-shot form below.

## Commands

The arguments after `nop i3-spatial-desktop`:

- **`focus <up|down|left|right|origin>`** or **`focus --location=X,Y`**: focus
  a workspace of the current desktop, following the [navigation](#navigation)
  options, or the one at an explicit location. `origin` always goes to the
  origin. Safe with `workspace_auto_back_and_forth`.
- **`move <up|down|left|right|origin>`** or **`move --location=X,Y`**: move
  the focused window to a workspace of the current desktop, chosen the same
  way. Focus stays put.
- **`output <left|right|up|down>`**: move the whole desktop to the
  neighbouring monitor. Focus follows. Does nothing at the edge (no wrap).
  Only in `desktop-per-monitor`.
- **`goto <NAME> [--location=X,Y]`**: focus a desktop's origin, never a
  satellite (`workspace number 4` would land on `4:db: ←` when `4:db` doesn't
  exist), or with `--location` the desktop's workspace there. NAME is used if
  it exists; otherwise an existing origin with the same number (`goto 1`
  finds `1:be`, also after a rename); otherwise the origin of a surviving
  satellite (`1:be: ←` → `1:be`); otherwise NAME is created.
- **`send <NAME> [--location=X,Y]`**: move the focused window to a desktop's
  origin (or the workspace at that location), resolved like `goto`. Focus
  stays put.
- **`rename-desktop <NAME>`**: rename the current desktop: its origin and
  every satellite, so `4:Foo: ↑` follows `4:Foo` to `4:Bar: ↑`. Works from
  any workspace of the desktop. A plain i3 rename of the origin does the same
  through the daemon; see [Renaming desktops](#renaming-desktops).

- **`pin-monitor [on|off|toggle]`**: keep the focused monitor on the
  workspace it shows, or let it follow again. Only in
  `location-per-monitor`; see [Several monitors](#several-monitors).

### Locations

`--location=X,Y` names a location directly: `0,0` is the origin, `-1,0` the
satellite left of it, `1,1` up and to the right. It must be on the grid (see [Navigation](#navigation));
locations the arrows could reach on your grid are accepted, others are
refused with `location (-2,0) is not on the grid`. The origin hotkey doesn't
matter here: with `--desktop-origin-hotkey=down`, `--location=0,-1` still
reaches `↓`, even though no arrow does.

Use the `=` form for negative numbers in i3 bindings (`--location=-1,0`);
`--location -1,0` also works on the command line.

### Renaming desktops

A desktop is named after its origin, and its satellites carry that name, so
renaming just the origin would leave the satellites behind as a separate,
orphaned desktop. i3-spatial-desktop renames the whole desktop either way:

- **A plain i3 rename of the origin** (recommended for a prompt). The daemon
  sees the origin renamed and renames the satellites to match. If the rename
  dropped the number (`4:Foo` → `Bar`), the daemon puts it back (`4:Bar`), so
  `$mod+4` keeps finding the desktop and you only need to type the label. A
  rename to a different number (`4:Foo` → `7:Bar`) is deliberate and left
  alone, as is a restore that would collide with another desktop. No extra
  process is started:

  ```
  bindsym $mod+Shift+R exec i3-input -F 'rename workspace to "%s"' -P 'Rename desktop: '
  ```

  This applies to any rename that drops a number, including a workspace with
  no satellites. Without the daemon running, plain renames orphan satellites.

- **`rename-desktop NAME`**, for scripts. Same result, checked up front: on
  desktop 4, `rename-desktop Bar` and `rename-desktop 4:Bar` both give
  `4:Bar`, while `7:Bar` is refused, as is a name another desktop already
  uses.

Your `$ws` variables don't need to change. `goto $ws4` with the old name
`4:Foo` still finds desktop 4 by its number while any of its workspaces
exist. Once all of them are gone (i3 deletes empty workspaces), the rename is
forgotten and `$mod+4` creates `$ws4` again.

## Options

Options go on the daemon line and apply to every binding:

```
exec_always --no-startup-id ~/.cargo/bin/i3-spatial-desktop --desktop-origin-hotkey=down daemon
```

| Flag                        | Default  | Meaning |
|-----------------------------|----------|---------|
| `--desktop-origin-hotkey`   | `none`   | `up`, `down` or `none`; see [Navigation](#navigation) |
| `--navigation-relative-to`  | `origin`; `focus` with a square or larger grid | `origin` or `focus` |
| `--grid`                    | `plus`   | `plus` or `square` |
| `--grid-bounds` / `--no-grid-bounds` | `1` | Steps from the origin the grid reaches, or no limit |
| `--render-workspace-location` | `arrows` | `arrows` or `coordinates`; see [Workspace names](#workspace-names) |
| `--sep`                     | `": "`   | Between the origin's name and the location |
| `--up` `--down` `--left` `--right` | `↑ ↓ ← →` | Arrow names |
| `--up-left` `--up-right` `--down-left` `--down-right` | `↖ ↗ ↙ ↘` | One-step diagonal arrow names |
| `--multimonitor-mode`       | `desktop-per-monitor` | `desktop-per-monitor`, `location-per-monitor` or `pan`; see [Several monitors](#several-monitors) |
| `--monitor-location=NAME=X,Y` | from the physical layout | A monitor's place on the monitor grid; once per monitor |

Changing the daemon line needs an i3 restart, not a reload. A flag inside a
single binding (`$rel --grid square focus up`)
overrides the daemon's for that key only.

Satellites are recognised under every navigation option and both
renderings, so switching those never orphans existing workspaces. Renaming
arrows (`--left` etc.) or changing `--sep` does: arrow-rendered satellites
are only recognised under the current names.

## Scripting

Every command also runs directly, without the daemon: it connects to i3, does
the one action (including the regrouping and re-sorting), and exits. Use this
from shell scripts, rofi/dmenu menus, bar click handlers, or anything else
that isn't a key press:

```sh
i3-spatial-desktop goto 4:db                 # focus origin 4:db (or whatever origin 4 is now)
i3-spatial-desktop goto 4:db --location=1,1  # its workspace up and to the right
i3-spatial-desktop rename-desktop Bar        # current desktop → 4:Bar, satellites included
i3-spatial-desktop focus left                # its ← satellite
i3-spatial-desktop send '10:com'             # send the focused window to 10:com
i3-spatial-desktop output right              # move the current desktop one monitor right
```

For example, a menu of the current desktop's workspaces:

```sh
choice=$(printf 'origin\nleft\nright\nup\n' | rofi -dmenu -p workspace) &&
  i3-spatial-desktop --desktop-origin-hotkey=down focus "$choice"
```

Things to know:

- **Options.** One-shot commands don't ask the daemon for its settings. Pass
  the same options the daemon uses: names must match for satellites to be
  recognised, the navigation options change where arrows lead, and
  `--multimonitor-mode` decides what every monitor shows.
- **Socket.** i3-spatial-desktop finds i3 through `$SWAYSOCK`, then `$I3SOCK`, then
  `i3 --get-socketpath` / `sway --get-socketpath`. Programs started by i3
  inherit `I3SOCK`; from elsewhere (cron, ssh) set it or `DISPLAY`.
- **Exit status.** 0 on success, including "nothing to do" (e.g. `focus
  right` when already on `→`, `output right` at the edge). 1 on failure, with
  the reason on stderr, e.g.
  `i3-spatial-desktop: command "move container to workspace \"4:db: ←\"" failed: Nothing
  to move: workspace empty`. 2 for invalid arguments.
- **Ordering.** A one-shot command is not queued behind key presses being
  handled by the daemon. That only matters if a script and your fingers act on
  the same desktop in the same instant.
- `i3-spatial-desktop --help` and `i3-spatial-desktop <command> --help` list every option.

## Guarantees

**One monitor per desktop,** in `desktop-per-monitor`. New workspaces
normally appear on the focused monitor, but a `workspace N output X` line in
your config can place them elsewhere. i3-spatial-desktop moves any workspace
that ends up away from its desktop back next to it.

**Bar order.** On each monitor: the origin's row first, then rows by
distance from it (above before below), each left to right. A plus gives `← | origin | → | ↑ | ↓`; a
square `← | o | → | ↖ | ↑ | ↗ | ↙ | ↓ | ↘`. Mixed renderings sort by
location together. i3 orders workspaces
that share a number by creation time, so an origin that was emptied (i3
deletes empty workspaces) and re-created would otherwise land after its
satellites. After every action, and whenever a workspace is created, the
daemon re-sorts the desktop by renaming each workspace to itself in order (i3
re-inserts a workspace on rename). An empty origin disappears from the bar
and comes back in its place.

## Sway

The same configuration works in sway. The differences: sway also re-runs
`exec_always` on reload, which simply replaces the daemon; and `include` has
been supported for much longer.

`location-per-monitor` and `pan` have only been tried on i3. Sway has no
primary output, so there the monitor grid's origin is the top-left monitor
unless `--monitor-location` says otherwise.

## Credits

This idea was inspired by a [video](https://demosthenes.org/tmp/StumpWMSpatialGroups.mp4) by and subsequent conversations with Russell Adams.
