# Definition of Done

An issue is done when **all** of the following are true. Issues may add items; they may not remove these.

## Scope

- [ ] Every acceptance criterion in the issue is met, and the PR shows how (a test, command output, or screenshot).
- [ ] Nothing outside the issue's scope was implemented. Excluded scope stayed excluded.
- [ ] Discovered follow-up work is recorded as new issues and linked from the PR.

## Architecture

- [ ] The change respects the architecture guardrails in [`AGENTS.md`](../../AGENTS.md) and every accepted ADR.
- [ ] Any architecture change has an accepted ADR (see [architecture-change-process.md](architecture-change-process.md)).
- [ ] Dependency direction between crates matches `docs/architecture/17-repository-structure.md` §18.2.
- [ ] Any new dependency has a permissive licence and costs nothing; anything else went through [human-approval-gates.md](human-approval-gates.md).

## Tests and CI

- [ ] The tests required by the issue and by [testing-policy.md](testing-policy.md) exist and pass.
- [ ] All CI checks are green on the PR's latest commit.
- [ ] No test or check was weakened, skipped, or deleted without a documented justification and Product Owner approval.
- [ ] Determinism-relevant code states its tier ([25-determinism-policy.md](../architecture/25-determinism-policy.md)) and has tests for that tier.

## Code quality

- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` pass (for Rust changes).
- [ ] Public items have doc comments; each crate has a README stating its responsibility, owned state, emitted event types, and invariants.
- [ ] No `unsafe` outside `ws-ffi` unless justified in a comment and in the PR.
- [ ] No wall-clock time, unordered hash-map iteration, or thread-scheduling dependence in simulation logic.

## Documentation

- [ ] Docs affected by the change are updated in the same PR (crate README, architecture doc, process doc, `docs/data/` for new data sources).
- [ ] Performance-relevant changes record their measurements or explain why none were needed.

## Review and merge

- [ ] The PR template is filled in completely.
- [ ] ChatGPT has reviewed the diff; no Blocker or Major findings remain open.
- [ ] Minor findings are fixed or tracked in a linked follow-up issue.
- [ ] The Product Owner has approved where required and merged.
- [ ] The issue is closed and the Technical Prototype tracking issue is updated.
