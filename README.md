# Eon Desktop

Eon Desktop is the repository for Eon for desktop. Its Venus subsystem renders
structured presentation state authored by the Orbit subsystem in Eon Sessions
and sends semantic interaction back to the authoritative session runtime.

## Status

The Venus client implements one Linux window for an Eon workspace or one
standalone already-running local Orbit session. In workspace mode it renders
Eon-authored horizontal tabs and every fitting header in a one-expanded vertical
pane accordion, routes selection back as semantic Eon actions, and attaches only
the selected live Orbit endpoint. It automatically recovers that attachment after
retryable local socket loss, detaches without ending any Session, and does not own
a PTY, terminal emulator, or workspace topology.

## Ownership

```text
Eon                     -> product policy, composition, distribution
Eon Desktop / Venus     -> native presentation, interaction, client failure UX
Eon Sessions / Orbit    -> PTYs, terminal state, session lifetime, wire authority
```

Venus consumes EONW v1 through `eon-workspace-protocol` 0.1.0 at exact Eon proof
`4af395aea06c230ee6b18cf0755ae25915c0b88d`. Eon alone owns workspace order,
selection, identities, actions, and pane-to-Session mappings. Venus consumes
`orbit-protocol` 0.1.0, ORBF v1, and ORBS v4 at exact Orbit proof
`7f067b30e97d0b4787a7c6c0bbe3dd8a80a61c2c`. One reducer turns complete canonical
frames into immutable scene data used by drawing and accessibility. The native
host owns the local socket, window, input mapping, and redraw lifecycle; it owns
no terminal state.

An Eon Workspace is composition, not another Session. Each pane references an
independent Orbit Session. Venus keeps the EONW connection for workspace state
and actions plus one Orbit connection to the selected pane; inactive Sessions
remain alive without a Venus connection.

## Run

Pass the Eon Sessions Orbit Unix socket to the Venus client:

```sh
cargo run --locked -- /path/to/orbit.sock
```

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
launches before showing the window when the native surface lacks premultiplied
alpha support. Eon owns persistence and product defaults for composed launches.

Pass `--background-blur` to ask the native compositor for full-surface blur
before Venus shows the window. Blur and opacity are independent: an opaque
default background hides the effect, while lower opacity reveals it. This
COSMIC Wayland example leaves terminal-default pixels fully transparent:

```sh
cargo run --locked -- \
  --background-opacity 0.0 \
  --background-blur \
  /path/to/orbit.sock
```

COSMIC owns the blur algorithm and strength. Unsupported or policy-disabled
compositors may ignore the best-effort request without failing launch. Venus
does not configure blur strength, and X11 and macOS blur remain unproved.

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
Alt+M creates a pane, and Ctrl+T creates a tab. Press F6 to cycle terminal, tab,
and pane keyboard focus; Left/Right on tabs, Up/Down on panes, and Escape remain
available. Pane headers show pane identity and offline state; Session mappings
remain in Eon diagnostics. Wheel over the tab strip or a pane header to reach
clipped headers without scrolling the terminal. In standalone mode these keys
remain Orbit input.

Wheel or trackpad movement scrolls through Orbit-owned retained history. Hold
Shift while dragging the left mouse button to select cells, then press
Ctrl+Shift+C to copy the exact bounded text returned by Orbit.
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
background images, plugins, remote and web access, macOS implementation,
packaging, and distribution are outside this slice.

The Linux host uses winit, wgpu, glyphon, AccessKit, and an isolated arboard
text-clipboard effect. macOS remains an architectural target, not an
implemented or proved platform. The exact
`7f067b30e97d0b4787a7c6c0bbe3dd8a80a61c2c` Orbit package revision supplies
accepted ORBS v4 and ORB-C12 and resolves from GitHub.

## LOC scorecard

The scorecard counts tracked project text and code, including rendered
`AGENTS.md` because agents consume it directly. It excludes `.git/`, Beads data,
lock files, and other generated artifacts.

| Surface | Lines |
|---|---:|
| Agent policy | 416 |
| README | 197 |
| Contracts and references | 330 |
| Crate decisions | 157 |
| Changelog | 90 |
| Rust source, including unit tests | 9,118 |
| Rust integration tests | 612 |
| Cargo manifest | 23 |
| **Total** | **10,943** |
