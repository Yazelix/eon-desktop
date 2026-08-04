# Venus contract index

Statuses are `Planned`, `Partially proved`, and `Proved`. A proved contract
names an accepted immutable proof commit and its canonical checks.

| ID | Behavior and owner | Status | Consumed boundary | Canonical proof | Gap |
|---|---|---|---|---|---|
| `VEN-C1` | Venus deterministically validates and materializes one coherent Orbit-authored structured presentation frame and ordered revisions into native draw inputs without terminal authority. | Planned | Orbit `ORB-C4` and `ORB-C6` at accepted proof `e4fde443e625180d7332eff4dff366f64bee30a8` | None | Backend, composition, text, native-window, and visual evidence remain unimplemented. |
| `VEN-C2` | Venus owns native interaction collection and sends only Orbit's semantic input and resize messages. | Planned | Orbit `ORB-C5` at accepted proof `e4fde443e625180d7332eff4dff366f64bee30a8` | None | Keyboard, paste, focus, mouse, resize, and platform input remain unimplemented. |
| `VEN-C3` | Venus is transient: closing or crashing the client does not own or terminate the Orbit session, and reopening materializes its coherent current state. | Planned | Orbit `ORB-C1` through `ORB-C4` proved at `e4fde443e625180d7332eff4dff366f64bee30a8`; `ORB-C7` has partial evidence at that revision | None | Native lifecycle and reconnect evidence remain unimplemented; Orbit's adversarial `ORB-C7` hardening remains in `orb-bi4.5`. |
| `VEN-C4` | Venus gives bounded, explicit client UX for attach rejection, protocol incompatibility, invalid frames, and Orbit loss rather than hanging or silently inventing state. | Planned | Orbit `ORB-C3` and `ORB-C4` proved at `e4fde443e625180d7332eff4dff366f64bee30a8`; `ORB-C7` has partial evidence at that revision | None | Failure states and recovery UX remain unimplemented; Orbit's adversarial `ORB-C7` hardening remains in `orb-bi4.5`. |

The Orbit proof revision above establishes the source boundary; it does not
prove a Venus contract. Every consumer proof records the exact Orbit revision
actually tested and reports any gap to Orbit.
