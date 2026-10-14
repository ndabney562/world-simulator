# ws-sim

## Responsibility

Owns the `World`: the authoritative simulation state and the loop that advances it. In the architecture it grows into the scheduler, command queue, journal, and system registry ([04-simulation-architecture.md](../../../docs/architecture/04-simulation-architecture.md) §5.1). As of TP-001 it is a skeleton: a world created from a seed that advances an integer tick counter.

## Owned state

- `seed: u64`, the root seed from `WorldConfig`, fixed for the life of the world.
- `tick: u64`, the number of steps taken since creation, starting at 0.

## Emitted event types

None yet. The world event system arrives with TP-008.

## Invariants

- `World::new` starts at tick 0, and each `World::step` increases the tick by exactly one.
- The seed never changes after creation.
- The state depends only on the configuration and the number of steps: two worlds with the same seed stepped the same number of times are equal (Tier 2, [25-determinism-policy.md](../../../docs/architecture/25-determinism-policy.md)).
- The world never reads the wall clock, the environment, or the filesystem.
- No `unsafe` code and no runtime dependencies.
