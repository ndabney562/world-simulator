# Testing Policy

The strategy is in [18-testing-strategy.md](../architecture/18-testing-strategy.md). This document sets the rules every PR follows.

## 1. Every change ships with tests

| Change | Required tests |
|---|---|
| New Rust function or type with logic | Unit tests in the same crate |
| Conversions, round trips, and invariants (frames, time, aggregation) | Property tests (`proptest`) in addition to examples |
| Tier 1 code (IDs, procedural decisions, cell membership) | Golden tests with committed expected values, identical on Linux and macOS |
| Tier 2 code (anything stepped by the scheduler) | Replay test: same inputs, same per-tick state hash, including a run in a separate process |
| LOD transitions | Round-trip conservation tests (`A(D(x)) = x` for counted quantities) |
| Commands, interventions, events | Tests asserting the journal entries, the events, and the causal links produced |
| Persistence | Save → load → state hash equality; schema version present |
| FFI surface | Rust-side tests of every exported function, plus C# tests once Unity exists |
| Bug fix | A regression test that fails before the fix and passes after |
| Performance-sensitive code | A benchmark or recorded measurement against the relevant budget ([15-ios-performance-memory.md](../architecture/15-ios-performance-memory.md)) |

## 2. Never weaken a test to get green

A failing test is information. It is fixed by fixing the code, or by correcting the test when the test itself is wrong. These actions **require a written justification in the PR and Product Owner approval** ([human-approval-gates.md](human-approval-gates.md)):

- Deleting, skipping (`#[ignore]`), or disabling a test or CI check.
- Loosening a tolerance, an assertion, or a performance budget.
- Updating Tier 1 golden values. This also needs a version bump of the generator, rules, or dataset whose output changed, because saves depend on those values.
- Marking a test as flaky. Flaky tests are root-caused and fixed; a re-run that passes is not a fix.

The justification states what the test checked, why that check is wrong or obsolete, and what now covers the behaviour.

## 3. Determinism tiers in tests

- Tier 1 tests compare exact values or hashes, and must pass on every CI platform.
- Tier 2 tests compare per-tick state hashes on the same build and platform. Golden files for Tier 2 may differ per platform and are kept separately.
- Tier 3 comparisons use tolerances and invariants, never hashes.
- Tier 4 (rendering, providers) is never compared for exactness. Provider tests assert that the core state hash is unchanged when providers switch.

## 4. Test hygiene

- Tests are deterministic: fixed seeds, no wall clock, no network, no dependence on test order.
- Test names say what they check (`round_trip_preserves_cohort_counts`, not `test_lod_2`).
- Shared fixtures and invariant checkers live in `ws-testkit` once it exists.
- Long-running tests (soak, statistical equivalence) run nightly, not on every PR, and are marked as such.

## 5. Invariant and architecture tests

Architecture rules that can be checked mechanically are checked in CI as they become available, for example:

- Crate dependency direction (§18.2 of [17-repository-structure.md](../architecture/17-repository-structure.md)).
- No crate under `core/` depends on Unity, rendering, platform APIs, or visual providers.
- No wall-clock APIs (`std::time::SystemTime`, `Instant`) in simulation crates.
- ADR index and documentation links are consistent (`scripts/check_docs.py`).

When a guardrail gets a mechanical check, the check is added to [ci.md](ci.md).
