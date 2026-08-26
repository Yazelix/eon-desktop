# Eon Desktop

Eon Desktop is the repository for Eon for desktop. Its Venus subsystem renders
structured presentation state authored by the Orbit subsystem in Eon Sessions
and sends semantic interaction back to the authoritative session runtime.

## Status

The Venus client implements one native Wayland window on Linux for an Eon workspace or one
standalone already-running local Orbit session. In workspace mode it renders
Eon-authored horizontal tabs and every fitting header in a one-expanded vertical
pane accordion. Tab headers show the numeric part of Eon's stable `tN` identity,
two spaces, and the leaf, `~`, or `/` derived from Eon's authoritative launch
directory. Visible live pane headers show Eon's opaque pane identity, a
two-space gutter, and a compact label for Orbit's working directory, using a home
marker at `HOME`; unset or empty `HOME` leaves paths absolute, while
only the selected endpoint receives presentation and input. When Eon publishes
one tab-bound directory-picker endpoint, Venus keeps the tab bar visible and
replaces the tab body with that terminal inside a one-cell inset. It
automatically recovers that attachment after retryable local socket loss, detaches
without ending any Session, and does not own a PTY, terminal emulator, or
workspace topology.

## Ownership

```text
Eon                     -> product policy, composition, distribution
Eon Desktop / Venus     -> native presentation, interaction, client failure UX
Eon Sessions / Orbit    -> PTYs, terminal state, session lifetime, wire authority
```

Venus consumes EONW v3 through `eon-workspace-protocol` 0.1.0 at exact Eon source
`96119f29ca2e3ec4ad19bbe272708b07d588429a`. Eon alone owns workspace order,
selection, identities, tab launch directories, directory-picker lifecycle,
actions, and pane-to-Session mappings. Venus consumes
`orbit-protocol` 0.1.0, ORBF v1, and ORBS v10 at exact Orbit proof
`59975e9176f5caf8b78dc3273e88d9ecbb75dc3f`. One reducer turns complete canonical
frames into immutable scene data used by drawing and accessibility. The native
host owns the local socket, window, input mapping, and redraw lifecycle; it owns
no terminal state.

An Eon Workspace is composition, not another Session. Each pane references an
independent Orbit Session. Venus keeps the EONW connection for workspace state
and actions, one presentation connection to the selected pane, and one read-only
metadata observer for each visible live pane. Hidden and offline Sessions have no
Venus observer and remain alive independently.

## Run

Pass the Eon Sessions Orbit Unix socket to the Venus client:

```sh
cargo run --locked -- /path/to/orbit.sock
```

Venus requires a native Wayland display and fails before presentation
attachment when one is unavailable. X11, Xwayland, and macOS are unsupported.

Without an argument, Venus uses
`$XDG_RUNTIME_DIR/yazelix-orbit/orbit.sock`, or
`/tmp/yazelix-orbit-$UID/orbit.sock` when the runtime directory is unavailable.
Only one presentation client can attach to an Orbit session at a time.
Venus may start before Orbit. A missing, refused, reset, or dropped local socket
retries after 250 ms, 500 ms, 1 s, 2 s, 4 s, and then every 5 s. Venus retains
the last coherent scene during recovery and replaces it only with a fresh
complete frame. Busy, incompatible, exited, invalid protocol or model, resource,
queue, input, and worker-start failures remain terminal and visible.

For an Eon workspace, pass the initial Orbit socket followed by the EONW socket:

```sh
cargo run --locked -- /path/to/orbit.sock /path/to/eon.sock
```

Pass `--application-id ID` before the sockets when the caller owns a distinct
desktop identity. The bounded ASCII token becomes the Wayland app ID before
window creation and does not replace Orbit-authored window titles. Direct Venus
launches use `eon`:

```sh
cargo run --locked -- --application-id eonova /path/to/orbit.sock
```

Pass `--no-decorations` to request a window without its native title bar. The
default remains decorated:

```sh
cargo run --locked -- --no-decorations /path/to/orbit.sock /path/to/eon.sock
```

Pass `--background-opacity VALUE` with a finite value from `0.0` through `1.0`
to control the terminal default background. Venus uses `1.0` when the option is
absent. This example uses the Nova-selected opacity:

```sh
cargo run --locked -- --background-opacity 0.88 /path/to/orbit.sock
```

The opacity follows Orbit-authored default background changes and covers empty
terminal padding. Explicit cell backgrounds, selection, inverse video,
workspace chrome, notices, focus borders, text, and cursors retain their
existing presentation. Visual transparency does not change pointer or keyboard
input. Venus rejects invalid values during launch and rejects translucent
launches before presenting its first frame when the native surface lacks
premultiplied alpha support. Winit does not expose Wayland window visibility
control. Eon owns persistence and product defaults for composed launches.

Pass `--background-blur` to ask the native compositor for full-surface blur at
window creation, before the first frame. Blur and opacity are independent: an
opaque default background hides the effect, while lower opacity reveals it. This
COSMIC Wayland example leaves terminal-default pixels fully transparent:

```sh
cargo run --locked -- \
  --background-opacity 0.0 \
  --background-blur \
  /path/to/orbit.sock
```

COSMIC owns the blur algorithm and strength. Unsupported or policy-disabled
Wayland compositors may ignore the best-effort request without failing launch.
Venus does not configure blur strength.

Direct Venus launches use a tail with color `#89b4fa` and duration multiplier
`1.0` when the cursor profile is absent. Pass `--cursor-effect-v1 none` for a
static cursor. A complete explicit tail profile requires one `#RRGGBB` trail
color and a finite duration multiplier from `0.25` through `4.0`:

```sh
cargo run --locked -- \
  --cursor-effect-v1 tail \
  --cursor-trail-color-v1 '#89b4fa' \
  --cursor-trail-duration-v1 1.0 \
  /path/to/orbit.sock
```

The tail animates only its bounded geometry; Orbit remains authoritative for
the cursor destination, shape, visibility, blink state, wide-cell geometry, and
cursor color. Venus rejects duplicate, incomplete, malformed, non-finite, or
out-of-range profile values before opening a window. Yazelix Cursors owns value
resolution and Eon owns serialization and persistence for composed launches;
that Eon producer is tracked separately and is not part of the current launcher.

In Eon's supervised mode, Venus reads one private bounded presentation stream.
Each complete Present command keeps the same Venus process and terminal
attachment and asks the native window system to present its existing window.
If that stream closes or fails, Venus exits without stopping Orbit and releases
its presentation attachment; an immediate supervised replacement retries a
transient Busy while the departing client releases it. Standalone Busy remains
terminal. Direct focus and unminimize are unavailable through winit on Wayland;
xdg activation remains compositor-controlled.

The accepted Eon snapshot supplies the authoritative selected Orbit endpoint.
While the window is open, Venus re-inspects Eon every 250 ms so accepted
workspace changes from another client appear without a click or restart.
Recovery continues only while that exact selected pane remains live; endpoint
replacement or authoritative offline state cancels obsolete retry state.
Click a tab or pane header to select it. Alt+H/L walks tabs, Alt+K/J walks panes,
Alt+M creates a pane, Ctrl+T creates a tab, and Alt+Z requests Eon's directory
picker. While the picker is active, its terminal receives input and the prior
workspace focus is restored when it closes. Press F6 to cycle terminal, tab, and
pane keyboard focus; Left/Right on tabs, Up/Down on panes, and Escape remain
available. Tab headers show `N  leaf`, `N  ~`, or `N  /` from Eon's launch
directory while hit testing and actions retain `tN`; accessibility pairs `tN`
with a bounded full path. Pane headers show pane identity, two spaces, then the
home marker at exact home, a `~/`-anchored path below home, or an absolute path elsewhere;
unset or empty `HOME` keeps paths absolute. Overlong labels preserve their
rightmost components. Terminal titles stay in
the selected native window, and Session mappings remain in Eon diagnostics.
Wheel over the tab strip or a pane header to reach clipped headers without
scrolling the terminal. In standalone mode these keys remain Orbit input.

Precision touchpad movement tracks Orbit-owned retained history at twice its
native pixel distance, and a complete gesture may continue with bounded momentum
after release. A bounded Orbit-authored row window keeps multi-row movement
continuous while signed commits are in flight. Discrete wheel steps move three
retained rows without synthetic momentum. Terminal-owned mouse modes continue to
receive their canonical Orbit input instead. Hold Shift while dragging the left
mouse button to bypass that capture. Drag to select cells, double-click to
select words, or triple-click to select logical lines. Releasing writes Orbit's
exact bounded text to both the ordinary Wayland clipboard and primary
selection; Ctrl+Shift+C remains an explicit ordinary-clipboard copy.
Press Ctrl+Shift+V or the native Paste key to read the ordinary clipboard once.
Orbit applies normal or bracketed paste from its authoritative terminal mode.
Terminal programs can also request bounded text writes through Orbit. On Linux,
Venus sends the standard destination to the ordinary clipboard and sends the
selection or primary destination to the primary clipboard.

## Architecture and evidence

- [`docs/CONTRACTS.md`](docs/CONTRACTS.md) indexes Venus subsystem behavior and proof.
- [`docs/REFERENCES.md`](docs/REFERENCES.md) records the exact architectural
  evidence used by the implementation gate.
- [`docs/CRATES.md`](docs/CRATES.md) records the measured dependency selection,
  owner seams, and rejected alternatives.

```sh
cargo fmt --check
cargo check --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

## Exclusions

Arbitrary split trees, simultaneous expanded panes, reordering, sidebars,
popups, persistent Venus configuration, blur strength or live blur changes,
background images, plugins, remote and web access, packaging, and distribution
are outside this slice.

The Linux host uses winit, wgpu, glyphon, AccessKit, and wl-clipboard-rs. It
selects native Wayland and Vulkan only. The exact
`59975e9176f5caf8b78dc3273e88d9ecbb75dc3f` Orbit package revision supplies
accepted ORBS v10, including authoritative selection completion, routed native
left-pointer gestures, bounded row-window previews, signed scroll batches, and
read-only pane metadata, and resolves from GitHub.

## LOC scorecard

The scorecard counts tracked project text and code, including rendered
`AGENTS.md` because agents consume it directly. It excludes `.git/`, Beads data,
lock files, and other generated artifacts.

| Surface | Lines |
|---|---:|
| Agent policy | 416 |
| README | 234 |
| Contracts and references | 907 |
| Crate decisions | 165 |
| Changelog | 142 |
| Rust source, including unit tests | 12,656 |
| Rust integration tests | 782 |
| Cargo manifest | 23 |
| **Total** | **15,325** |
