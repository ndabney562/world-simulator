# Roles and Responsibilities

## Product Owner (Noah)

- Owns the product vision, priorities, and stage scope.
- Creates or approves every GitHub issue before implementation starts (an issue Claude drafts is not ready until the Product Owner has accepted it into the backlog or said "go").
- Authorizes engineering work explicitly, either per issue or as a recorded named workstream.
- Merges pull requests personally or delegates named technical merges with explicit conditions.
- Decides every item in [human-approval-gates.md](human-approval-gates.md).
- Settles disagreements between the AI collaborators.

## Implementation engineer (Codex or Claude, assigned per issue)

- Implements one open, approved issue at a time, on a dedicated branch, within that issue's scope.
- Writes and updates tests and documentation alongside code.
- Runs every available check locally before pushing.
- Opens a PR that references the issue and fills in the PR template completely.
- Drives CI to green. Never weakens, skips, or deletes a test to get green (see [testing-policy.md](testing-policy.md)).
- Addresses review findings: fixes them, or replies on the PR with the reason a finding is not being fixed.
- Drafts ADRs when an implementation needs an architecture change, and stops for approval before implementing it.
- Reports every push with branch, commit SHA, message, and files changed, confirmed against the remote.
- Merges only within recorded Product Owner delegation after independent review and latest-head green CI; never changes repository settings or branch protection, and never starts work that no issue covers.
- Owns the handoff through CI, review, readiness and merge (when delegated). Before pausing, records the next action, responsible owner and blocking condition. Successful infrastructure recovery returns readiness/merge handling to this owner.

## Architecture and code-review partner (a different assistant)

- Reviews the actual GitHub diff of each PR (the PR's Files tab or `.diff` URL), not a summary of it, whenever possible.
- Checks the change against its issue, the architecture docs, accepted ADRs, and the guardrails in [`AGENTS.md`](../../AGENTS.md).
- Classifies every finding with a severity from [code-review.md](code-review.md).
- Reviews proposed ADRs and architecture changes before the Product Owner accepts them.
- In the reviewer role, does not edit the implementation branch. Findings reach the PR as a review or comment (posted when authorized, or relayed by the Product Owner), so the record lives on GitHub.
- Codex may implement issues assigned by Noah; Claude may review those changes. Self-review does not replace review by a different assistant.
- Default roles remain Claude implementing and ChatGPT reviewing unless Noah assigns otherwise.

## Shared rules for all AI collaborators

- GitHub is the source of truth. If a decision is made in chat, it is written into an issue, PR, ADR, or doc before it is relied on.
- Neither AI treats the other's output as approval. Approval comes only from the Product Owner.
- Disagreements are written on the PR or issue with reasons; the Product Owner decides.
