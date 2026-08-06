# Venus contract index

Statuses are `Planned`, `Partially proved`, and `Proved`. A proved contract
names an accepted immutable proof commit and its canonical checks.

| ID | Behavior and owner | Status | Consumed boundary | Canonical proof | Gap |
|---|---|---|---|---|---|
| `VEN-C1` | Venus deterministically validates and materializes one coherent Orbit-authored structured presentation frame and ordered revisions into native draw inputs without terminal authority. | Proved | Orbit `ORB-C4` and `ORB-C6` at accepted proof `c905bf9610581747f1b07565814b501ca66cfaa6` | Venus `8929c9f9d151641a343813ddeb6005cb9c771286`; canonical corpus, locked Rust checks, bounded revision-safe complete-frame replacement, Unicode-preserving shaping checks, and exact Orbit `847cab1ca37495c5cd45454623bd81909b488564` Xwayland acceptance at 929x976 and 100x52: sustained combined CPU p95 167 percent, peak RSS about 55 MiB, and final convergence 13.6 ms. | No initial-slice gap. Wayland and macOS remain unproved platform surfaces. |
| `VEN-C2` | Venus owns native interaction collection and sends only Orbit's semantic input and resize messages. | Proved | Orbit `ORB-C5` and canonical ORBS v1 at accepted proof `c905bf9610581747f1b07565814b501ca66cfaa6` | Venus `8929c9f9d151641a343813ddeb6005cb9c771286`; semantic mapping and presentation gating checks, ordered acknowledgements across bounded frame replacement, shell/Neovim/Yazi input dogfood, and 50 targeted fullscreen cycles returning to 929x976 and coherent 100x52 state within 500 ms. | No initial-slice gap. Candidate-list IMEs and Wayland remain manual quality surfaces. |
| `VEN-C3` | Venus is transient: closing or crashing the client does not own or terminate the Orbit session, and reopening materializes its coherent current state. | Proved | Orbit `ORB-C1` through `ORB-C4` at accepted proof `c905bf9610581747f1b07565814b501ca66cfaa6`; `ORB-C7` has partial evidence at that revision | Venus `8929c9f9d151641a343813ddeb6005cb9c771286`; transport lifecycle, reattachment, terminal-state ordering, and native attached-session loss checks plus coherent Xwayland reattachment in 122 ms while Orbit and its PTY survived client loss. | No Venus gap; Orbit retains its partial `ORB-C7` hardening gap. |
| `VEN-C4` | Venus gives bounded, explicit client UX for attach rejection, protocol incompatibility, invalid frames, and Orbit loss rather than hanging or silently inventing state. | Proved | Orbit `ORB-C3`, `ORB-C4`, canonical ORBS v1, and partial `ORB-C7` evidence at accepted proof `c905bf9610581747f1b07565814b501ca66cfaa6` | Venus `8929c9f9d151641a343813ddeb6005cb9c771286`; bounded model, attribution, accessibility, framing, queue-capacity, revision ordering, event-ordering, terminal-state, and first-cause checks plus Xwayland resize, loss, and reattachment dogfood without stale state or an unexpected notice. | No Venus gap; Orbit retains its partial `ORB-C7` hardening gap. |

The Orbit proof revisions above establish source boundaries; they do not prove
a Venus contract. Every consumer proof records the exact Orbit revisions tested
and reports any gap to Orbit.

Orbit proof `c905bf9610581747f1b07565814b501ca66cfaa6` is published on
`origin/edge` and remains Venus's exact `orbit-protocol` dependency. Later
Orbit runtime proofs `838b67652c4df1979e599b9c401ee664ffac66bd` and
`847cab1ca37495c5cd45454623bd81909b488564` are compatible: `crates/protocol`,
`Cargo.toml`, and `Cargo.lock` are byte-identical from the pinned dependency
through the exercised runtime revision, so no Venus manifest migration is
required. Orbit retains its partial `ORB-C7` hardening gap.
