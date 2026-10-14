# Code Review

## What reviewers look at

The **actual diff** on GitHub (the PR's Files tab, or `https://github.com/ndabney562/world-simulator/pull/<N>.diff`), checked against:

1. The issue: is every acceptance criterion met, and is nothing outside scope included?
2. The architecture docs and accepted ADRs, and the guardrails in [`AGENTS.md`](../../AGENTS.md).
3. Correctness, including edge cases and determinism tier.
4. Tests: do they exist, do they test the right thing, would they catch a regression?
5. Performance against mobile budgets, where relevant.
6. Readability and maintainability for collaborators working with partial context.

## Severity levels

Every finding carries one severity, written at the start of the comment, for example `**[Blocker]** …`.

| Severity | Meaning | Merge rule |
|---|---|---|
| **Blocker** | Wrong behaviour, a broken or weakened test, a violated architecture guardrail or ADR, a determinism tier violation, a licence or cost problem, data loss, a security problem, or work outside the issue's scope | Must be fixed before merge |
| **Major** | A significant problem that is not immediately breaking: missing required tests, a design that will force rework in the next issues, a missing invariant, a performance risk against the mobile budget | Must be fixed before merge, unless the Product Owner explicitly defers it to a linked follow-up issue |
| **Minor** | A real but small problem: unclear naming, a missing doc comment, a weak test name, a small inefficiency | Fixed in this PR or tracked in a linked follow-up issue; does not block merge |
| **Nit** | Style or preference | Optional; the implementer decides |
| **Question** | The reviewer needs information to judge | Must be answered on the PR; may turn into a finding |

## Resolving findings

- The implementer replies to every Blocker, Major, and Minor finding, either with the commit that fixes it or with the reason it is not being fixed.
- A disagreement on a Blocker or Major finding is decided by the Product Owner, not by the implementer.
- Fixes are new commits on the PR branch; history is not rewritten during review.
- The reviewer (or the Product Owner on the reviewer's behalf) marks a finding resolved once the fix is verified in the diff.

## Review summary

Each review ends with a short summary: the count of findings by severity, and a verdict of **Approve**, **Approve with minor follow-ups**, or **Changes required**. Only the Product Owner's merge makes a PR final.
