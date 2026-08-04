# Venus

Venus is the greenfield native graphical client for Orbit and Yazelix Astra.
It renders structured presentation state authored by Orbit and sends semantic
interaction back to the authoritative session runtime.

## Status

This repository contains plans and contracts only. Orbit's governance handoff
is closed, and the user activated `ven-upt.1` for architecture and research.
Product implementation remains gated on accepting that decision and explicitly
activating `ven-upt.2`.

The first target is deliberately narrow: one Linux window attached to one
already-running local Orbit session. The client renders the accepted Orbit
frame contract, sends semantic input and resize events, survives Orbit session
detachment, and reports bounded attachment or server failures. It does not own
a PTY or terminal emulator.

## Ownership

```text
Yazelix Astra  -> product policy, composition, distribution
Venus          -> native presentation, interaction, client failure UX
Orbit          -> PTYs, terminal state, session lifetime, wire authority
```

Venus consumes the accepted Orbit `ORB-C4`, `ORB-C5`, and `ORB-C6` boundary and
preserves Orbit's session, authority, and one-client contracts. Contract gaps
return to Orbit instead of becoming Venus compatibility code.

## Planning sources

- [`docs/CONTRACTS.md`](docs/CONTRACTS.md) indexes Venus behavior and proof.
- [`docs/REFERENCES.md`](docs/REFERENCES.md) routes rendering and composition
  research before code shape is chosen.
- [`docs/CRATES.md`](docs/CRATES.md) records dependency decisions; no graphical
  stack is selected yet.
- Beads contain the gated implementation plan.

```sh
bv --robot-triage
br ready
```

## Initial exclusions

Tabs, panes, sidebars, popups, settings, visual effects, configuration, plugins,
remote and web access, macOS implementation, packaging, and distribution are
outside the first slice.

## LOC scorecard

The scorecard counts tracked handwritten text and code. It excludes `.git/`,
Beads data, lock files, and generated artifacts.

| Surface | Lines |
|---|---:|
| Agent policy | 207 |
| README | 64 |
| Contracts and references | 82 |
| Crate decisions | 11 |
| Changelog | 5 |
| **Total** | **369** |
