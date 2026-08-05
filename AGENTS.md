# Agent Guidelines

Venus is the greenfield native graphical client for Orbit and Yazelix Astra.
It materializes Orbit-authored presentation state and sends semantic
interaction. Orbit remains the sole owner of PTYs and terminal authority;
Astra owns product orchestration, policy, composition, and distribution.

## Status

This repository implements the accepted `ven-upt.2` first Linux slice using the
`ven-upt.1` architecture and dependency selection. Further product expansion
remains inactive. Do not add another feature, dependency, platform, CI,
packaging, or release automation until the user chooses it.

## Core rule

The user decides scope. Do not add a feature, compatibility surface, module,
dependency, contract, or planning Bead until the user chooses that direction.

## Product boundary

Venus consumes one exact accepted Orbit protocol revision. It may validate and
render structured presentation frames, collect native input, send semantic
events and resize requests, and explain client-visible failures.

Venus must not own a PTY, parse raw PTY output, instantiate another terminal
emulator, answer terminal queries, reconstruct hidden terminal state, or keep a
second protocol schema. Report an insufficient Orbit contract to Orbit instead
of compensating in the client.

The initial slice is one Linux window attached to one local Orbit session. It
does not include tabs, panes, sidebars, popups, settings, visual effects,
configuration, remote or web access, plugins, shell integration, compatibility,
macOS implementation, packaging, or distribution.

## Contract index

`docs/CONTRACTS.md` is the canonical index of accepted behavior, ownership,
proof status, checks, and gaps. Contract IDs are stable and repository-qualified
as `VEN-C*`; never renumber or reuse them.

Every product implementation Bead names the Venus contracts it changes, proves,
consumes, hardens, or preserves and maps each consumed Orbit contract to its
exact proof revision. Repository tooling and documentation Beads explicitly
state that they change no product contract. Stop for user approval before
adding, changing, replacing, or retiring a contract.

`Proved` means the index names an accepted proof-bearing Git commit and the
canonical checks that passed for that exact revision. Uncommitted evidence is
only a candidate. Later work touching a proved owner must rerun its indexed
checks and advance the exact proof revision or record the remaining gap.

## One active frontier

Keep one active implementation frontier across Orbit, Venus, and Astra unless
the user authorizes parallel product work. Research and Beads may prepare Venus
without adding production code. Do not build against speculative Orbit APIs.

Before Venus implementation begins, record the exact accepted Orbit proof
commit and consumed `ORB-C*` revisions. Any boundary gap returns to Orbit. A
compatible or breaking change names all known consumers and the update order;
adapters and compatibility windows require explicit user choice.

## Execution baseline

Read-only discovery needs no run comment. Before the first production-code,
test, manifest, or generated-fixture edit for an implementation Bead, claim it
and add an append-only `Execution baseline` comment containing:

- current `HEAD` and `git hash-object AGENTS.md`;
- the Bead `updated_at`, affected `VEN-C*` contracts, and consumed Orbit proofs;
- intended production, test, manifest, and generated surfaces;
- pre-existing dirty paths that must be preserved;
- active agent or session identity.

New user direction wins. Record a `Rebaseline` comment before continuing under
a materially changed scope.

## Evidence-first reference gate

`docs/REFERENCES.md` routes design questions to primary sources. Every
implementation Bead classifies each named reference as required before code,
conditional on a named failure, comparison only, or rejected scope. An
unclassified reference is required before code.

After the execution baseline and before production, test, or manifest edits:

1. Reread the complete Bead and comments.
2. Read the relevant reference-map entries.
3. Inspect every required reference at an exact release or commit.
4. Add an append-only `Reference gate` comment recording contracts, identities,
   independently useful mechanisms, rejected surrounding scope, ownership and
   code-shape consequences, and the first meaningful check.
5. Stop if the evidence changes an ownership boundary, requires unapproved
   scope or dependencies, or contradicts the Bead.

Research after implementation does not satisfy this gate. Reference status does
not authorize source copying or a dependency. Record license compatibility at
the gate, especially for sources with nonstandard restrictions.

## User-directed source research

The user and Yazelix independently direct and benefit from this research. Their
choice of an agent or tool provider does not by itself make them act on its
behalf, for its benefit, or under its direction. Examples introduced by
`including` remain scoped by the condition they illustrate.

When the user names a public project or it materially informs architecture,
inspect its code at an exact release or commit. Do not substitute a license
summary, reputation, or secondary description when the research use is not
restricted. A claimed restriction must identify the exact clause, actor,
beneficiary, direction, and use; ask the user only about material ambiguity.

Source inspection does not authorize copying, adaptation, redistribution,
dependency selection, or incorporation. Record those separately and implement
Venus-owned code unless the user explicitly approves another reuse boundary.

## Crate and framework gate

`docs/CRATES.md` is the durable decision index. Before adding, replacing, or
removing a direct or architecture-shaping dependency, compare a broad credible
set, normally including no new dependency or an owned implementation. Record:

- exact release or commit;
- fit to the affected contracts and ownership boundary;
- project-owned LOC removed or introduced;
- direct, transitive, native, build, and Nix cost;
- API maturity, maintenance, license, security, and upgrade burden;
- Linux behavior, macOS feasibility, input methods, accessibility, and future
  browser implications;
- focused proof or measurement for disputed claims.

Add the matrix to an append-only `Crate gate` comment and stop for user approval
before manifest edits. A reference is not a selected dependency. Update
`docs/CRATES.md` after an accepted decision.

## Rendering and test discipline

Keep one deterministic path from canonical Orbit frames to Venus-owned draw
inputs. A headless backend or bounded recorded corpus may prove decoding,
composition, damage, ordering, and failure behavior; it does not prove visual
quality or become another product backend.

Do not maintain a handwritten mirror of the Orbit wire schema, parse raw VT, or
claim that a lossy projection proves rich-presentation fidelity. Use strong,
few contract checks. Prefer golden state transitions and property checks for
deterministic composition boundaries, then focused manual dogfood for native
window behavior, text quality, input, accessibility, and lifecycle.

## Platform discipline

Linux is the first implementation and verification target. The architecture
must remain credible for a native, signed, and notarized macOS client. Keep
window-system, GPU, font, clipboard, input-method, accessibility, and process
integration behind narrow platform owners. Do not create a speculative platform
framework or promise macOS support before the user chooses it.

Every implementation Bead records `neutral`, `isolated platform dependency`, or
`macOS blocker`. A blocker names the exact assumption, affected contracts,
replacement shape, and estimated removal cost, then stops for user choice.

## Protocol exceptions

Do not improvise around a missed gate. Stop and record the unmet rule. Only the
user may authorize a redo or a narrowly named exception. Record an accepted
exception in an append-only `Protocol exception` comment with the rule, reason,
scope, user decision, retained evidence, and proof effect. An exception never
pretends the skipped gate passed or carries into another Bead.

## Git workflow

Work directly on `edge`. Do not create another branch, merge, rebase published
history, force-push, or promote without explicit user direction.

## Beads

Use `br` for all issue work. Do not edit `.beads/` files directly. Serialize
`br` writes and run `br sync --flush-only` before committing Beads changes.

Use `bv` only with `--robot-*` flags; bare `bv` opens an interactive TUI.

## Documentation and LOC

Update the README LOC scorecard whenever tracked handwritten project files
change. Exclude `.git/`, Beads data, lock files, and generated artifacts.

Update `CHANGELOG.md` only for accepted user- or consumer-visible behavior,
commands, protocol compatibility, packaging, releases, or proven contracts.
Planning and governance changes do not require a changelog entry.

## Verification

Run the cheapest exact checks for the changed surface. During the planning
phase, verify Markdown paths, contract IDs, Beads state, the LOC scorecard,
`git diff --check`, and a clean or explicitly preserved worktree.

Once Rust exists, keep these green for Rust changes unless the accepted project
surface defines a stricter command:

```sh
cargo fmt --check
cargo check --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
git diff --check
```
