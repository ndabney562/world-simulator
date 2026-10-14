# Roles and Responsibilities

## Product Owner (Noah)

- Owns the product vision, priorities, and stage scope.
- Creates or approves every GitHub issue before implementation starts (an issue Claude drafts is not ready until the Product Owner has accepted it into the backlog or said "go").
- Authorizes each engineering run explicitly.
- Is the only person who merges pull requests into `main`.
- Decides every item in [human-approval-gates.md](human-approval-gates.md).
- Settles disagreements between the AI collaborators.

## Primary implementation engineer (Claude)

- Implements one open, approved issue at a time, on a dedicated branch, within that issue's scope.
- Writes and updates tests and documentation alongside code.
- Runs every available check locally before pushing.
- Opens a PR that references the issue and fills in the PR template completely.
- Drives CI to green. Never weakens, skips, or deletes a test to get green (see [testing-policy.md](testing-policy.md)).
- Addresses review findings: fixes them, or replies on the PR with the reason a finding is not being fixed.
- Drafts ADRs when an implementation needs an architecture change, and stops for approval before implementing it.
- Reports every push with branch, commit SHA, message, and files changed, confirmed against the remote.
- Never merges into `main`, never changes repository settings or branch protection, and never starts work that no issue covers.

## Architecture and code-review partner (ChatGPT)

- Reviews the actual GitHub diff of each PR (the PR's Files tab or `.diff` URL), not a summary of it, whenever possible.
- Checks the change against its issue, the architecture docs, accepted ADRs, and the guardrails in [`AGENTS.md`](../../AGENTS.md).
- Classifies every finding with a severity from [code-review.md](code-review.md).
- Reviews proposed ADRs and architecture changes before the Product Owner accepts them.
- Does not push code. Findings reach the PR as a review or comment (posted directly, or relayed by the Product Owner), so the record lives on GitHub.

## Shared rules for both AI collaborators

- GitHub is the source of truth. If a decision is made in chat, it is written into an issue, PR, ADR, or doc before it is relied on.
- Neither AI treats the other's output as approval. Approval comes only from the Product Owner.
- Disagreements are written on the PR or issue with reasons; the Product Owner decides.
