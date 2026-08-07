# Eon Desktop

Eon Desktop is the repository for Eon for desktop. Its Venus subsystem renders
structured presentation state authored by the Orbit subsystem in Eon Sessions
and sends semantic interaction back to the authoritative session runtime.

## Status

The Venus client implements one Linux window attached to one already-running
local Orbit session. It renders the accepted Orbit frame contract, sends
semantic native input and resize events, detaches without ending the Orbit
session, scrolls Orbit-owned retained history, presents Orbit-owned selection,
writes explicit copied text to the native clipboard, and reports bounded
attachment or server failures. It does not own a PTY or terminal emulator.

## Ownership

```text
Eon                     -> product policy, composition, distribution
Eon Desktop / Venus     -> native presentation, interaction, client failure UX
Eon Sessions / Orbit    -> PTYs, terminal state, session lifetime, wire authority
```

Venus consumes `orbit-protocol` 0.1.0, ORBF v1, and ORBS v2 at exact Orbit proof
`9d6d2bb37f20ab4ad9e186c7bc715eabef43e757`. One reducer turns complete canonical
frames into immutable scene data used by drawing and accessibility. The native
host owns the local socket, window, input mapping, and redraw lifecycle; it owns
no terminal state.

## Run

Start the Eon Sessions Orbit server first, then pass its Unix socket to the
Venus client:

```sh
cargo run --locked -- /path/to/orbit.sock
```

Without an argument, Venus uses
`$XDG_RUNTIME_DIR/yazelix-orbit/orbit.sock`, or
`/tmp/yazelix-orbit-$UID/orbit.sock` when the runtime directory is unavailable.
Only one presentation client can attach to an Orbit session at a time.

Wheel or trackpad movement scrolls through Orbit-owned retained history. Hold
Shift while dragging the left mouse button to select cells, then press
Ctrl+Shift+C to copy the exact bounded text returned by Orbit.

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

## Initial exclusions

Tabs, panes, sidebars, popups, settings, visual effects, configuration, plugins,
remote and web access, macOS implementation, packaging, and distribution are
outside the first slice.

The Linux host uses winit, wgpu, glyphon, AccessKit, and an isolated arboard
text-clipboard effect. macOS remains an architectural target, not an
implemented or proved platform. The exact
`9d6d2bb37f20ab4ad9e186c7bc715eabef43e757` Orbit package revision is published
and resolves from GitHub; later Orbit hardening at
`6452a2f4f4233a7a89a17e479eda36b010516dcf` preserves that product proof.

## LOC scorecard

The scorecard counts tracked handwritten text and code. It excludes `.git/`,
Beads data, lock files, and generated artifacts.

| Surface | Lines |
|---|---:|
| Agent policy | 188 |
| README | 91 |
| Contracts and references | 145 |
| Crate decisions | 109 |
| Changelog | 27 |
| Rust source, including unit tests | 4,350 |
| Rust integration tests | 363 |
| Cargo manifest | 19 |
| **Total** | **5,292** |
