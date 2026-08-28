# Agent Guidelines

This file is self-contained. Canonical protocol text was rendered into it;
the source repository is needed only to update or verify the import.
Do not edit this generated file directly. Edit `.agent-protocols.local.md`
or `.agent-protocols.exceptions.json`, then render from the pinned source.

## Protocol import record

- Source: `https://github.com/luccahuguet/starcompass`
- Source commit: `f9f1f240c59e000eb3ac7d4a6a6d6d576386a24f`
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
| `AP-PROOF-001` | 5 | `2e8503daec35ded03c8ad5b1d76a86ce967537b553eed60ded5df5462eb8bd3a` |
| `AP-PLAN-001` | 10 | `2a4c4bf8c63dbff36ccbe642fa32408c130fa9a97217c424ca1bde6f43911bd8` |
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

Proof records revision, command/observation, environment, result, exercised
surface, and phase: proposed, implemented, mechanically verified, dogfooded,
accepted, or promoted. Never widen it; rerun after material change. Run a
complete suite once per materially unchanged candidate/environment. Docs,
contract metadata, and planning-only follow-ups reuse it: recorded source
commands are evidence, not a checklist; never rerun them.

Keep raw logs for costly reproduction, disputes, performance proof, or lossy
summaries; temporary paths are not evidence.

Performance claims require bottleneck measurement, fixed representative
workload/environment, baseline, correctness oracle, and distribution/bound.
Preserve constraining negatives.

Repeat review/simplification after changing its subject. Claim convergence
only when a full pass finds no actionable in-scope bugs or simplifications.

### AP-PLAN-001 — Durable planning state

Keep later-needed outcomes/constraints in planning or canonical docs. Issues
hold chosen goals/decisions, material defects, or schedulable follow-ups;
methods stay in owning issue.

A run includes automatic continuations until control returns; reads are
unrestricted. A writing run either:

1. Owns one issue at most; may create, claim, update, implement, or close it.
2. With explicit user authorization, creates/updates/closes a named/accepted
   planning-only batch; claims nothing and changes no implementation.

Never combine them.

Bind implementation to its issue before planning/code writes; return at
completion, blocking, or handoff. Report unapproved findings; create separate
issues only for material out-of-scope or independently schedulable work; defer
them outside such batches.

Keep fields current; append only evidence. Preserve contracts,
decisions, dependencies, acceptance, material negatives/failures,
constraining rejections, and approvals. Execution-baseline and reference-gate
comments must not repeat issue text; record only changing identities, dirty
state, adopted/rejected mechanisms, stop-condition decisions, and first
check. Before handoff, keep status honest, model real prerequisites, reconcile,
and use the issue tool; never edit storage.

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
the accepted `ven-c87` Eon workspace slice. `VEN-C8` is partially proved through
Venus `d2d798099934dcf8037bfad6ab856e40c9b989fe`; its remaining open proof is
fractional native scale. `VEN-C18` is proved through the same revision. Native
Linux Wayland is the sole platform; broader compositor coverage remains
unproved.

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
does not include arbitrary split trees, simultaneous expanded panes, sidebars,
popups, settings, visual effects, configuration, remote or web access, plugins,
shell integration, compatibility windows, packaging, or distribution. X11,
Xwayland, macOS, and other native platforms are unsupported product surfaces,
not compatibility targets.

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
- Linux Wayland behavior, input methods, accessibility, and future browser
  implications;
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

Native Linux Wayland is the sole implementation and verification target. Venus
selects Wayland and Vulkan only; X11, Xwayland, macOS, and other native platforms
are unsupported. Keep the existing Wayland host owner narrow, and do not add a
compatibility backend, portability layer, or direct protocol copy without an
explicit product decision.

Every implementation Bead records `neutral` or `isolated Wayland dependency`.

## Protocol exceptions

A missed Venus gate requires stopping and recording the unmet rule. Record any
user-authorized redo or narrowly named exception in an append-only `Protocol
exception` comment with the rule, reason, scope, user decision, retained
evidence, and proof effect. It never pretends the skipped gate passed or carries
into another Bead.

## Git workflow

Work directly on `edge`. Do not merge or promote without explicit user
direction.

For proof-bearing work, commit and verify the source candidate first. Then
update contract metadata, close the Bead, and commit both as one follow-up. The
contract and closure name the source commit, not the metadata commit.

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

Prefer one `mktemp -d` root for disposable proof state; clean it on exit and
place any sockets, scripts, captures, or worktrees beneath it.

Once Rust exists, keep these green for Rust changes unless the accepted project
surface defines a stricter command:

```sh
cargo fmt --check
cargo check --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
git diff --check
```
