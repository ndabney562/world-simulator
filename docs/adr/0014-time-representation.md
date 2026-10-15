# ADR 0014: Simulation time representation (D-5)

- Status: Accepted (2026-10-15, by merging PR #18)
- Date: 2026-10-14
- Decided by: Noah (Product Owner), on Claude's proposal for TP-004 (#5)
- Issue: #5
- Supersedes: none

## Context
[21-decisions-now-vs-later.md](../architecture/21-decisions-now-vs-later.md) D-5 fixes simulation time as 64-bit integer microseconds from J2000, mapped to the real UTC calendar ([07-coordinates-and-time.md](../architecture/07-coordinates-and-time.md) §8.9, [04-simulation-architecture.md](../architecture/04-simulation-architecture.md) §5.3). Before the first code depends on it (TP-004), the details that saves and Tier 1 values will rely on must be exact: what instant the epoch is, what the unit counts, what the range is, how leap seconds are treated, and how the host's wall-clock measurements reach a core that never reads the wall clock ([25-determinism-policy.md](../architecture/25-determinism-policy.md), ADR 0010).

Constraints: no floating point in time arithmetic (Tier 1); no wall-clock reads in simulation logic (Tier 2); overflow must never wrap silently; time zones and astronomical time scales come later.

## Options considered
1. **Simulation time follows UTC directly** (a POSIX-style count where every day has 86,400 seconds): simple calendar arithmetic, but a leap second has no representation, two UTC labels collide or one is skipped, and durations across a leap second are wrong by a second.
2. **Simulation time is a uniform count of SI seconds, and UTC is a labelling computed through a leap-second table** (as TAI and GPS time work): every elapsed second is one second, durations are exact, every UTC label (including `23:59:60`) maps to exactly one instant; the cost is a small table and a lookup when converting to or from the calendar.
3. **Floating-point Julian dates**: natural for astronomy, but not exact, not Tier 1, and the precision degrades with distance from the epoch.

## Decision
Option 2.

1. **Type and unit.** `SimTime` is a signed 64-bit integer count of **microseconds** of a uniform time scale (SI seconds, no leap-second discontinuities). `SimDuration` is a signed 64-bit count of microseconds. Both live in `ws-time`.
2. **Epoch.** `SimTime(0)` is **2000-01-01T12:00:00 UTC** exactly, the civil J2000 instant named in §8.9. This is deliberately a UTC label, not the astronomical J2000.0 epoch (which is 2000-01-01T12:00:00 TT, 64.184 s earlier); TT and TDB conversions come later and will apply the offset.
3. **Range.** `SimTime::MIN..=SimTime::MAX`, about ±292,277 years around 2000. Every value in the range converts to a UTC label. Calendar dates are proleptic Gregorian with astronomical year numbering (year 0 exists; ISO 8601 years outside 0000–9999 carry a sign).
4. **Leap seconds.** `ws-time` embeds a versioned table of the dates on which IERS inserted a leap second (27 positive leap seconds, 1972-06-30 to 2016-12-31; `LEAP_SECOND_TABLE_VERSION = 1`).
   - UTC → `SimTime` and back is exact for every valid label, including `23:59:60` on a table date, which is the uniform second between `23:59:59` and the next midnight. `23:59:60` on any other date is rejected as invalid input.
   - A duration that spans a leap second includes it: from 2016-12-31T23:59:59Z to 2017-01-01T00:00:00Z is 2 s.
   - Before 1972-01-01 the mapping is proleptic: UTC is treated as a uniform scale with TAI − UTC fixed at 10 s (pre-1972 UTC's fractional rate offsets are not modelled).
   - After the last table entry no further leap seconds are assumed. A future table version that adds one changes only the UTC labels of instants after it; `SimTime` values, which are what saves and the journal store, are unaffected. Adding an entry bumps `LEAP_SECOND_TABLE_VERSION` and its golden tests.
5. **Overflow.** All arithmetic is checked or saturating and returns typed errors (`TimeOverflow`); no operator can wrap or panic on overflow. Release builds keep overflow checks on as a backstop.
6. **Steps.** A system's native step is a positive `SimDuration` (`Rate` in `ws-time`). Tick indices are `floor(SimTime / step)`, aligned to the epoch, computed with integer division only, so systems with commensurate steps share boundaries.
7. **Wall-clock input.** The core never reads a clock. The host (Unity) measures elapsed real time and passes it in as an unsigned integer number of microseconds (`advance_by_host_elapsed(host_elapsed_us: u64)`); the simulation clock converts it to whole fixed steps through an integer accumulator, multiplied by the time scale and capped per call. The same rule applies to the per-`advance` wall-clock budget of §5.2 when the scheduler arrives: it is a host-supplied integer input, never a clock read. A new world's start time is likewise an input (`WorldConfig`), never the device clock.

## Consequences
- Durations and replay are exact on every platform; time arithmetic is Tier 1.
- Converting to or from the calendar costs a binary search over 27 entries; simulation stepping never touches the calendar.
- Saves, the journal, and snapshots store `SimTime` as an `i64` of microseconds from 2000-01-01T12:00:00 UTC; changing the epoch, unit, or leap-second treatment later would need a save migration.
- Time zones and daylight saving (Vertical Slice), TT/TDB and Earth rotation (later stages), and journaled time-control commands (TP-008) build on this without changing it.
- Revisit if IERS changes UTC (the planned end of leap seconds by 2035 needs only that the table stops growing) or if microsecond resolution proves too coarse for a system.
