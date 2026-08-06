# Venus

Venus is the greenfield native graphical client for Orbit and Yazelix Astra.
It renders structured presentation state authored by Orbit and sends semantic
interaction back to the authoritative session runtime.

## Status

Venus implements one Linux window attached to one already-running local Orbit
session. It renders the accepted Orbit frame contract, sends semantic native
input and resize events, detaches without ending the Orbit session, and reports
bounded attachment or server failures. It does not own a PTY or terminal
emulator.

## Ownership

```text
Yazelix Astra  -> product policy, composition, distribution
Venus          -> native presentation, interaction, client failure UX
Orbit          -> PTYs, terminal state, session lifetime, wire authority
```

Venus consumes `orbit-protocol` 0.1.0, ORBF v1, and ORBS v1 at exact Orbit proof
`c905bf9610581747f1b07565814b501ca66cfaa6`. One reducer turns complete canonical
frames into immutable scene data used by drawing and accessibility. The native
host owns the local socket, window, input mapping, and redraw lifecycle; it owns
no terminal state.

## Run

Start an Orbit session server first, then pass its Unix socket to Venus:

```sh
cargo run --locked -- /path/to/orbit.sock
```

Without an argument, Venus uses
`$XDG_RUNTIME_DIR/yazelix-orbit/orbit.sock`, or
`/tmp/yazelix-orbit-$UID/orbit.sock` when the runtime directory is unavailable.
Only one presentation client can attach to an Orbit session at a time.

## Architecture and evidence

- [`docs/CONTRACTS.md`](docs/CONTRACTS.md) indexes Venus behavior and proof.
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

The Linux host uses winit, wgpu, glyphon, and AccessKit. macOS remains an
architectural target, not an implemented or proved platform. The exact
`c905bf9610581747f1b07565814b501ca66cfaa6` Orbit package revision is published
and resolves from GitHub. Later Orbit runtime proof
`847cab1ca37495c5cd45454623bd81909b488564` preserves identical protocol and
Cargo metadata, so Venus keeps its accepted dependency pin.

## LOC scorecard

The scorecard counts tracked handwritten text and code. It excludes `.git/`,
Beads data, lock files, and generated artifacts.

| Surface | Lines |
|---|---:|
| Agent policy | 191 |
| README | 85 |
| Contracts and references | 123 |
| Crate decisions | 88 |
| Changelog | 21 |
| Rust production | 3,486 |
| Rust tests | 307 |
| Cargo manifest | 18 |
| **Total** | **4,319** |
