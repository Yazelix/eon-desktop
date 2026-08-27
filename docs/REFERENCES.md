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
- [foot](https://codeberg.org/dnkl/foot/src/commit/85655c74a4ded119392ea8b632626c3920042807)
  at `85655c74a4ded119392ea8b632626c3920042807` is MIT-licensed comparison
  evidence for a Wayland-only terminal host and its headless Sway/Cage test
  boundary. Venus rejects foot's PTY, terminal, configuration, renderer, and
  test-harness ownership; the reference does not select another compositor or
  terminal dependency.
- Eon's
  [central terminal-product comparison route](https://github.com/Yazelix/eon/blob/edge/docs/REFERENCES.md#terminal-product-comparisons)
  owns Monstar's exact release identity, license, useful mechanisms, and product
  rejections. A Venus Bead follows that route only for a named native-host,
  terminal-interaction, or performance question, then records the exact source
  it inspects. Venus retains Orbit's PTY and terminal authority; this repository
  keeps no duplicate Monstar pin.
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
correctness, input methods, accessibility, native Linux Wayland behavior,
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
selection derived from the accepted scene. Exact wl-clipboard-rs 0.9.3 supplies
the isolated native Wayland text effect selected in `docs/CRATES.md`.

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

## Required for read-only pane metadata

`ven-render-live-pane-title-cwd-8c7` consumes canonical ORBS v5 from exact
[Eon Sessions](https://github.com/Yazelix/eon-sessions/tree/69c402737799f03e615473956954a043647a4713)
source `69c402737799f03e615473956954a043647a4713`. Orbit owns bounded title and
working-directory values plus the read-only `ObserveMetadata` role; Venus sends
that one request, rejects every other role or message order, and retains only
the latest accepted update for each visible live endpoint.
Within Eon's local-only product boundary, the projection treats a `file://`
authority as transport decoration and displays only the absolute path. Remote
working-directory semantics and percent decoding remain outside the contract.

Exact EONW v4 protocol source `963bdaf4d9816f27a26c7bdb6ee122855566a222`
is authoritative for picker lifecycle and tab binding, active-tab identity,
pending-tab state, tab launch directory, pane liveness, endpoint identity, and
optional selection. Venus derives picker visibility only when its bound tab is
active, starts no hidden or offline observer, retires obsolete endpoints
directly from each accepted snapshot, and defers the selected endpoint's
observer until its presentation attachment has completed.
That deferral preserves Orbit's single pending-negotiation slot without adding
retry policy or another lifecycle owner.

[Zellij](https://github.com/zellij-org/zellij/tree/b9637022eaddb22855dc9914a0cc06762a124b8c)
at `b9637022eaddb22855dc9914a0cc06762a124b8c` is MIT-licensed comparison-only
evidence for showing a pane title within the available header width. Venus
adopts that narrow outcome through its existing header clipping and rejects
Zellij's terminal, tab, layout, plugin, configuration, and title-policy owners.
No dependency or copied source is added.

## Required for native paste

`ven-native-clipboard-paste-uas` consumes Orbit `ORB-C5` at accepted proof
`9d6d2bb37f20ab4ad9e186c7bc715eabef43e757`. Orbit accepts one canonical paste
up to 1 MiB and applies normal or bracketed terminal encoding. Venus passes that
message unchanged and keeps no terminal paste policy.

Exact winit 0.30.13 supplies logical Ctrl+Shift+V and `NamedKey::Paste`. Exact
wl-clipboard-rs 0.9.3 returns ordinary Wayland clipboard bytes for a selected
text MIME. Venus bounds and UTF-8-validates them; empty or non-text content stays
visible. Mars `21109e3ebc24b63da11bae644dfb9bab28ce0e18` confirms the same Linux
shortcuts. Venus reuses no Mars binding, clipboard, or terminal code.

`ven-consume-terminal-clipboard-writes-zgh` first consumed Orbit `ORB-C11` and
canonical ORBS v3 at proof `3ee7c80005f3d2bbe81e539799327803716f6174`.
Orbit supplies bounded UTF-8 and the normalized standard, selection, or primary
destination. Venus accepts the effect only while attached and passes it to the
existing native clipboard owner without a local decoder or replay state. The
ORBS v4 gate above preserves that unchanged effect at `7f067b30`.

Exact wl-clipboard-rs 0.9.3 exposes the ordinary and primary Linux clipboards
through Wayland data-control. The pinned Ghostty comparable maps standard to the
ordinary clipboard and maps both selection and primary to the platform primary
clipboard. Venus uses that map and has no secondary or compatibility fallback.

Alacritty 0.15.1 at
`0c405d53e74ace2980fc5e6c6d5b710c144bc075` was comparison-only evidence for
wheel residuals, Shift selection precedence, Ctrl+Shift+C, clipboard isolation,
and backend-specific IME cursor-area comparison. Venus reused no source
and rejected Alacritty's terminal, grid, selection, configuration, auto-copy,
primary-selection, search, and raw-display clipboard ownership.

## Required for native Eon workspace materialization

`ven-c87`, `ven-present-tab-directory-picker-a6v`, and
`ven-consume-picker-first-eonw-v4-zd1` consume `EON-C10`, `EON-C17`, `EON-C18`,
and dependency-free EONW v4 from exact Eon source
`963bdaf4d9816f27a26c7bdb6ee122855566a222`; prior workspace proof
documentation is recorded at `93c3c723e5da95c92882571b2904d4c9a4b9f2ff`.
The owner package supplies complete
ordered snapshots, stable tab, pane, and Session identities, authoritative raw
tab launch directories, exact opaque Orbit endpoints, liveness, structured
failures, and semantic actions. Venus consumes those values directly, derives
bounded tab labels and accessibility context, and retains `tN` for action routing.
The `ven-c87.1` refresh hardening
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

The v4 picker state contributes only one existing-tab identity and one validated
endpoint; its bound pending tab may have no panes or selected pane. Venus
derives visibility from equality with the active tab and reuses its single
terminal attachment, scene, renderer, input, and AccessKit owners. One explicit
`--workspace` option selects the existing EONW startup path without an initial
Orbit endpoint; one physical Alt+Z maps to the semantic action, and existing
Alt+H/L actions remain Eon-owned traversal. Nova
`f1beb34fe6060cfa2c0201d7f8095f6ef707f467`, Zellij
`910339e219b17f4e2ffd4a2cb6e35cccc7246493`, and Kitty
`54416498c89e1d07e5079c49d15470dd0d947ce7` remain comparison-only for the
accepted shortcut, inset, and terminal-backed lifecycle. Venus reuses no source
and rejects a native picker, inferred modal state, simultaneous terminals, and
a generic popup or modal API.

## Required for pixel and kinetic retained-history scrolling

`ven-venus-kinetic-touchpad-scroll-5sl` consumes Orbit `ORB-C8` and `ORB-C9`
through exact ORBS v7 source
`baf8aa28dcaa50484cd221aa7730defedc2356bb`. Orbit supplies one bounded
revision-bound nearest-first row window and one signed atomic viewport commit;
Venus does not infer routing, edges, cells, or revisions.

Patched winit 0.30.13 at
`fb45fbf901fbe70cc9a877b5d651d0b60c206b08` maps Wayland discrete axes to
`LineDelta`, continuous axes to physical `PixelDelta`, and available axis-stop
to `Ended`. It discards axis source and hardware event time and documents that
discrete wheel sequences may lack `Ended`. Venus therefore gives momentum only
to phase-complete pixel sequences and adds no device or source heuristic.
`Window::request_redraw` with `pre_present_notify` owns compositor callback
pacing. Wgpu 30.0.0 retains guaranteed FIFO and its default desired frame
latency of two until installed measurements justify a change. Glyphon 0.12 and
the existing rectangle batch accept fractional positions and clipping.

GTK main `04275027fc556b1b0f3935e2f502fc356e88227b` is comparison-only
evidence for a 150 ms recent-sample velocity estimate, frame-clock advancement,
and elapsed-time exponential friction of 4 s^-1. Kitty
`7f71461a9de44f71f631db29d50bcb3e866988b8` is comparison-only evidence for
sub-row terminal presentation and precision momentum. Venus reuses no source
and rejects overshoot, bounce, velocity stacking, fixed-rate timers,
frame-dependent decay, platform source code, and public tuning.

## Required for native selection gestures

`ven-venus-native-selection-gestures-d3x` consumes Orbit `ORB-C5` and `ORB-C9`
through exact accepted ORBS v10 source
`59975e9176f5caf8b78dc3273e88d9ecbb75dc3f`. Orbit chooses terminal mouse
input or host selection from authoritative terminal state, pins that route for
the sequence, applies cell, word, and logical-line gestures, freezes copy text,
tags release copy separately from explicit ordinary copy, and reports the
authoritative completion revision. Venus supplies native surface position,
modifiers, event-delivery time, ordered buffering, and clipboard effects.

The accepted reference gate in Bead `ven-venus-native-selection-gestures-d3x`
compares WezTerm, Kitty, Alacritty, Zellij, and Ghostty and adopts their common
uncaptured selection plus Shift bypass behavior. Venus adds no click recognizer,
terminal-state inference, dependency, or unreleased winit API.

## Required for continuous-output scrollback acceptance

`ven-accept-continuous-output-scrollback-a1s` consumes Orbit `ORB-C8` through
exact accepted source `a65e199e16e97330175e314cacf791fa00f53069` under unchanged
ORBS v10. Orbit resolves lagging relative preview and signed-scroll requests
against current terminal authority while future revisions and unrelated stale
input remain strict. Venus retains one pending preview, one in-flight batch,
fractional and kinetic input, canonical frame reduction, and truthful failures.
While screen, geometry, default cell colors, and palette remain compatible, it
keeps the last bounded revision-bound row window as presentation input, submits
crossed rows against that exact lagging revision, accepts only the matching
in-flight outcome, and never carries terminal-routing evidence across a plain
frame. It adds no retry, failure-string policy, history cache, or revision
owner.

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

## Required for caller-owned application identity

Exact patched winit 0.30.13 commit
`fb45fbf901fbe70cc9a877b5d651d0b60c206b08` documents and implements
`WindowAttributesExtWayland::with_name`: its general name becomes the Wayland
application ID and should match the distributed desktop-file ID; its instance
name is unused on Wayland. VEN-C17 reuses only this existing native window
attribute seam before creation. It rejects desktop-file loading, title-based
grouping, product-specific branches, mutable identity, and another window or
event-loop owner.

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

## Watchlist

- [MetalTerm](https://metalterm.dev/) is source-unavailable comparison evidence
  to revisit only for a named Venus question about idle redraw, GPU measurement,
  OSC 133 command blocks, or grapheme and cell storage. Its Metal renderer and
  native macOS packaging are outside Venus's platform scope. Its
  [site repository and issues](https://github.com/pioner92/metalterm-site)
  and the creator's [X feed](https://x.com/pioner_dev) are discovery surfaces,
  not implementation authority. No application source or license was public
  when checked on 2026-08-21; pin a stable artifact and reproduce performance
  claims before using them at a reference gate.

## Rejected initial routing

- Rio VT is not a Venus dependency; replacing Orbit's terminal engine is an
  Orbit decision after a concrete engine failure.
- librio matters only for a separately authorized non-Rust consumer.
- Ratatui may support a lossy diagnostic client but cannot prove the native
  Venus or rich-presentation contracts.
- Qwertty targets controlling-terminal applications and does not provide the
  native client foundation.
