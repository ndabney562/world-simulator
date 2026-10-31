<!-- Title: "TP-NNN: <title>" for Technical Prototype work, otherwise "<type>(<scope>): <summary>". -->

Closes #<!-- issue number -->

## Summary
<!-- What this PR does, in plain language: Before / After. -->

## Architecture impact
<!-- Which guardrails, ADRs, and architecture docs this touches. "None beyond the issue's scope" is a valid answer; an architecture change needs an ADR in this PR. -->
- Determinism tier(s) of new code:
- Crate dependency changes:
- FFI / save-format / event-schema changes:

## Files changed
<!-- Grouped by area, one line each on why. -->
- 

## Tests performed
<!-- Exact commands run and their results. List new tests by name. -->
- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Other:

## Known limitations

## Performance implications
<!-- Measurements against docs/architecture/15-ios-performance-memory.md budgets, or why none apply. -->

## Follow-up work
<!-- Linked issues created for discovered work. -->

## Checklist
- [ ] Only this issue's scope is implemented
- [ ] Acceptance criteria are each demonstrated above
- [ ] No test or check weakened, skipped, or deleted (or justified below and approved)
- [ ] Docs updated
- [ ] Meets the [Definition of Done](https://github.com/ndabney562/world-simulator/blob/main/docs/process/definition-of-done.md)

## Handoff
<!-- Use docs/process/shared-development.md: objective/issue, owner/reviewer, branch/base/head SHA, changes, verification with tested SHA, blockers, next steps. Never include secrets. -->
