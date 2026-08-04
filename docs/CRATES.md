# Venus crate decisions

No graphical dependency is selected. References and candidates become direct
or architecture-shaping dependencies only after the `AGENTS.md` crate gate and
explicit user approval.

| Boundary | Selected shape | Status | Credible candidates | Decision requirement |
|---|---|---|---|---|
| Orbit protocol consumer | Canonical Orbit wire representation at an exact accepted revision | Planned | Reuse the Orbit-owned schema directly; generated binding only if already canonical; no handwritten mirror | Prove bounded deterministic decode and compatibility reporting without a second schema or terminal owner. |
| Backend and native event boundary | Undecided | Planned | winit; another maintained native-window owner; smallest owned platform seam | Compare lifecycle, input methods, accessibility, Linux behavior, macOS feasibility, dependency/build cost, and owned LOC. |
| Composition, text, and renderer | Undecided | Planned | wgpu with glyphon or cosmic-text; a libghostty-derived renderer boundary; Sugarloaf if a concrete need activates it; smallest owned implementation | Study FrankenTUI backend and patch-stream ADRs plus OpenTUI Rust composition before selecting a shape. Prove Unicode, graphemes, cursor, damage, clipping, layering, and deterministic headless inputs before manual visual dogfood. |
