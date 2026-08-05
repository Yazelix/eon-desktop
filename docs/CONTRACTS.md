# Venus contract index

Statuses are `Planned`, `Partially proved`, and `Proved`. A proved contract
names an accepted immutable proof commit and its canonical checks.

| ID | Behavior and owner | Status | Consumed boundary | Canonical proof | Gap |
|---|---|---|---|---|---|
| `VEN-C1` | Venus deterministically validates and materializes one coherent Orbit-authored structured presentation frame and ordered revisions into native draw inputs without terminal authority. | Partially proved | Orbit `ORB-C4` and `ORB-C6` at accepted proof `c905bf9610581747f1b07565814b501ca66cfaa6` | Candidate implementation; canonical corpus, locked checks, and Linux/X11 dogfood passed before an immutable Venus proof commit. | Exact Venus proof commit pending. |
| `VEN-C2` | Venus owns native interaction collection and sends only Orbit's semantic input and resize messages. | Partially proved | Orbit `ORB-C5` and canonical ORBS v1 at accepted proof `c905bf9610581747f1b07565814b501ca66cfaa6` | Candidate implementation; semantic mapping checks plus native key and resize dogfood passed before an immutable Venus proof commit. | Exact Venus proof commit pending. |
| `VEN-C3` | Venus is transient: closing or crashing the client does not own or terminate the Orbit session, and reopening materializes its coherent current state. | Partially proved | Orbit `ORB-C1` through `ORB-C4` at accepted proof `c905bf9610581747f1b07565814b501ca66cfaa6`; `ORB-C7` has partial evidence at that revision | Candidate implementation; transport lifecycle checks and native close/reopen dogfood passed before an immutable Venus proof commit. | Exact Venus proof commit pending; Orbit retains its partial `ORB-C7` hardening gap. |
| `VEN-C4` | Venus gives bounded, explicit client UX for attach rejection, protocol incompatibility, invalid frames, and Orbit loss rather than hanging or silently inventing state. | Partially proved | Orbit `ORB-C3`, `ORB-C4`, canonical ORBS v1, and partial `ORB-C7` evidence at accepted proof `c905bf9610581747f1b07565814b501ca66cfaa6` | Candidate implementation; bounded model, framing, and native failure checks passed before an immutable Venus proof commit. | Exact Venus proof commit pending; Orbit retains its partial `ORB-C7` hardening gap. |

The Orbit proof revisions above establish source boundaries; they do not prove
a Venus contract. Every consumer proof records the exact Orbit revisions tested
and reports any gap to Orbit.
