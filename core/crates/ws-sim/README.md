# ws-sim

## Responsibility

Owns the `World`: the authoritative simulation state and the loop that advances it. In the architecture it grows into the scheduler, command queue, journal, and system registry ([04-simulation-architecture.md](../../../docs/architecture/04-simulation-architecture.md) §5.1). As of TP-006 it holds a world created from a seed and a start time, advanced in fixed base steps by its `Clock` ([ADR 0014](../../../docs/adr/0014-time-representation.md)), and the toy world from `ws-world`, whose movement system runs once per base step ([ADR 0016](../../../docs/adr/0016-entity-identity-storage-and-randomness.md)). Since TP-007 its residents run at L1 or L2 per building, changed by `World::set_lod` ([ADR 0017](../../../docs/adr/0017-simulation-lod-l1-l2-round-trip.md)).

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
- `WorldConfig::start` must lie on the base-step grid (a whole number of base steps from the epoch, ADR 0017 §8): `World::new` returns `CreateError::UnalignedStart` otherwise and never rounds. So every clock step falls on a `ws_time::Rate` boundary of the base step.
- `World::advance_by_host_elapsed(host_elapsed_us)` multiplies host time by the scale, adds the pending remainder, and takes every whole base step up to the per-call cap. Whole steps beyond the cap are dropped; the sub-step remainder is kept, including across pauses and scale changes. `Paused` takes zero steps.
- Integer arithmetic only, so the same sequence of host inputs gives the same steps and state, and splitting the same host time differently across calls gives the same result when no call reaches the cap.
- Time never wraps: a step past `SimTime::MAX` returns `TimeOverflow` and leaves the world unchanged.
- The seed never changes after creation.
- The state depends only on the configuration and the sequence of calls (Tier 2, [25-determinism-policy.md](../../../docs/architecture/25-determinism-policy.md)); time arithmetic itself is Tier 1.
- `World::step` runs the toy world's tick for the step from `now` (reconcile residency with the building levels, L1 movement, the L2 event drain up to the end of the step), then advances the clock by one base step; `advance_by_host_elapsed` does the same for every step it takes. A failed step (time overflow, an exhausted target counter, or a violated invariant) leaves the world unchanged.
- `World::set_lod(request)` applies a set of `(building, level)` pairs as one atomic transaction at the current boundary and returns the numbers demoted and promoted, or a `LodError` with the world unchanged. `World::check_lod_invariants` runs the `ws-world` checker at the current time.
- `World::state_hash` hashes the seed, the tick, the time, and the toy world's canonical state (Tier 2). `tests/replay.rs` checks identical per-tick hashes for 10,000 ticks with the same seed, with and without scripted LOD requests, different hashes for a different seed, the same hashes from a separate process, and that `advance_by_host_elapsed` matches `step` with transitions between calls.
- `World::new` fails, rather than renumbering anything, if two procedural IDs collide.
- The world never reads the wall clock, the environment, or the filesystem; elapsed host time and the start time are inputs.
- No `unsafe` code. Runtime dependencies: `ws-ecs` (for `EntityId` in `set_lod`, re-exported), `ws-rng`, `ws-time`, `ws-world`.

## Performance

`cargo bench -p ws-sim` (criterion, dev-only) on the Linux machine used for TP-006 (Intel Xeon, 2.1 GHz), after 600 warm-up ticks so most agents are walking:

| Benchmark | Time | Per agent |
|---|---|---|
| `World::step`, 50 agents | 38 µs | 0.76 µs |
| `World::step`, 5,000 agents | 4.0 ms | 0.80 µs |
| `World::new`, 5,000 agents (generation) | 13 ms | 2.6 µs |

Most of the step is the per-agent `SurfacePos::reexpress` (a cell lookup, about 0.6 µs in `ws-math`'s benchmark) and building each agent's cell frame. Against [15-ios-performance-memory.md](../../../docs/architecture/15-ios-performance-memory.md) §16.3 (≤ 8 ms per frame for high-priority simulation): at the 100 ms base step and ×1, 5,000 agents cost about 4 ms once every three frames at 30 fps, so a frame that runs a tick has about 4 ms of headroom; at ×10 (about 3.3 ticks per frame) the same 5,000 agents would need about 13 ms and exceed the budget. Memory is about 96 bytes per agent (see the ws-world README), 0.5 MB for 5,000, against the 150–250 MB for materialized entities in §16.4. iPhone measurements belong to TP-011.

### Simulation LOD (TP-007)

`cargo bench -p ws-sim --bench lod` on the Linux machine used for TP-007 (Intel Xeon, 2.8 GHz, 4 vCPU; a shared cloud machine, slower than the TP-006 one: `world_step/5000` measures 5.4 ms on it, against 4.0 ms above, both on `main` before TP-007 and with it). Toy-world agents in five buildings, after 600 warm-up ticks:

| Benchmark | 30 agents | 3,000 agents | 100,000 agents |
|---|---|---|---|
| Demote all (`set_lod` → L2) | 16 µs | 1.71 ms (0.57 µs/agent) | 92 ms (0.92 µs/agent) |
| Promote all (`set_lod` → L1) | 18 µs | 1.88 ms (0.63 µs/agent) | 100 ms (1.00 µs/agent) |
| One base step, all L1 | | 3.30 ms | 106 ms |
| One base step, all L2 | | 0.91 µs | 49 µs |
| One simulated hour, all L2 | | 37 ms (44,307 events) | 2.2 s (1,470,819 events) |

- **Transitions.** A 3,000-agent transition takes 1.7 to 1.9 ms, inside the 4 ms (half of the 8 ms high-priority budget, 15 §16.3) that ADR 0017 §15 asks for. Per-agent cost rises by about 60% from 3,000 to 100,000 agents (cache misses in the larger columns), still within the ADR's 1 µs (demote) and 2 µs (promote) targets.
- **L2 against L1.** About 14.8 events per agent per simulated hour, as ADR 0017 §15 predicts. An hour costs 0.84 µs per event at 3,000 agents and 1.5 µs at 100,000, step overhead included (target ≤ 2 µs). An all-L1 hour at 3,000 agents is 36,000 steps of 3.30 ms, about 119 s, so L2 is about 3,200 times cheaper (about 1,700 times at 100,000), against the target of at least 1,000.
- A step with no event due costs a queue peek and a check of the (empty) unreconciled index: under 1 µs at 3,000 agents. Without that index the first measurement was 19 µs, because reconciliation scanned every agent.

### Movement frame reuse (#31, 2026-10-16)

On the same Apple M1 Pro Mac (arm64) with Rust 1.97.0, release `world_step/5000` measured **3.3330 ms before → 2.2909 ms after** (about 31% faster; after 95% interval 2.2826–2.3001 ms). `world_step/50` measured 35.443 → 23.904 µs. This reaches #31's approximate 2.4 ms desktop target. At ×10, an average 3.33 ticks per 30 fps frame costs about 7.64 ms; individual four-tick frames and device overhead still require measurement, so this is not an iPhone/frame-budget guarantee.

The L1 planner reuses one exact frame per occupied cell per step for target projection and re-expression; the full cell lookup and ADR 0016 edge rule remain unchanged. The ordered cache is temporary derived data and adds no component/save/hash state. It allocates by distinct moving cells; the all-L2 path returns before using it. See the [verification report](../../../docs/verification/issue-31/README.md) for full LOD comparisons, reproducible commands and direct baseline replay evidence. Older numbers above describe their original machines/revisions.
