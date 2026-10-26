# AI Development Workflow

```
Product Owner
  → GitHub Issue
  → Assigned Codex or Claude implementation branch
  → Pull Request
  → automated CI
  → Different assistant reviews the diff
  → Implementer fixes
  → CI
  → Product Owner approval (when required)
  → merge (Product Owner or explicitly delegated implementer)
```

## 1. Issue

- Every change starts from an **open GitHub issue** created from a template in [`.github/ISSUE_TEMPLATE/`](../../.github/ISSUE_TEMPLATE/): implementation, bug, or architecture change.
- An implementation issue states its objective, architectural references, exact scope, excluded scope, implementation requirements, acceptance criteria, required tests, performance considerations, dependencies, expected artifacts, and Definition of Done.
- An issue is small enough that its PR can be reviewed meaningfully in one sitting. If an issue grows during implementation, the extra work becomes a new issue.
- Work starts only after the Product Owner has authorized it, and only when the issue's dependencies are merged.

## 2. Branch

- One branch per issue, created from the current `main`. Naming is in [branch-and-commit-conventions.md](branch-and-commit-conventions.md).
- The branch contains only that issue's scope. Unrelated fixes discovered on the way become new issues (or, if they block the work, are called out in the PR).

## 3. Implementation

- Follow the architecture docs, accepted ADRs, and the guardrails in [`AGENTS.md`](../../AGENTS.md).
- If the work needs an architecture change, stop and follow [architecture-change-process.md](architecture-change-process.md) before continuing.
- Add or update tests as required by [testing-policy.md](testing-policy.md).
- Update the docs the change affects (crate README, architecture doc, ADR status, process docs).
- Run every available local check (format, lint, build, tests) before pushing.

## 4. Pull request

- Open a PR into `main` whose description follows [`.github/pull_request_template.md`](../../.github/pull_request_template.md) and contains `Closes #<issue>`.
- Open it as a **draft** while CI is still being driven to green; mark it ready for review when CI is green and the self-review is done.
- Report the push: branch, SHA, commit message, and files, confirmed against the remote.

## 5. CI

- CI runs on every push and PR ([ci.md](ci.md)). A red PR is never "waiting on review": the implementer fixes it first.
- A failing check is root-caused. Flaky tests are fixed, not retried until green.

## 6. Review

- The assigned reviewer (a different assistant from the author) reviews the **actual diff** on GitHub. Findings are recorded on the PR with a severity ([code-review.md](code-review.md)).
- Blocker and Major findings must be resolved before merge. Minor findings are fixed or turned into a follow-up issue linked from the PR. Nits are optional.

## 7. Fixes

- The implementer pushes fixes as new commits on the same branch (no force-push once a review has started), replies to each finding with what changed and in which commit, and drives CI back to green.
- Review repeats until no Blocker or Major findings remain.

## 8. Approval and merge

- The Product Owner approves where [human-approval-gates.md](human-approval-gates.md) requires it. The Product Owner merges, or the implementer merges within recorded named-workstream delegation after independent review and latest-head green CI.
- Merge method: **squash and merge** for normal issue PRs, so each issue lands as one commit on `main` whose message references the issue. Use **create a merge commit** only when the branch history itself must be preserved (as with the Architecture V1 freeze).
- After merge, the issue closes automatically through `Closes #N`; the implementer confirms it closed and updates any tracking issue.

## 9. Stop

After opening the PR, the implementer waits for independent review and owns fixes, CI, readiness and the next handoff. When technical merging and dependent starts are already delegated, finish that loop and continue from updated `main` without asking again. Otherwise record the concrete decision needed and stop only the work dependent on it. A CI monitor reporting recovery does not transfer ownership. See [current-workstream.md](current-workstream.md).

## Isolated work and handoffs

Use the [shared development guide](shared-development.md) for task ownership, separate checkouts, phone/Mac responsibilities, and the handoff format. Assign implementation to one assistant and review to another; the Product Owner authorizes work and either performs merges or explicitly delegates them.
