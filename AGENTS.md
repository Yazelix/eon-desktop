# Agent Guidelines

This file is self-contained. Canonical protocol text was rendered into it;
the source repository is needed only to update or verify the import.
Do not edit this generated file directly. Edit `.agent-protocols.local.md`
or `.agent-protocols.exceptions.json`, then render from the pinned source.

## Protocol import record

- Source: `https://github.com/luccahuguet/starcompass`
- Source commit: `c524d3c47592ce006b749d0c6db7fd0c3478ce63`
- Profiles: `greenfield`
- Manifest: `.agent-protocols.json` (schema 1)

| Protocol | Version | SHA-256 |
| --- | ---: | --- |
| `AP-SCOPE-001` | 3 | `cae30f9031beea4f831a89445f35807f50618e0f2cb0432f3b94bec40da9aeef` |
| `AP-CONTRACT-001` | 4 | `cdf2e5d69cefd34ceeeaa6b7511b21f921e2be50c17d3c01b85120b20234cff7` |
| `AP-REFERENCE-001` | 3 | `186fa74aa530c9c76ae507685461dff2b1506ecb5087230ded51f02cecb9231a` |
| `AP-MINIMAL-001` | 4 | `612bc9c62a20adf7332724d4663af0a0e591d624de2a76b1c257a9ed428db1f6` |
| `AP-DEPENDENCY-001` | 2 | `ded49add538b82de0a9c522bc8a34720f4ebbb47f39fc2f7ddb25e7aad1700d3` |
| `AP-OWNERSHIP-001` | 2 | `d218a4e0625b659ec366284110bdfce02bd66cb229ab6ba07f2317092ea13053` |
| `AP-TEST-001` | 3 | `58b5837cb679e958192b366bb15d6e34649f5a91ff9f4accdc7edb4ef5cdb873` |
| `AP-PROOF-001` | 4 | `2762fdf80ba2a36bb1e8844a44959bea7a6309dcd7a5796029a06a7ad9c26692` |
| `AP-PLAN-001` | 9 | `a1671c41d8a5c059a7138914ae4903b1d305473cf9681e293fa7d16c0b939b15` |
| `AP-CI-001` | 3 | `c7cb65a81ce8434d02f2d19306bf93358f3624d0b77886bc91d4de93f7a779ba` |
| `AP-EXCEPTION-001` | 2 | `2c4f00299922edac83286821af07ad485e5a630bca6acbbce918d87614abb395` |
| `AP-GIT-001` | 5 | `f2c3254311de57a23a13aa382fe3285e65bb34e535ecd813d1be783ca79d4bf9` |

### Local exceptions

No local exceptions.

## Canonical protocols

### AP-SCOPE-001 — User-owned scope

The user owns scope. Inspection, audit, diagnosis, explanation, and
recommendation authorize no implementation, external-state, or durable-planning
write. Implement only a chosen outcome.

Do not silently add features, compatibility promises, public surfaces,
migrations, repositories, or planning items. Preserve user-owned inputs and
artifacts; replace or delete an exact target only when the outcome requires it.
Otherwise write a distinct result. State consequential assumptions and stop
when a missing scope choice would materially change the result. “Finish” adds
persistence, not authority. Report out-of-scope findings unless the user chose
a durable destination.

### AP-CONTRACT-001 — Contract-driven changes

Before code shape, state the smallest observable contract: consumer, trigger,
result, and important failures. Of the contracts consistent with the request
and evidence, choose the fewest unsupported guarantees or restrictions; leave
reasonable future behavior unspecified. Give it a stable ID only when later
consumers need one.

Keep docs, help, examples, and configuration aligned with current commands,
paths, flags, defaults, and availability; label planned, partial, or gated
behavior. Choose one source-of-truth owner, the cheapest falsifying check, and
the smallest complete vertical slice. Update the contract first for an
intentional behavior change. Implementation details are not contracts unless a
component must rely on them.

### AP-REFERENCE-001 — Evidence before code shape

Before a consequential code shape, read affected instructions, code, contracts,
tests, and required subsystem references. Record the adopted, rejected, or
unresolved mechanism; separate evidence from inference and revisit it after a
material shape change. Memory, summaries, and reputation are discovery only.

Apply source-license terms to their exact actors, uses, conditions,
beneficiaries, and direction. Choosing a provider does not make a user act for
it; an “including” example remains scoped by its condition. Inspection is
distinct from copying, adaptation, redistribution, dependency selection, and
incorporation, so a restriction on one does not spread to another. If an
interpretation would block required evidence, identify the exact clause and
roles and resolve material ambiguity with the user. Review does not require
reuse.

### AP-MINIMAL-001 — Minimum sufficient implementation

After identifying the contract, evidence, owner, flow, and boundaries, take the
first sufficient option: no change; existing owner, helper, or pattern;
standard library or native platform; accepted dependency that owns the
behavior; minimum correct local code.

Judge the whole lifecycle, including duplicate truth, coordination, coupling,
migration and removal, portability, operations, proof, and agent context. Scope
instructions narrowly; retain non-obvious constraints and reusable
behavior-changing workflows, not generic or duplicated policy. Load details
only when needed.

A patch is not minimal if it preserves a wrong or duplicate owner, treats a
symptom below its shared cause, bypasses a boundary, or raises downstream cost.
Prefer deletion, direct ownership, and fewer files; use patch size only between
equally correct system shapes. Never remove required behavior, trust-boundary
validation, data-loss protection, security, accessibility, or the cheapest
runnable check for non-trivial logic.

When available, use upstream
[Ponytail](https://github.com/DietrichGebert/ponytail/tree/16f29800fd2681bdf24f3eb4ccffe38be3baec6b)
as a fallible code-shape bias after these constraints. Its absence does not
suspend them, and its hooks do not prove compliance.

### AP-DEPENDENCY-001 — Dependency gate

Before adding a dependency, name its capability and contract. Compare owned
code, the standard library, and credible candidates for correctness,
maintenance, platform and license fit, transitive weight, API stability, and
net complexity. Record the choice, meaningful rejections, and replacement cost;
pin it and prove the relied-on behavior. Remove it when its ownership no longer
justifies its cost.

### AP-OWNERSHIP-001 — One owner per invariant

Give every invariant, state transition, and user-visible policy one owner;
consumers must not reconstruct or reinterpret it. Name the owner before adapters
or synchronization and delete duplicates. Put policy at the highest layer with
enough context and enforcement at the lowest correct layer. Make cross-boundary
data explicit and versioned for independently released consumers.

### AP-TEST-001 — Strong and few tests

Protect meaningful contracts, regressions, boundaries, and failures with a few
strong tests. Use TDD for deterministic helpers, parsers, protocol behavior,
and regressions whose expected behavior is known first; use contract-first
integration checks for layout, runtime, architecture, forks, and dogfood.

For consequential agent-instruction, prompt, or skill changes, run isolated
representative tasks without supplying the expected conclusion. Structural
checks prove structure, not agent behavior. Test observable effects; delete
duplicate proof, implementation trivia, and scaffolding. Guard absence only
when absence is a security, licensing, size, ownership, or known-regression
contract.

### AP-PROOF-001 — Explicit proof lifecycle

Proof supports only its recorded command or observation, revision, environment,
result, and exercised surface. Distinguish proposed, implemented, mechanically
verified, dogfooded, accepted, and promoted states; rerun stale proof and never
widen its claim.

For performance claims, measure the bottleneck, compare the same representative
workload and environment with a recorded baseline, retain a correctness oracle,
and report a distribution or bound. Preserve constraining negative results.

A review or simplification pass that materially changes its subject invalidates
completion. Repeat it on the revised state and declare convergence only after a
full pass finds no actionable in-scope bugs or simplifications.

### AP-PLAN-001 — Durable planning state

Keep later-needed outcomes and constraints in the designated planning system or
canonical docs. Issues represent chosen goals, decisions, material defects, or
schedulable follow-ups; methods stay in their owning issue.

A run includes automatic continuations and ends when control returns. Planning
reads are unrestricted. A run that writes planning state or implementation uses
one shape:

1. Own at most one issue; create, claim, update, implement, or close only it.
2. With explicit user authorization, create, update, or close a named or
   accepted planning-only batch; claim nothing and edit no implementation.

Do not combine these shapes in one run.

Bind implementation work to its issue before planning or code writes. When that
issue completes, blocks, or hands off, return without starting another. Report
unapproved findings; create separate issues only for material out-of-scope or
independently schedulable work, and outside an authorized batch defer creation.

Keep editable fields at accepted current state after review, simplification, or
verification; reserve append-only history for needed evidence. Preserve
contracts, decisions, dependencies, acceptance evidence, material negative
results and failures, constraining rejections, and approvals. Keep raw logs with
their proof. Keep status honest, model only real prerequisites, and reconcile
before handoff. Use the designated issue tool; never edit its storage directly.

### AP-CI-001 — Bounded continuous integration

Before hosted automation, name its contract and why local proof is
insufficient. Bound triggers, jobs, timeouts, permissions, artifacts, cache,
concurrency; specify fork and secret behavior. Prefer one cheap deterministic
job. Record when to expand, reduce, or remove. Confidence must justify its
financial, latency, security, and maintenance costs, including private minutes
and storage.

### AP-EXCEPTION-001 — Explicit local exceptions

Only an exception approved by the user or named authority may narrow, replace,
or suspend an import. Record protocol ID, exact scope, reason, approver, date,
and any expiry or review condition. Unrecorded conflicts are drift; additive
detail is an overlay.

### AP-GIT-001 — Safe repository history

Repository history is shared user state. Inspect status and local policy;
preserve unrelated or concurrent work and use the least destructive sufficient
operation.

Amend the unpublished task commit for same-task corrections; refresh downstream
commits and generated artifacts. After external reliance (push, promotion,
release, or pin), commit corrections separately. Never reset, discard,
force-push, rewrite published history, or create a branch without authority.
Verify intended diff before commit and remote state after push. Roll back with a
reviewed revert unless local policy says otherwise.

## Repository-local rules

Eon Desktop is the repository for Eon for desktop. Its Venus subsystem
materializes Orbit-authored presentation state and sends semantic interaction.
Eon Sessions contains Orbit, which remains the sole owner of PTYs and terminal
authority; Eon owns product orchestration, policy, composition, and distribution.

## Status

This repository implements the accepted `ven-upt.2` standalone Linux slice and
the accepted `ven-c87` Eon workspace slice. The accepted `VEN-C8` proof remains
Venus `2dfe364fdc6a86ec183bf846d1b233e7a64de0dd`; direct workspace shortcuts and
fitting pane-header materialization are an uncommitted `ven-599` candidate.
Broader native Wayland and macOS remain unproved.

## Product boundary

Venus consumes one exact accepted Orbit protocol revision and, in workspace
mode, one exact accepted Eon workspace protocol revision. It may validate and
render structured presentation frames, materialize Eon-authored workspace
topology, collect native input, send semantic actions and resize requests, and
explain client-visible failures.

Venus must not own a PTY, parse raw PTY output, instantiate another terminal
emulator, answer terminal queries, reconstruct hidden terminal state, or keep a
second protocol schema. Report an insufficient Orbit contract to Orbit instead
of compensating in the client.

The current Linux slice is one native window. It supports either one standalone
local Orbit session or Eon-authored horizontal tabs and ordered per-active-tab
vertical accordion pane headers around exactly one expanded Orbit surface. It
does not include arbitrary split trees, simultaneous expanded panes, reordering,
sidebars, popups, settings, visual effects, configuration, remote or web access,
plugins, shell integration, compatibility windows, macOS implementation,
packaging, or distribution.

A new Venus module is product scope, not an implementation detail.

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

Keep one active implementation frontier across Eon Sessions, Eon Desktop, and
Eon unless the user authorizes parallel product work. Research and Beads may
prepare Venus without adding production code. Do not build against speculative
Orbit APIs.

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

Research after implementation does not satisfy this Venus gate. Record license
compatibility at the gate, especially for sources with nonstandard restrictions.

The independent actor analysis in `AP-REFERENCE-001` applies to the user and
Yazelix. When the user names a public project or it materially informs
architecture, treat its code at an exact release or commit as required evidence
for the Venus reference gate. Implement Venus-owned code unless the user
explicitly approves another reuse boundary.

## Crate and framework gate

`docs/CRATES.md` is the durable decision index. For a direct or
architecture-shaping dependency, extend `AP-DEPENDENCY-001` with:

- exact release or commit;
- project-owned LOC removed or introduced;
- direct, transitive, native, build, and Nix cost;
- upgrade burden;
- Linux behavior, macOS feasibility, input methods, accessibility, and future
  browser implications;
- focused proof or measurement for disputed claims.

Add the matrix to an append-only `Crate gate` comment and stop for user approval
before manifest edits. Update `docs/CRATES.md` after an accepted decision.

## Rendering and test discipline

Keep one deterministic path from canonical Orbit frames to Venus-owned draw
inputs. A headless backend or bounded recorded corpus may prove decoding,
composition, damage, ordering, and failure behavior; it does not prove visual
quality or become another product backend.

Do not maintain a handwritten mirror of the Orbit wire schema, parse raw VT, or
claim that a lossy projection proves rich-presentation fidelity. Prefer golden
state transitions and property checks for deterministic composition boundaries,
then focused manual dogfood for native window behavior, text quality, input,
accessibility, and lifecycle.

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

A missed Venus gate requires stopping and recording the unmet rule. Record any
user-authorized redo or narrowly named exception in an append-only `Protocol
exception` comment with the rule, reason, scope, user decision, retained
evidence, and proof effect. It never pretends the skipped gate passed or carries
into another Bead.

## Git workflow

Work directly on `edge`. Do not merge or promote without explicit user
direction.

## Beads

Use `br` for all issue work. Serialize its writes and run
`br sync --flush-only` before committing or handing off Beads changes.

Use `bv` only with `--robot-*` flags; bare `bv` opens an interactive TUI. Start
read-only triage with `bv --robot-triage --format toon`, then confirm claimable
state with `br show <id> --json` or `br ready --json` before writing.

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
