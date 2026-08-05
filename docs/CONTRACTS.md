# Venus contract index

Statuses are `Planned`, `Partially proved`, and `Proved`. A proved contract
names an accepted immutable proof commit and its canonical checks.

| ID | Behavior and owner | Status | Consumed boundary | Canonical proof | Gap |
|---|---|---|---|---|---|
| `VEN-C1` | Venus deterministically validates and materializes one coherent Orbit-authored structured presentation frame and ordered revisions into native draw inputs without terminal authority. | Proved | Orbit `ORB-C4` and `ORB-C6` at accepted proof `c905bf9610581747f1b07565814b501ca66cfaa6` | Venus `0669767e4652682b95c60896c177178232a63755`; canonical corpus, locked Rust checks, and Linux/X11 rich-scene, compositor-resize, color, and surface smoke. | No initial-slice gap. Wayland and macOS remain unproved platform surfaces. |
| `VEN-C2` | Venus owns native interaction collection and sends only Orbit's semantic input and resize messages. | Proved | Orbit `ORB-C5` and canonical ORBS v1 at accepted proof `c905bf9610581747f1b07565814b501ca66cfaa6` | Venus `0669767e4652682b95c60896c177178232a63755`; semantic mapping checks plus native typing, Enter, focus, and compositor-resize dogfood. | No initial-slice gap. Candidate-list IMEs and Wayland remain manual quality surfaces. |
| `VEN-C3` | Venus is transient: closing or crashing the client does not own or terminate the Orbit session, and reopening materializes its coherent current state. | Proved | Orbit `ORB-C1` through `ORB-C4` at accepted proof `c905bf9610581747f1b07565814b501ca66cfaa6`; `ORB-C7` has partial evidence at that revision | Venus `0669767e4652682b95c60896c177178232a63755`; transport lifecycle checks and native process-stop/reopen dogfood against one surviving Orbit session. | No Venus gap; Orbit retains its partial `ORB-C7` hardening gap. |
| `VEN-C4` | Venus gives bounded, explicit client UX for attach rejection, protocol incompatibility, invalid frames, and Orbit loss rather than hanging or silently inventing state. | Proved | Orbit `ORB-C3`, `ORB-C4`, canonical ORBS v1, and partial `ORB-C7` evidence at accepted proof `c905bf9610581747f1b07565814b501ca66cfaa6` | Venus `0669767e4652682b95c60896c177178232a63755`; bounded model and framing checks plus native connection-loss and surface-startup smoke. | No Venus gap; Orbit retains its partial `ORB-C7` hardening gap. |

The Orbit proof revisions above establish source boundaries; they do not prove
a Venus contract. Every consumer proof records the exact Orbit revisions tested
and reports any gap to Orbit.

Orbit proof `c905bf9610581747f1b07565814b501ca66cfaa6` is not yet published
on `origin/edge`. The locked local proof passes, but a clean external Cargo
checkout cannot resolve the exact Git dependency until that Orbit history is
published.
