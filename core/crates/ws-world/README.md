# ws-world

## Responsibility

The world model. As of TP-006 (#7) it holds the Technical Prototype **toy world**: five box buildings at fixed offsets from the Bay Ridge reference point and a few dozen residents who walk between them ([ADR 0016](../../../docs/adr/0016-entity-identity-storage-and-randomness.md)). It proves identity, storage, randomness, movement across cell edges, and state hashing; it is not content. The world tree, aggregates, and real data arrive in later stages.

| Module | Provides |
|---|---|
| `dataset` | `TOY_BUILDINGS` (constant, `TOY_DATASET_VERSION = 1`); `Dataset` and `BuildingSite` |
| `ids` | Procedural IDs and generator decisions: `building_id`, `resident_id`, `resident_count`, `residents`, `next_target`; the crate's `Purpose` labels |
| crate root | `ToyWorld`: generation, insertion, the movement system `step`, `state_hash`; components `Building`, `Resident`, `Walk`; `WalkParams`, `WorldError` |

## Owned state

`ToyWorld`, held by `ws-sim`'s `World`:

- `seed`, `dataset_version`, and `WalkParams` (speed 1.4 m/s, wait 50 steps).
- `ws-ecs` columns: `buildings: Column<Building>` (dataset index, centre `SurfacePos`, box size); `residents: Column<Resident>` (generation address `(building_index, slot)` and home building); `positions: Column<SurfacePos>`; `walks: Column<Walk>` (velocity, target building, arrived flag, steps left to wait, targets drawn).

## Emitted event types

None. Events arrive with TP-008.

## Invariants

- **Tier 1 identity and generation.** Building IDs are `derive(seed, "ws-world/id/building", [dataset_version, index])`, resident IDs `derive(seed, "ws-world/id/resident", [dataset_version, building_index, slot])`, both in the procedural namespace. Residents per building are `min + below(derive(seed, "ws-world/gen/resident-count", [dataset_version, index]), max - min + 1)`, 4 to 12 by default. All integer; golden values in `tests/golden/toy_world.txt` were produced by an independent Python implementation and must match on Linux and macOS CI.
- **Target choice.** An agent's `k`-th target is `next_target(seed, agent, k, buildings, current)`: draw `k` of its stream `Stream::new(seed, "ws-world/agent/next-target", [agent])`, mapped with `below` onto the buildings other than the current one, in ascending ID order. It depends only on those inputs, never on the tick of arrival, other agents, or other draws (`tests/world.rs`).
- **Movement (Tier 2).** Each step, in ascending ID order, an agent counts down its wait, draws its next target, or walks `speed × dt` straight to the target centre in its cell's ENU frame, arriving (snapped to the centre) when that is within one step. Each walking agent's position is then re-expressed, so after every step every agent lies in its own cell (`occupied_cell() == cell()`), and an agent that is not moving never changes cell. Agents stay within the buildings' bounding box.
- **Collisions fail.** Generation returns `WorldError::Duplicate` if two IDs collide; nothing is renumbered. `tests/collisions.rs` checks 10,000 seeds (450,813 IDs: 50,000 buildings and 400,813 residents) for collisions within and across worlds.
- **State hash.** `state_hash` is SipHash-2-4 over the seed, dataset version, walk parameters, and every component of every entity (IDs, integers, cell IDs, float bits), column by column in ascending ID order. It excludes storage indices, capacities, addresses, and derived values. Stable within one build and platform (Tier 2). Worlds with the same entities hash equally however they were built.
- Integer simulation time comes in as `SimDuration`; no wall clock, no threads, no `unsafe`, no unordered maps.

## Performance

About 96 bytes per L1 agent in the columns (16 `Resident` + 24 `SurfacePos` + 32 `Walk` + 3 × 8 ID copies), well inside the 0.5–1 KB L1 target of 09 §10.8, because TP-006 agents have no needs, plans, or perception yet. Step cost is measured by `cargo bench -p ws-sim` (see the ws-sim README).

## Dependencies

`ws-ecs`, `ws-math`, `ws-rng`, `ws-time`.
