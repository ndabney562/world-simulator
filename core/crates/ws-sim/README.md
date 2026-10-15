# ws-sim

## Responsibility

Owns the `World`: the authoritative simulation state and the loop that advances it. In the architecture it grows into the scheduler, command queue, journal, and system registry ([04-simulation-architecture.md](../../../docs/architecture/04-simulation-architecture.md) §5.1). As of TP-004 it holds a world created from a seed and a start time, advanced in fixed base steps by its `Clock` ([ADR 0014](../../../docs/adr/0014-time-representation.md)).

## Owned state

- `seed: u64`, the root seed from `WorldConfig`, fixed for the life of the world.
- `clock: Clock` (`src/clock.rs`):
  - `start` and `now: SimTime`; `now` starts at `WorldConfig::start`, which the host supplies.
  - `base_step: Rate`, fixed; default 100 ms (`Clock::DEFAULT_BASE_STEP`).
  - `steps: u64`, base steps taken since creation; this is the world's tick.
  - `scale: TimeScale` (`Paused`, `X1`, `X10`, `X100`); default `X1`. Changed by a plain method call until TP-008 journals it.
  - `max_steps_per_call`, default 100 (`Clock::DEFAULT_MAX_STEPS_PER_CALL`), and `pending_us`, the sub-step remainder of scaled host time.

## Emitted event types

None yet. The world event system arrives with TP-008.

## Invariants

- `World::new` starts at tick 0 at `WorldConfig::start`; each `World::step` increases the tick by exactly one and the time by exactly one base step.
- `now == start + tick * base_step` at all times.
- `World::advance_by_host_elapsed(host_elapsed_us)` multiplies host time by the scale, adds the pending remainder, and takes every whole base step up to the per-call cap. Whole steps beyond the cap are dropped; the sub-step remainder is kept, including across pauses and scale changes. `Paused` takes zero steps.
- Integer arithmetic only, so the same sequence of host inputs gives the same steps and state, and splitting the same host time differently across calls gives the same result when no call reaches the cap.
- Time never wraps: a step past `SimTime::MAX` returns `TimeOverflow` and leaves the world unchanged.
- The seed never changes after creation.
- The state depends only on the configuration and the sequence of calls (Tier 2, [25-determinism-policy.md](../../../docs/architecture/25-determinism-policy.md)); time arithmetic itself is Tier 1.
- The world never reads the wall clock, the environment, or the filesystem; elapsed host time and the start time are inputs.
- No `unsafe` code. Its only runtime dependency is `ws-time`.
