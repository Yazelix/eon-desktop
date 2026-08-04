# Venus reference routing

References answer a named question; they do not select dependencies or authorize
source reuse. Before code, the implementing Bead records exact releases or
commits, useful mechanisms, rejected surrounding scope, license compatibility,
and consequences for ownership, code shape, and the first check.

## Required before the first client implementation

- Orbit `ORB-C4` and `ORB-C6` plus the canonical `orbit-protocol` presentation
  codec are accepted at `6e53fedb97f764f3683c83edcd9a5227b8f56e56`;
  semantic-input behavior `ORB-C5` remains proved at
  `e4fde443e625180d7332eff4dff366f64bee30a8`. Venus must consume canonical wire
  owners at exact revisions. Attach, input, and resize still lack an accepted
  reusable consumer codec and remain a pre-implementation Orbit boundary gap.
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

The Jeffrey Emanuel references carry nonstandard licensing. The user explicitly
directed exact-commit inspection for architecture ideas and independently
directs and benefits from it; tool choice alone does not satisfy a restriction
on acting for a named provider. Inspection does not authorize copying,
adaptation, redistribution, incorporation, or dependency selection, which
remain separate license and user-approval gates.

## Crate-gate comparisons

- [winit](https://github.com/rust-windowing/winit) is a window and native-event
  candidate.
- [wgpu](https://github.com/gfx-rs/wgpu) is a GPU abstraction candidate.
- [glyphon](https://github.com/grovesNL/glyphon) and
  [cosmic-text](https://github.com/pop-os/cosmic-text) are text and shaping
  candidates.
- [Sugarloaf](https://github.com/raphamorim/rio/tree/main/sugarloaf) is
  conditional on a demonstrated need for its renderer shape.

Compare complete ownership shapes, not isolated crates. The gate measures text
correctness, input methods, accessibility, Linux behavior, macOS feasibility,
future browser implications, owned LOC, dependency/build cost, and maintenance.

## Rejected initial routing

- Rio VT is not a Venus dependency; replacing Orbit's terminal engine is an
  Orbit decision after a concrete engine failure.
- librio matters only for a separately authorized non-Rust consumer.
- Ratatui may support a lossy diagnostic client but cannot prove the native
  Venus or rich-presentation contracts.
- Qwertty targets controlling-terminal applications and does not provide the
  native client foundation.
