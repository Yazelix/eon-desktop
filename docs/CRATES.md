# Venus crate decisions

No graphical dependency is selected. References and candidates become direct
or architecture-shaping dependencies only after the `AGENTS.md` crate gate and
explicit user approval.

| Boundary | Selected shape | Status | Credible candidates | Decision requirement |
|---|---|---|---|---|
| Orbit protocol consumer | Orbit-owned `orbit-protocol` 0.1.0 at `6e53fedb97f764f3683c83edcd9a5227b8f56e56`; no Venus manifest yet | Presentation codec accepted; local-session codec blocked on user decision | Exact-Git consumption of the accepted package; an approved typed extension for attach, input, and resize; no handwritten or generated mirror | The presentation codec already proves bounded decode and revision reduction. Before implementation, approve and prove the missing reusable local-session codec, then approve the exact Venus dependency. |
| Backend and native event boundary | Undecided | Planned | winit; another maintained native-window owner; smallest owned platform seam | Compare lifecycle, input methods, accessibility, Linux behavior, macOS feasibility, dependency/build cost, and owned LOC. |
| Composition, text, and renderer | Undecided | Planned | wgpu with glyphon or cosmic-text; a libghostty-derived renderer boundary; Sugarloaf if a concrete need activates it; smallest owned implementation | Study FrankenTUI backend and patch-stream ADRs plus OpenTUI Rust composition before selecting a shape. Prove Unicode, graphemes, cursor, damage, clipping, layering, and deterministic headless inputs before manual visual dogfood. |
