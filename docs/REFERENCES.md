# Venus reference routing

References answer a named question; they do not select dependencies or authorize
source reuse. Before code, the implementing Bead records exact releases or
commits, useful mechanisms, rejected surrounding scope, license compatibility,
and consequences for ownership, code shape, and the first check.

## Required before the first client implementation

- Orbit `ORB-C1` through `ORB-C6`, ORBF v1, ORBS v1, and the dependency-free
  `orbit-protocol` 0.1.0 package are accepted at
  `c905bf9610581747f1b07565814b501ca66cfaa6`; `ORB-C7` is partially proved at
  that revision. Venus must consume the package at that exact Git revision and
  must not mirror either codec or its semantic values.
- [FrankenTUI](https://github.com/Dicklesworthstone/frankentui) is required for
  backend boundaries, deterministic rendering transitions, and test
  architecture. Inspect its
  [presenter-emission ADR](https://github.com/Dicklesworthstone/frankentui/blob/main/docs/adr/ADR-002-presenter-emission.md),
  [terminal-backend ADR](https://github.com/Dicklesworthstone/frankentui/blob/main/docs/adr/ADR-003-terminal-backend.md),
  [backend-strategy ADR](https://github.com/Dicklesworthstone/frankentui/blob/main/docs/adr/ADR-008-terminal-backend-strategy.md),
  backend traits, headless backend, golden checks, and property-test structure.
  Venus accepts independently justified backend seams, deterministic patch or
  damage transitions, and test patterns—not its TUI, widget, ANSI-presenter, or
  runtime architecture.
- [OpenTUI Rust](https://github.com/Dicklesworthstone/opentui_rust) is required
  composition-model evidence. Inspect its buffer layering, clipping, alpha,
  draw-buffer composition, hit grid, double buffering, and separation between
  the rendering engine and the host application loop. Venus does not assume a
  cell-grid renderer, terminal output backend, or OpenTUI dependency.
- [Ghostling](https://github.com/ghostty-org/ghostling) and
  [libghostty-rs](https://github.com/Uzaaft/libghostty-rs) remain required
  ownership references for native windows, glyphs, GPU integration, callbacks,
  and render-loop boundaries. They do not authorize a second terminal state in
  Venus.

`ven-upt.1` completed this gate at FrankenTUI
`479436597890a14e82676d0067e3917b2a9de8f5`, OpenTUI Rust
`a37bed1d2569fdd5d3bc33ec19c0678cd0cb1cd8`, Ghostling
`f9034e43a50a2f3a8101e35497f486090c1ddd6e`, and libghostty-rs
`72ac98f292879bf9f788fcbb11238c562a1eebe6`. Implementation work must reread
the complete Bead evidence rather than treating these identities as selected
dependencies.

The Jeffrey Emanuel references carry nonstandard licensing. The user explicitly
directed exact-commit inspection for architecture ideas and independently
directs and benefits from it; tool choice alone does not satisfy a restriction
on acting for a named provider. Inspection does not authorize copying,
adaptation, redistribution, incorporation, or dependency selection, which
remain separate license and user-approval gates.

## Selected crate evidence and comparisons

- [winit 0.30.13](https://docs.rs/winit/0.30.13/winit/) owns the window and
  native-event boundary. Its
  [`EventLoopProxy`](https://docs.rs/winit/0.30.13/winit/event_loop/struct.EventLoopProxy.html)
  carries typed wakeups from transport work into the application loop.
- [wgpu 30.0.0](https://docs.rs/wgpu/30.0.0/wgpu/) owns GPU access. Its
  [`CurrentSurfaceTexture`](https://docs.rs/wgpu/30.0.0/wgpu/enum.CurrentSurfaceTexture.html)
  distinguishes success, occlusion, timeout, outdated configuration, and loss
  for bounded renderer recovery.
- [glyphon 0.12.0](https://docs.rs/glyphon/0.12.0/glyphon/) integrates text
  preparation, clipping, raster caches, and mask or color atlases with wgpu 30.
  Its [cosmic-text 0.19.0](https://docs.rs/cosmic-text/0.19.0/cosmic_text/)
  re-export supplies advanced shaping and font fallback. Corpus and native
  checks must still prove terminal-cell fidelity and atlas failure behavior.
- [COSMIC Terminal](https://github.com/pop-os/cosmic-term/tree/7daf10e3b540f612cbc48973469656ecd1635dfc)
  at `7daf10e3b540f612cbc48973469656ecd1635dfc` is conditional comparison-only
  evidence for terminal shaping, bidirectional or ligature behavior, wide-cell
  projection, decoration geometry, input methods, and renderer backend parity.
  Its pinned [`terminal.rs`](https://github.com/pop-os/cosmic-term/blob/7daf10e3b540f612cbc48973469656ecd1635dfc/src/terminal.rs)
  and [`terminal_box.rs`](https://github.com/pop-os/cosmic-term/blob/7daf10e3b540f612cbc48973469656ecd1635dfc/src/terminal_box.rs)
  demonstrate measured monospace width, bounded shape-run caching, and glyph-run
  decoration projection. Venus rejects its PTY, parser, terminal-grid, workspace,
  selection, clipboard, search, configuration, and unfinished damage ownership.
  COSMIC Terminal is [GPL-3.0-only](https://github.com/pop-os/cosmic-term/blob/7daf10e3b540f612cbc48973469656ecd1635dfc/LICENSE):
  inspection does not authorize copying, adaptation, incorporation, or dependency
  selection, each of which requires its own license and dependency gate.
- [Sugarloaf](https://github.com/raphamorim/rio/tree/main/sugarloaf) is
  conditional on a demonstrated need for its renderer shape.
- [AccessKit 0.24.1](https://docs.rs/accesskit/0.24.1/accesskit/) and
  [accesskit_winit 0.33.2](https://docs.rs/accesskit_winit/0.33.2/accesskit_winit/)
  own native accessibility adaptation. The
  [`ActivationHandler`](https://docs.rs/accesskit/0.24.1/accesskit/trait.ActivationHandler.html)
  requires a real tree by the next display refresh even when the application
  would skip rendering. Venus derives that tree from the accepted scene;
  pointer hit testing follows the last-presented scene.
- [softbuffer 0.4.8](https://docs.rs/softbuffer/0.4.8/softbuffer/) remains the
  software comparison. Its CPU buffer supports damage on selected platforms,
  while AppKit presentation requires a blocking copy.
- [Vello 0.9.0](https://docs.rs/vello/0.9.0/vello/) with
  [Parley 0.11.0](https://docs.rs/parley/0.11.0/parley/) remains the rich-vector
  comparison. Vello identifies its renderer as alpha and requires GPU compute;
  Parley supplies broader rich-text layout than the accepted grid needs.

Compare complete ownership shapes, not isolated crates. The gate measures text
correctness, input methods, accessibility, Linux behavior, macOS feasibility,
future browser implications, owned LOC, dependency/build cost, and maintenance.

The user selected exact-revision Orbit protocol consumption with winit 0.30.13,
wgpu 30.0.0, glyphon 0.12.0 and its cosmic-text 0.19.0 re-export, AccessKit
0.24.1 with accesskit_winit 0.33.2, and pollster 1.0.1. `ven-upt.2` completed the
reference and crate gates before implementing that shape.

## Required for native history, selection, and copy

`ven-4sn` consumed Orbit `ORB-C8` at proof
`840a67c0cb32b334ed54888321d5ca77e58117b0` and `ORB-C9` plus canonical ORBS v2
at proof `9d6d2bb37f20ab4ad9e186c7bc715eabef43e757`. The exact package defines
revision-bound Begin, Update, Finish, and Copy actions and the bounded
server-only `CopiedText` effect. Venus adopted those messages directly and did
not add a protocol mirror, history cache, terminal selection, or compatibility
adapter.

Exact winit 0.30.13 line and pixel wheel events support cell-normalized bounded
native accumulation. Exact AccessKit 0.24.1 text runs and text positions support
selection derived from the accepted scene. Exact arboard 3.6.1 supplies the
isolated native text write selected in `docs/CRATES.md`.

## Required for exact ORBS v4 consumption

`ven-adopt-orbs-v4-without-interaction-expansion-zdc` consumes canonical ORBS
v4 and proved `ORB-C1` through `ORB-C12` from exact Orbit source
`7f067b30e97d0b4787a7c6c0bbe3dd8a80a61c2c`; accepted proof metadata is at
`4a8da857bafd980c199e6c49d2718ad9cae26dc0`. The exact diff from the prior
`3ee7c80005f3d2bbe81e539799327803716f6174` consumer replaces version-range
Hello/Attached/Incompatible negotiation with exact-header Hello and Attached,
and adds read-only vertical previews plus typed committed-wheel outcomes.

Venus sends no preview request and rejects an unsolicited preview at its model
ordering boundary. Terminal-routed wheel outcomes reuse the accepted-result
owner; viewport outcomes enter the existing complete-frame reducer and bounded
frame queue. An unsupported header version retains explicit incompatibility UX
without decoding another revision. No input, renderer, gesture, retry, schema,
adapter, or compatibility path is added.

## Required for native paste

`ven-native-clipboard-paste-uas` consumes Orbit `ORB-C5` at accepted proof
`9d6d2bb37f20ab4ad9e186c7bc715eabef43e757`. Orbit accepts one canonical paste
up to 1 MiB and applies normal or bracketed terminal encoding. Venus passes that
message unchanged and keeps no terminal paste policy.

Exact winit 0.30.13 supplies logical Ctrl+Shift+V and `NamedKey::Paste`. Exact
arboard 3.6.1 reads UTF-8 from the ordinary clipboard and reports empty or
non-text content. Mars `21109e3ebc24b63da11bae644dfb9bab28ce0e18` confirms
the same Linux shortcuts. Venus reuses no Mars binding, clipboard, or terminal
code.

`ven-consume-terminal-clipboard-writes-zgh` first consumed Orbit `ORB-C11` and
canonical ORBS v3 at proof `3ee7c80005f3d2bbe81e539799327803716f6174`.
Orbit supplies bounded UTF-8 and the normalized standard, selection, or primary
destination. Venus accepts the effect only while attached and passes it to the
existing native clipboard owner without a local decoder or replay state. The
ORBS v4 gate above preserves that unchanged effect at `7f067b30`.

Exact arboard 3.6.1 exposes the ordinary and primary Linux clipboards through
its selected `wayland-data-control` build. The pinned Ghostty comparable maps
standard to the ordinary clipboard and maps both selection and primary to the
platform primary clipboard. Venus uses that map and rejects arboard's secondary
clipboard for this contract. macOS has no distinct arboard primary target and
remains outside the proved platform surface.

Alacritty 0.15.1 at
`0c405d53e74ace2980fc5e6c6d5b710c144bc075` was comparison-only evidence for
wheel residuals, Shift selection precedence, Ctrl+Shift+C, clipboard isolation,
and backend-specific IME cursor-area comparison. Venus reused no source
and rejected Alacritty's terminal, grid, selection, configuration, auto-copy,
primary-selection, search, and raw-display clipboard ownership.

## Required for native Eon workspace materialization

`ven-c87` consumes `EON-C10` and dependency-free EONW v1 from exact Eon proof
`4af395aea06c230ee6b18cf0755ae25915c0b88d`; proof documentation is recorded at
`93c3c723e5da95c92882571b2904d4c9a4b9f2ff`. The owner package supplies complete
ordered snapshots, stable tab, pane, and Session identities, exact opaque Orbit
endpoints, liveness, structured failures, and pointer or four-direction semantic
actions. Venus consumes those values directly. The `ven-c87.1` refresh hardening
reuses bounded `Inspect` every 250 ms without adding a protocol message, CLI
adapter, hidden-state reader, or second workspace schema.

Canario/Rio at `3e41b8b19a1cad9cd9bdfc8f7900cf61ce5a9098`
was MIT-licensed comparison-only evidence for bounded header scrolling, direct
labeled-row activation, distinct selected state, and one selected content
surface. Venus reused no source and rejected its terminal ownership, split and
reorder behavior, previews, sidebar, persistence, command palette, and effects.
Nova at `57b0c8621894c59d058d7ba7d91464864b7c4917` is Apache-2.0
comparison-only evidence for shared non-modal `Alt+m` pane creation and
`Alt+h/l` focus-or-tab traversal. The user selected the complete Eon keymap;
Venus reuses no Nova source, Zellij configuration, plugin, or pane ownership.
Exact winit, wgpu, glyphon, and AccessKit versions already selected above own the
native event, clipping, drawing, tab semantics, expanded state, and accessibility
action mechanisms used by the workspace projection.

## Required for supervisor-owned native presentation

`eon-cyt` consumes Eon's existing `EON-C11` Present action and exact child-process
ownership without changing EONW. Exact winit 0.30.13 owns
`Window::set_minimized`, `Window::focus_window`, and
`Window::request_user_attention`. Locked winit documents direct unminimize and
focus as unsupported on Wayland; its attention path requests and applies
`xdg_activation_v1` for the existing surface under compositor policy. Venus
accepts only the bounded private supervisor signal and remains the sole native
activation owner.

The selected shape rejects a duplicate window, Venus replacement, platform token
in EONW, compositor-specific commands, D-Bus application infrastructure, polling
files, and a signal-handler dependency. The compositor retains final activation
policy, so native acceptance is platform-specific.

## Required for native cursor-tail materialization

`ven-rio-cursor-animation-u57` consumes unchanged canonical ORBS v2 cursor
values at Orbit proof `9d6d2bb37f20ab4ad9e186c7bc715eabef43e757`
through the current exact `orbit-protocol` source
`7f067b30e97d0b4787a7c6c0bbe3dd8a80a61c2c`. Orbit remains authoritative for
destination, shape, visibility, blink state, wide-tail state, and cursor color.

[Rio](https://github.com/raphamorim/rio/tree/e019a9325b59a025cffa03a21d0788168514d502)
at `e019a9325b59a025cffa03a21d0788168514d502` is MIT-licensed required evidence
for its complete trail cursor, spring helper, renderer glue, route reset, and
redraw scheduling. Venus adopts the independently useful four-corner critically
damped spring, 40 ms short-horizontal and 150 ms longer timing classes, 100 ms
delta cap, first-frame and route snap, direction-ranked lag, one two-triangle
quad, and redraw only while unsettled. It reuses no Rio source and rejects Rio's
terminal state, Sugarloaf, renderer framework, configuration, panels, and
polling.

[Yazelix Cursors](https://github.com/Yazelix/cursors/tree/f97d0e7d3badf37ce3c01c1eba99b6b2bd17a7bf)
at `f97d0e7d3badf37ce3c01c1eba99b6b2bd17a7bf` is Apache-2.0 required boundary
evidence. Its `eon-venus-native-v1` target resolves exactly `none` or `tail`
with the first resolved palette color and a duration multiplier. Venus consumes
only strict Eon-serialized scalar launch values; it neither depends on Yazelix
Cursors nor reads its registry. Eon launcher
`013ffb34acfd01b90dc4999f10526a4f2d8ec068` confirms that bounded options precede
one or two positional sockets. Eon serialization remains separately owned and
does not expand this Venus implementation.

The existing wgpu rectangle pipeline and dynamic buffer upload are sufficient.
The conditional wgpu post-processing comparison is rejected: no shader ABI,
new dependency, renderer replacement, general effects engine, or persistent
Venus setting is introduced.

## Required for compositor-owned background blur

`ven-wayland-background-blur-e06` keeps exact winit 0.30.13 and patches only its
source to `chiyuki0325/winit-0.30` commit
`fb45fbf901fbe70cc9a877b5d651d0b60c206b08`. That commit is directly above the
v0.30.13 tag and is an exact backport of verified merged upstream commit
`c4afadbfabf7b1e7989b40b493db1a4c7bd8ff4e`. It prefers
`ext-background-effect-v1`, retains the KDE fallback, and leaves the public
`WindowAttributes::with_blur` contract unchanged. The fork and upstream remain
Apache-2.0.

Wayland Protocols 1.46 at peeled commit
`6141e1154303dadd5c3e480bc4a16e26f1dcb2af` is the first corrected protocol
reference and is byte-identical to the XML bundled by locked
`wayland-protocols` 0.32.13. Release 1.45 is rejected because it encodes the
blur capability as zero. The protocol makes the region surface-local and
double-buffered while leaving the algorithm and policy to the compositor.

Installed COSMIC compositor 1.0.0 at
`091583ac84abac02967ae358cf9570ddfef63b31` advertises the corrected blur
capability, commits the requested region with surface state, and owns the
rendering algorithm. Its source is GPL-3.0 evidence only; Venus copies and
incorporates none of it. Venus adds one launch boolean at its existing window
creation boundary. It rejects direct Wayland ownership, capability caching,
blur strength, renderer effects, live updates, Eon persistence, and another
window or event-loop owner.

## Rejected initial routing

- Rio VT is not a Venus dependency; replacing Orbit's terminal engine is an
  Orbit decision after a concrete engine failure.
- librio matters only for a separately authorized non-Rust consumer.
- Ratatui may support a lossy diagnostic client but cannot prove the native
  Venus or rich-presentation contracts.
- Qwertty targets controlling-terminal applications and does not provide the
  native client foundation.
