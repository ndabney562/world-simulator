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
3. **Open an ADR PR** that changes only docs. Architecture changes are never mixed into an implementation PR.
4. **ChatGPT reviews** the ADR PR like any other diff.
5. **The Product Owner accepts** by approving and merging. The ADR's status is set to **Accepted** in the PR before merge, and the ADR index in [`docs/adr/README.md`](../adr/README.md) is updated.
6. **Implementation follows** in separate issues that cite the ADR.

## Superseding

An accepted ADR is never edited to change its decision. A new ADR supersedes it; the old ADR's status becomes `Superseded by NNNN`, and both are linked. Typos and broken links may be fixed in place.

## Spikes

A Technical Prototype spike (for example TP-011 or TP-012) produces a written result: a short note in `docs/spikes/` or a Proposed ADR. A spike's code is throwaway unless an issue says otherwise.

## Changelog

Every change to `docs/architecture/` adds an entry to [`docs/architecture/CHANGELOG.md`](../architecture/CHANGELOG.md).
