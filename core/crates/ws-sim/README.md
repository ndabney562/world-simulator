# ws-sim

## Responsibility

Owns the `World`: the authoritative simulation state and the loop that advances it. In the architecture it grows into the scheduler, command queue, journal, and system registry ([04-simulation-architecture.md](../../../docs/architecture/04-simulation-architecture.md) §5.1). As of TP-006 it holds a world created from a seed and a start time, advanced in fixed base steps by its `Clock` ([ADR 0014](../../../docs/adr/0014-time-representation.md)), and the toy world from `ws-world`, whose movement system runs once per base step ([ADR 0016](../../../docs/adr/0016-entity-identity-storage-and-randomness.md)).

## Owned state

- `seed: u64`, the root seed from `WorldConfig`, fixed for the life of the world.
- `clock: Clock` (`src/clock.rs`):
  - `start` and `now: SimTime`; `now` starts at `WorldConfig::start`, which the host supplies.
  - `base_step: Rate`, fixed; default 100 ms (`Clock::DEFAULT_BASE_STEP`).
  - `steps: u64`, base steps taken since creation; this is the world's tick.
  - `scale: TimeScale` (`Paused`, `X1`, `X10`, `X100`); default `X1`. Changed by a plain method call until TP-008 journals it.
  - `max_steps_per_call`, default 100 (`Clock::DEFAULT_MAX_STEPS_PER_CALL`), and `pending_us`, the sub-step remainder of scaled host time.
- `toy: ToyWorld` (`ws-world`), generated from the seed, the built-in toy dataset, and `WorldConfig::population` (`Population::TOY` by default: 4 to 12 residents per building).

## Emitted event types

None yet. The world event system arrives with TP-008.

## Invariants

- `World::new` starts at tick 0 at `WorldConfig::start`; each `World::step` increases the tick by exactly one and the time by exactly one base step.
- `now == start + tick * base_step` at all times.
- Clock steps fall on the base step's `ws_time::Rate` tick boundaries (and those of any rate whose step is a multiple of it) only if `start` is a whole number of base steps from the epoch. This is not enforced yet; hosts should supply an aligned start until TP-007 decides.
- `World::advance_by_host_elapsed(host_elapsed_us)` multiplies host time by the scale, adds the pending remainder, and takes every whole base step up to the per-call cap. Whole steps beyond the cap are dropped; the sub-step remainder is kept, including across pauses and scale changes. `Paused` takes zero steps.
- Integer arithmetic only, so the same sequence of host inputs gives the same steps and state, and splitting the same host time differently across calls gives the same result when no call reaches the cap.
- Time never wraps: a step past `SimTime::MAX` returns `TimeOverflow` and leaves the world unchanged.
- The seed never changes after creation.
- The state depends only on the configuration and the sequence of calls (Tier 2, [25-determinism-policy.md](../../../docs/architecture/25-determinism-policy.md)); time arithmetic itself is Tier 1.
- `World::step` advances the clock by one base step and runs the toy world's movement system for that step; `advance_by_host_elapsed` runs it once for every step it takes. A failed step (time overflow, or a movement error, never expected) leaves the world unchanged.
- `World::state_hash` hashes the seed, the tick, the time, and the toy world's canonical state (Tier 2). `tests/replay.rs` checks identical per-tick hashes for 10,000 ticks with the same seed, different hashes for a different seed, and the same hashes from a separate process.
- `World::new` fails, rather than renumbering anything, if two procedural IDs collide.
- The world never reads the wall clock, the environment, or the filesystem; elapsed host time and the start time are inputs.
- No `unsafe` code. Runtime dependencies: `ws-rng`, `ws-time`, `ws-world`.

## Performance

`cargo bench -p ws-sim` (criterion, dev-only) on the Linux machine used for TP-006 (Intel Xeon, 2.1 GHz), after 600 warm-up ticks so most agents are walking:

| Benchmark | Time | Per agent |
|---|---|---|
| `World::step`, 50 agents | 38 µs | 0.76 µs |
| `World::step`, 5,000 agents | 4.0 ms | 0.80 µs |
| `World::new`, 5,000 agents (generation) | 13 ms | 2.6 µs |

Most of the step is the per-agent `SurfacePos::reexpress` (a cell lookup, about 0.6 µs in `ws-math`'s benchmark) and building each agent's cell frame. Against [15-ios-performance-memory.md](../../../docs/architecture/15-ios-performance-memory.md) §16.3 (≤ 8 ms per frame for high-priority simulation): at the 100 ms base step and ×1, 5,000 agents cost about 4 ms once every three frames at 30 fps, so a frame that runs a tick has about 4 ms of headroom; at ×10 (about 3.3 ticks per frame) the same 5,000 agents would need about 13 ms and exceed the budget. Memory is about 96 bytes per agent (see the ws-world README), 0.5 MB for 5,000, against the 150–250 MB for materialized entities in §16.4. iPhone measurements belong to TP-011.
