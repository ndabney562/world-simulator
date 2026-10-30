# Engineering Process

How World Simulator is built from Architecture V1 onwards. GitHub is the canonical source of truth: issues hold the specification of each piece of work, pull requests hold the change and its review, and `docs/` holds the architecture and these rules. Chat history is never the authoritative specification for anything that can live here.

| Document | Covers |
|---|---|
| [current-workstream.md](current-workstream.md) | Current status, owner, recorded delegation and next handoff |
| [friend-distribution.md](friend-distribution.md) | Ready-to-play sharing target, distribution options and pending decisions |
| [shared-development.md](shared-development.md) | Daily workflow, isolated checkouts, handoffs, phone/Mac access, and first milestone |
| [roles-and-responsibilities.md](roles-and-responsibilities.md) | Product Owner, assigned implementation engineer and independent assistant reviewer |
| [ai-development-workflow.md](ai-development-workflow.md) | The issue → branch → PR → CI → review → fix → approval → merge loop |
| [human-approval-gates.md](human-approval-gates.md) | What only the Product Owner can approve |
| [definition-of-done.md](definition-of-done.md) | When an issue and its PR are finished |
| [testing-policy.md](testing-policy.md) | What tests every change needs, and the rules for changing tests |
| [code-review.md](code-review.md) | Review severity levels and how findings are resolved |
| [architecture-change-process.md](architecture-change-process.md) | When an ADR is needed and how one is accepted |
| [branch-and-commit-conventions.md](branch-and-commit-conventions.md) | Branch names, commit messages, PR titles |
| [ci.md](ci.md) | What CI checks today and how it grows |
| [technical-prototype-backlog.md](technical-prototype-backlog.md) | The Technical Prototype issues and their dependencies |

Templates live in [`.github/ISSUE_TEMPLATE/`](../../.github/ISSUE_TEMPLATE/) and [`.github/pull_request_template.md`](../../.github/pull_request_template.md). AI collaborators start from [`AGENTS.md`](../../AGENTS.md).

Changes to these documents follow the same PR workflow and are merged under the [standing delegation](human-approval-gates.md#standing-delegation-2026-10-29).
