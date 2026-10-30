# AGENTS.md: conventions for AI collaborators

This file is for Codex, Claude, ChatGPT, and any other AI collaborator working in this repository. Read it before doing anything else.

## Source of truth

GitHub is canonical. Specifications live in issues, decisions in ADRs (`docs/adr/`), architecture in `docs/architecture/` (Architecture V1, frozen), and process in `docs/process/`. Chat history is not a specification: if something decided in chat matters, it is written into GitHub first.

## Roles

- **Noah (Product Owner):** priorities, approvals, and merges personally or through recorded delegation, including the [standing delegation](docs/process/human-approval-gates.md#standing-delegation-2026-10-29). See [human-approval-gates](docs/process/human-approval-gates.md).
- **Codex or Claude:** implementation engineer for the issue Noah assigns; one writer per branch.
- **ChatGPT/Codex or Claude:** reviewer of the other assistant’s actual GitHub diff; an author’s self-review is not independent review. When no other assistant is available, a fresh-context review agent reviews instead ([standing delegation](docs/process/human-approval-gates.md#standing-delegation-2026-10-29)).
- Existing defaults remain Claude implementing and ChatGPT reviewing when Noah has not assigned different roles.

Daily setup and handoffs: [shared-development](docs/process/shared-development.md).

Details: [roles-and-responsibilities](docs/process/roles-and-responsibilities.md).

## Execution rules

1. Work only from an open GitHub issue that the Product Owner has authorized.
2. Create a dedicated branch from `main` ([conventions](docs/process/branch-and-commit-conventions.md)).
3. Implement only that issue's scope. Excluded scope stays excluded.
4. Add or update tests ([testing policy](docs/process/testing-policy.md)).
5. Update relevant documentation.
6. Run every available check locally (see [ci.md](docs/process/ci.md)).
7. Open a PR referencing the issue, using the [PR template](.github/pull_request_template.md).
8. Get the review required by the [standing delegation](docs/process/human-approval-gates.md#standing-delegation-2026-10-29), then merge after latest-head green CI and continue; stop only at a gate it keeps for Noah. Read [current-workstream](docs/process/current-workstream.md) and [human-approval-gates](docs/process/human-approval-gates.md) before asking again. Delegation never waives review, latest-head green CI or its excluded gates.

The full loop is in [ai-development-workflow](docs/process/ai-development-workflow.md); completion criteria are in [definition-of-done](docs/process/definition-of-done.md); review severities are in [code-review](docs/process/code-review.md).

## Architecture guardrails

Unless superseded by an accepted ADR:

- Rust owns authoritative simulation state.
- Unity owns rendering, presentation, input, and platform integration.
- Communication uses a coarse-grained commands-in / snapshots-out boundary; no per-entity calls across the FFI.
- Visual providers never modify simulation state (ADR 0008).
- Simulation LOD and rendering LOD are separate concepts.
- Procedural identity and structural baseline decisions follow Tier 1 determinism requirements (ADR 0010, [25-determinism-policy](docs/architecture/25-determinism-policy.md)).
- Events, history, and causal provenance are first-class (ADRs 0011, 0012).
- Player interventions are structured commands, never arbitrary code execution.
- Mobile performance (iPhone 16 Pro, ADR 0037) is a design constraint from the beginning.
- Geospatial and visual-provider work does not block the core Rust → FFI → Unity → iPhone path (ADR 0013).
- Do not prematurely implement the city, planet, solar system, LLM NPCs, detailed economics, or polished graphics.

## Code rules (Rust core)

- Nothing under `core/` depends on Unity, rendering, platform APIs, or providers. It builds and tests on Linux.
- Dependency direction: `ws-math`, `ws-time`, `ws-rng` → `ws-world`, `ws-events` → domain crates → `ws-sim` → `ws-persist` → `ws-ffi` ([17-repository-structure](docs/architecture/17-repository-structure.md) §18.2). Crates are created only when an issue needs them.
- `ws-ffi` is the only crate Unity links, and the only place `unsafe` is expected.
- Simulation logic uses integer simulation time, never the wall clock; iteration order is canonical (no unordered hash-map iteration affecting results); randomness comes from counter-based streams keyed by stable IDs.
- Each crate has a README stating its responsibility, owned state, emitted event types, and invariants.
- Dependencies must be free and permissively licensed (ADR 0007).

## Reporting

After every push, report the branch, commit SHA, message, and files changed, confirmed against the remote, so reviewers can check the diff directly.
