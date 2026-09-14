# Eon Desktop

Eon Desktop is the repository for Eon for desktop. Its Venus subsystem renders
structured presentation state authored by the Orbit subsystem in Eon Sessions
and sends semantic interaction back to the authoritative session runtime.

## Status

The Venus client implements one native Wayland window on Linux for an Eon workspace or one
standalone already-running local Orbit session. In workspace mode it renders
Eon-authored horizontal tabs and every fitting header in a one-expanded vertical
pane accordion. Tab headers show their current one-based position, two spaces,
and the leaf, `~`, or `/` derived from Eon's authoritative launch directory.
Pill-shaped tabs fit their labels, with middle ellipsis for long names
and full launch-path previews on hover. Selected tabs use a brighter fill and
label without an underline; keyboard tab focus adds a rounded outline.
Pane headers use quiet backgrounds and a lighter selected fill and label. One
rounded frame connects the pane stack, with full-width separators and a small
bottom gap sharing the terminal background. Selection and hover fills follow the
stack corners; keyboard focus adds a rounded accent outline.
Visible live pane headers show Eon's opaque pane identity, a
two-space gutter, and a compact label for Orbit's working directory, using a home
marker at `HOME`; unset or empty `HOME` leaves paths absolute, while
only the selected endpoint receives presentation and input. A fresh workspace
and every new tab may begin with no pane while Eon publishes a Project popup.
Tool popups and the project chooser share one rounded terminal surface covering
the stack. Its outer frame keeps the pane stack's exact edges while Eon-supplied
logical margins inset terminal content beneath its compact border label. Tabs
remain visible and actionable. A tab containing only hidden popups has an empty body
and keeps keyboard/accessibility focus on its selected tab until it reopens
retained work through the catalog shortcut. Venus
automatically recovers that attachment after retryable local socket loss, detaches
without ending any Session, and does not own a PTY, terminal emulator, or
workspace topology.

## Ownership

```text
Eon                     -> product policy, composition, distribution
Eon Desktop / Venus     -> native presentation, interaction, client failure UX
Eon Sessions / Orbit    -> PTYs, terminal state, session lifetime, wire authority
```

Venus consumes EONW v5 through `eon-workspace-protocol` 0.1.0 at exact Eon source
`0cc8f477298681ae3945903e8fdb5852d487c5ab`. Eon alone owns workspace order,
optional selection, pending-tab state, identities, tab launch directories,
popup catalog, geometry settings, commands, lifecycle, actions, and Session mappings.
This consumer is active in Eon's current v5 runtime, whose component graph pins
Venus source `d212ff911c18cf0c1cd0f6b7e3f48a2e01d78d86`. Venus consumes
`orbit-protocol` 0.1.0, ORBF v2, and ORBS v11 at exact Orbit proof
`ea9fd28ce0908f218cf65d4e6df368f0a4e565f5`. One reducer turns complete canonical
frames into immutable scene data used by drawing and accessibility. The native
host owns the local socket, window, input mapping, and redraw lifecycle; it owns
no terminal state.

An Eon Workspace is composition, not another Session. Each pane references an
independent Orbit Session. Venus keeps the EONW connection for workspace state
and actions, one presentation connection to the selected popup or pane, and one read-only
metadata observer for each visible live pane. Hidden and offline Sessions have no
Venus observer and remain alive independently.

## Run

Pass the Eon Sessions Orbit Unix socket to the Venus client:

```sh
cargo run --locked -- /path/to/orbit.sock
```

On Linux, Venus requires a native Wayland display and fails before presentation
attachment when one is unavailable. This is the only proved host. The Apple
Silicon macOS implementation has a native-proved opaque AppKit, AccessKit, and
Metal foundation, but remains unsupported until the rest of VEN-C16's proof
succeeds. X11, Xwayland, and Intel macOS remain unsupported.

### Typography and initial size

Startup options apply to standalone and workspace surfaces:

```sh
cargo run --locked -- \
  --font-family 'DejaVu Sans Mono' \
  --font-fallback 'Symbols Nerd Font Mono' \
  --font-size 20 --line-height 1.5 \
  --columns 100 --rows 30 /path/to/orbit.sock
```

The primary family must be installed and monospace. Repeat `--font-fallback`
for up to eight installed families in preference order; platform fallback
follows them. Names are nonempty trimmed UTF-8, at most 128 bytes without
controls. Missing named fonts fail launch. Font availability does not promise
all glyphs or style variants; Venus does not install fonts.

`--font-size` accepts 6–96 nominal logical pixels, and `--line-height` accepts
1–3 times that size. Absence preserves 16 px nominal text and 10 by 18 logical
cells. One rounded physical cell grid drives text, cursor, pointer, selection,
scrolling, IME and accessibility.

Positive `--columns` and `--rows` request a terminal grid, including space for
workspace headers or popup margins. An omitted dimension retains its existing
initial window dimension; the default window is 960 by 600 logical pixels.
Requests must fit Orbit's 100,000-cell and native surface limits. The compositor
may override initial sizing, and later user resizing remains unconstrained.
Typography must leave room for a terminal cell in the initial window, including
workspace headers, even when columns and rows are omitted.
These are startup options; Eon owns persistent product configuration.

### Hyperlinks

Hover an explicit OSC 8 link to highlight it and preview its actual target.
Ctrl+left click opens it. Ctrl+Shift+C copies the hovered target unless the
terminal has selected text, in which case it copies that selection. Ordinary
clicks retain terminal mouse reporting and selection behavior; ordinary
URL-looking text is not detected as a link. The accessibility tree exposes each
current visible target as an Open link followed by a Copy button. A changed
frame retires both. Hover and actions pause during scroll animation and renderer
recovery.

Opening accepts ASCII HTTP/HTTPS targets up to 4096 bytes, with a host and
without credentials. Other schemes, malformed targets and oversized links
produce an accessible notice. Copy accepts target text up to the same limit
without control characters, including schemes that cannot be opened.
The native Linux host must provide `gio` on PATH and a registered HTTP/HTTPS
handler. Venus passes one exact URI argument without a shell, allows one
dispatch at a time, and retires a stalled dispatcher after ten seconds.
Copy does not require GIO. Broader compositor and fractional-scale proof remain
open.

### Attachment

Without an argument, Venus uses
`$XDG_RUNTIME_DIR/yazelix-orbit/orbit.sock`, or
`/tmp/yazelix-orbit-{effective-uid}/orbit.sock` otherwise.
Only one presentation client can attach to an Orbit session at a time.
Venus may start before Orbit. A missing, refused, reset, or dropped local socket
retries after 250 ms, 500 ms, 1 s, 2 s, 4 s, and then every 5 s. Venus retains
the last coherent scene during recovery and replaces it only with a fresh
complete frame. Busy, incompatible, exited, invalid protocol or model, resource,
queue, input, and worker-start failures remain terminal and visible.
Routine workspace attachment and first-frame progress are silent; standalone
attachment retains progress messages.

For an Eon workspace, select workspace mode with only the EONW socket:

```sh
cargo run --locked -- --workspace /path/to/eon.sock
```

Pass `--pane-frames false` to hide decorative pane borders. The default is
`true`; both modes keep the same terminal grid, pane headers, click targets and
accessible bounds. Keyboard focus and hover remain visible with frames off.
Missing, duplicate or invalid boolean values fail before window creation.
Standalone and popup presentation have no pane frames; popups retain their own
rounded outline. This is a
startup option; Eon owns persistent product configuration.

Workspace mode opens the EONW transport first and waits for its accepted
snapshot before attaching its terminal endpoint. The first snapshot may select
a popup or have no visible terminal; no initial Orbit endpoint, durable pane,
or placeholder Session is required.

Pass `--application-id ID` before the launch target when the caller owns a
distinct desktop identity. The bounded ASCII token becomes the Wayland app ID
before window creation and does not replace Orbit-authored window titles.
Direct Venus launches use `eon`:

```sh
cargo run --locked -- --application-id eonova /path/to/orbit.sock
```

Pass `--no-decorations` to request a window without its native title bar. The
default remains decorated:

```sh
cargo run --locked -- --no-decorations --workspace /path/to/eon.sock
```

Pass `--background-opacity VALUE` with a finite value from `0.0` through `1.0`
to control the terminal default background. Venus uses `1.0` when the option is
absent. This example uses the Nova-selected opacity:

```sh
cargo run --locked -- --background-opacity 0.88 /path/to/orbit.sock
```

The opacity follows Orbit-authored default background changes and covers empty
terminal padding and exposed popup margins. Explicit cell backgrounds, selection, inverse video,
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
The `stdin-ready-v1` mode consumes one canonical EONW v5 startup snapshot for
workspace launches, then reports `ready-v1` on stdout after admitting fonts and
the actual window's initial native geometry, before terminal attachment. Eon
bounds that exchange and starts new commands only after readiness.
Each complete Present command keeps the same Venus process and terminal
attachment and asks the native window system to present its existing window.
If that stream closes or fails, Venus exits without stopping Orbit and releases
its presentation attachment; an immediate supervised replacement retries a
transient Busy while the departing client releases it. Standalone Busy remains
terminal. Direct focus and unminimize are unavailable through winit on Wayland;
xdg activation remains compositor-controlled.

The accepted Eon snapshot supplies the authoritative terminal endpoint: the
active tab's selected popup, otherwise its selected Orbit pane. An empty body
detaches the presentation without stopping hidden Sessions.
While the window is open, Venus re-inspects Eon every 250 ms so accepted
workspace changes from another client appear without a click or restart.
Recovery continues only while that exact attachment remains current and live;
endpoint replacement or authoritative offline state cancels obsolete retry
state.
Click a tab or pane header to select it. Alt+1 through Alt+9 select tab positions
1 through 9, and Alt+0 selects position 10; missing positions do nothing.
Alt+H/L walks every tab, Alt+K/J walks panes,
Ctrl+Alt+H/L moves the active tab, Ctrl+Alt+K/J moves the selected pane,
Alt+Shift+W closes the expected active non-final tab, Alt+M creates a pane,
Alt+Shift+T requests a pending tab. Eon's catalog supplies popup shortcuts,
including Alt+Z for Project. A shortcut invokes its exact tab, entry and current
instance: Toggle from terminal focus, Focus from workspace chrome. Repeated
press events do not create duplicate structural actions. Eon decides whether
each action is available and owns dismissal, cwd changes and command lifetime.
Switching tabs retains each tab's popup selection. F6 cycles terminal and tabs,
plus panes when visible; Left/Right on tabs and Up/Down on panes traverse them.
Escape returns chrome focus to the terminal. While the terminal has focus,
Escape, Ctrl+C, Tab and Enter reach its application through ordinary Orbit input.
Alt+/ opens a native Shortcuts dialog for the Eon surface. It uses the same
fixed binding descriptors as dispatch and appends the enabled Project/tool rows
from the current Eon catalog; child-application bindings remain in those apps.
Wheel, Up/Down, Page Up/Down, Home and End scroll the list. Escape or Alt+/
closes it and restores the prior terminal or chrome focus without sending a
workspace action or terminal key.
Popup margins shrink to preserve a usable cell grid; tiny surfaces omit the
label before clipping terminal content. Tab headers show the current positional
`N  leaf`, `N  ~`, or `N  /` from Eon's launch directory while hit testing and
actions retain stable `tN`; accessibility and hover details pair the current
position with its full launch path. Tab widths follow shaped text plus padding up to
280 logical pixels at default typography; larger fonts scale that limit.
Long labels preserve both ends without cutting shaped clusters. If the name and
ellipsis cannot fit, the tab keeps its number whenever that fits. Hover previews
wrap the path within the window. Shell `cd` changes pane metadata, while tab
names and widths stay tied to the launch directory.
Pane headers show pane identity, two spaces, then the
home marker at exact home, a `~/`-anchored path below home, or an absolute path elsewhere;
unset or empty `HOME` keeps paths absolute. Overlong labels preserve their
rightmost components. Terminal titles stay in
the selected native window, and Session mappings remain in Eon diagnostics.
Wheel over the tab strip, including its gaps, or a pane header to reach clipped
headers without scrolling the terminal. In standalone mode these keys remain Orbit input.

Compatible terminal output keeps scrolling, selection, and tab/pane focus usable
between repaints. Venus retains the last presented input geometry while
refreshing content; Orbit continues parsing output and owns the anchored
history viewport. Resize, screen, workspace, and attachment changes still
require fresh presentation.

While scrolled, `↑ N rows` shows the last committed viewport's wrapped display
rows above live output (`↑ 1 row` for one). The selected pane header reserves
space for it, preserving the directory ending when the label needs shortening.
Standalone and popup terminals use a small top-right overlay
without resizing the grid; it yields to selection, link previews, notices,
popup tab previews, and an overlapping terminal cursor. Live bottom, alternate screen, recovery, pending reflow,
and known terminal-owned scrolling hide it. Fractional preview movement never
changes the number. If the complete label cannot fit, it stays available in the
terminal's accessible description; digits are never truncated. The description
is not a live alert.

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
- The [memory comparison](docs/benchmarks/venus-memory-2026-09-08.md) records
  the cell-buffer allocation savings and their environment limits.

```sh
cargo fmt --check
cargo check --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

The application regression also needs an isolated native Wayland display and
Vulkan renderer. Point `XDG_RUNTIME_DIR` and `WAYLAND_DISPLAY` at that display,
and `TMPDIR` at disposable proof storage, then run:

```sh
timeout 30s cargo test --locked --bin yazelix-venus live_output_keeps_application_input_admitted_before_repaint -- --ignored
```

## Exclusions

Arbitrary split trees, simultaneous expanded panes, sidebars,
popup command/lifetime policy, persistent Venus configuration, blur strength or live blur changes,
background images, plugins, remote and web access, packaging, and distribution
are outside this slice.

The proved Linux host uses winit, wgpu, glyphon, AccessKit, and wl-clipboard-rs
with native Wayland and Vulkan. The unproved Apple Silicon host reuses the same
renderer/model path with AppKit, AccessKit, and Metal; native interaction,
pasteboard, lifecycle acceptance, effects, and composition remain open. The exact
`ea9fd28ce0908f218cf65d4e6df368f0a4e565f5` Orbit package revision supplies
accepted ORBF v2 / ORBS v11, including authoritative scrollback position,
selection completion, routed native
left-pointer gestures that survive compatible live output, bounded row-window
previews, signed scroll batches, and read-only pane metadata. The exact source
revision must be available in the Git checkout cache or published to GitHub.

## LOC scorecard

The scorecard counts tracked handwritten text and code. It excludes `.git/`,
Beads data, lock files, and generated artifacts, including rendered `AGENTS.md`,
benchmark CSV data, and disposable qualification patches.

| Surface | Lines |
|---|---:|
| Agent policy inputs | 216 |
| README | 384 |
| Repository attributes and ignore rules | 7 |
| Contracts and references | 1,775 |
| Memory benchmark report | 158 |
| Crate decisions | 268 |
| Changelog | 257 |
| Rust source, including unit tests | 18,015 |
| Rust integration tests | 1,034 |
| Cargo manifest | 32 |
| **Total** | **22,146** |
