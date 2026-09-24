# Eon Desktop

Eon Desktop contains Venus, the native client that renders Orbit Sessions and
sends user input back to their authoritative runtime. In an Eon workspace,
Venus draws the tabs, pane stack, and popups from Eon's workspace state.

## Demo

[![Animated Venus demo rendering Eon's two-pane native Wayland workspace](https://raw.githubusercontent.com/Yazelix/eon/ffa06c6a414921a1a0570c3ea3c1e49b1a3210da/assets/demo/eon-demo.gif)](https://github.com/Yazelix/eon/blob/ffa06c6a414921a1a0570c3ea3c1e49b1a3210da/assets/demo/eon-demo.mp4)

[Watch the MP4](https://github.com/Yazelix/eon/blob/ffa06c6a414921a1a0570c3ea3c1e49b1a3210da/assets/demo/eon-demo.mp4) of the composed Linux alpha.

## Run locally

The proved host is **x86_64 Linux on native Wayland with Vulkan**. With an Orbit
server running, point Venus at its Unix socket:

```sh
cargo run --locked -- /path/to/orbit.sock
```

Venus can also start before Orbit and reconnect when the socket appears. To
view an Eon workspace, pass its workspace socket instead:

```sh
cargo run --locked -- --workspace /path/to/eon.sock
```

[Running Venus](docs/LAUNCH.md) covers fonts, window options, attachment, and
supervised startup. Apple Silicon macOS has partial native proof and remains
unsupported as a product target.

## Read more

- [Interaction](docs/INTERACTION.md): links, workspace controls, scrolling, selection, and clipboard.
- [Presentation and ownership](docs/PRESENTATION.md): visual behavior, component boundaries, and limits.
- [Development checks](docs/DEVELOPMENT.md): local and native proof commands.
- [Contract index](docs/CONTRACTS.md): accepted behavior, exact revisions, and remaining gaps.
- [References](docs/REFERENCES.md), [crate decisions](docs/CRATES.md), and the [memory comparison](docs/benchmarks/venus-memory-2026-09-08.md): design evidence.

## License

The copyright holder offers the project-owned source history under
[Apache-2.0](LICENSE). The OpenAI mark has separate copyright attribution and
trademark terms in [third-party notices](THIRD_PARTY_NOTICES.md).

## LOC scorecard

The scorecard counts tracked handwritten text and code. It excludes `.git/`,
Beads data, lock files, and generated artifacts, including rendered `AGENTS.md`,
benchmark CSV data, and disposable qualification patches.

| Surface | Lines |
|---|---:|
| Agent policy inputs | 216 |
| README | 68 |
| Repository attributes and ignore rules | 7 |
| License | 201 |
| Third-party notices | 37 |
| Contracts and references | 2,285 |
| Guides | 408 |
| Memory benchmark report | 158 |
| Crate decisions | 302 |
| Changelog | 298 |
| Rust source, including unit tests | 20,553 |
| Rust integration tests | 1,045 |
| Cargo manifest | 33 |
| **Total** | **25,611** |
