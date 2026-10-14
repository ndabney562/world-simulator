# ws-time

## Responsibility

Simulation time as integers ([ADR 0014](../../../docs/adr/0014-time-representation.md), [07-coordinates-and-time.md](../../../docs/architecture/07-coordinates-and-time.md) §8.9):

- `SimTime`: signed `i64` microseconds of a uniform time scale since 2000-01-01T12:00:00 UTC. `Copy`, 8 bytes, totally ordered.
- `SimDuration`: signed `i64` microseconds.
- `UtcDateTime`: a validated proleptic Gregorian UTC label, converted to and from `SimTime` both ways through an embedded leap-second table (`LEAP_SECOND_TABLE_VERSION`).
- `Rate`: a system's native step, with epoch-aligned tick indices (`tick_index`, `tick_start`, `is_tick_start`).

It is a foundation crate (layer 0): it depends on no other workspace crate. Time zones, daylight saving, and astronomical time scales (TT, TDB) are out of scope until later stages.

## Owned state

None. It defines value types and pure functions; the leap-second table is a compile-time constant.

## Emitted event types

None.

## Invariants

- No floating point, no wall-clock reads, no allocation (Tier 1, [25-determinism-policy.md](../../../docs/architecture/25-determinism-policy.md)).
- No arithmetic operator impls: every operation that can overflow is `checked_*` (returning `TimeOverflow`) or `saturating_*`.
- `SimTime::to_utc` is total; `SimTime::from_utc(t.to_utc()) == Ok(t)` for every `t`.
- `UtcDateTime::new` accepts second 60 only at `23:59:60` on a tabulated leap-second date; that label maps to the uniform second between `23:59:59` and the next midnight.
- Before 1972-01-01, TAI − UTC is treated as a constant 10 s; after the last table entry (2016-12-31) no further leap seconds are assumed.
- Tick `k` of a `Rate` starts at `k * step` microseconds from the epoch.
- The golden table in `tests/golden.rs` is Tier 1: it must pass unchanged on Linux and macOS. Changing a value needs a `LEAP_SECOND_TABLE_VERSION` bump and Product Owner approval.
- No `unsafe` code and no runtime dependencies (`proptest` is a dev-dependency).
