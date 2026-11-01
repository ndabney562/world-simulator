# Architecture Change and ADR Process

Architecture V1 (commit `fe2f3dbb43a88411509d861f038b80de73d35c13`, merged into `main` by PR #1) is frozen. It changes only through this process.

## When an ADR is needed

Write an ADR when a change would:

- Contradict or replace an accepted ADR or an architecture guardrail in [`AGENTS.md`](../../AGENTS.md).
- Change the Rust ↔ Unity boundary (`ws-ffi` surface, snapshot formats), save formats, ID schemes, coordinate or time representations, or the event record.
- Lock in one of the "decide in the Technical Prototype" choices (S-1 to S-8 in [21-decisions-now-vs-later.md](../architecture/21-decisions-now-vs-later.md)), such as the ECS library or serialization format.
- Lock in one of the "decide now" items (D-4 to D-22) in more detail than the architecture docs do.
- Add a major dependency that is hard to remove later.
- Change stage scope or stage order.

Routine implementation choices inside an issue's scope do not need an ADR; they are explained in the PR.

## Steps

1. **Open an architecture-change issue** from the template, describing the problem, the options, and the proposed decision.
2. **Draft the ADR** as `docs/adr/NNNN-short-title.md` from [`docs/adr/0000-template.md`](../adr/0000-template.md), with status **Proposed**, and update the affected architecture docs in the same PR.
3. **Put the ADR and its implementation in one PR** (decided by Noah on 2026-10-31, #144). There is no separate design-only PR first; Noah still sees the ADR in the PR. An ADR with no code yet (for example a spike result) may still go alone.
4. **A different assistant reviews** the PR like any other diff, or a fresh-context review agent when none is available ([standing delegation](human-approval-gates.md#standing-delegation-2026-10-29)).
5. **The implementer accepts and merges** under the [standing delegation](human-approval-gates.md#standing-delegation-2026-10-29); Noah can reverse any ADR. The ADR's status is set to **Accepted** in the PR before merge, and the ADR index in [`docs/adr/README.md`](../adr/README.md) is updated.
6. **Later work** that builds on the decision cites the ADR.

## Superseding

An accepted ADR is never edited to change its decision. A new ADR supersedes it; the old ADR's status becomes `Superseded by NNNN`, and both are linked. When only part is superseded, the status becomes `Accepted; <part> superseded by NNNN`. Typos and broken links may be fixed in place.

## Spikes

A Technical Prototype spike (for example TP-011 or TP-012) produces a written result: a short note in `docs/spikes/` or a Proposed ADR. A spike's code is throwaway unless an issue says otherwise.

There is no architecture changelog: git history and PR bodies record each change (#144).
