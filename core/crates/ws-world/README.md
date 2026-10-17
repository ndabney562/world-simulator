# ws-world

## Responsibility

The world model. As of TP-006 (#7) it holds the Technical Prototype **toy world**: five box buildings at fixed offsets from the Bay Ridge reference point and a few dozen residents who walk between them ([ADR 0016](../../../docs/adr/0016-entity-identity-storage-and-randomness.md)). It proves identity, storage, randomness, movement across cell edges, and state hashing; it is not content. As of TP-007 (#8) residents run at one of two simulation levels, L1 (stepped) or L2 (a discrete-event background schedule), chosen per building ([ADR 0017](../../../docs/adr/0017-simulation-lod-l1-l2-round-trip.md)). The world tree, aggregates, and real data arrive in later stages.

| Module | Provides |
|---|---|
| `dataset` | `TOY_BUILDINGS` (constant, `TOY_DATASET_VERSION = 1`); `Dataset` and `BuildingSite` |
| `ids` | Procedural IDs and generator decisions: `building_id`, `resident_id`, `resident_count`, `residents`, `next_target`; the crate's `Purpose` labels |
| `lod` | Simulation LOD (ADR 0017): `Lod`, `Background`, `Activity`, `Origin`; `ToyWorld::set_lod` (the transition transaction), `tick` (reconcile, L1 movement, L2 drain), `advance_background`, `check_lod_invariants`, `conservation_summary`; `LodError`, `LodViolation`, `TransitionReport` |
| crate root | `ToyWorld`: generation, insertion, the movement system `step`, `state_hash`; components `Building`, `Resident`, `Walk`; `WalkParams`, `WorldError` |

## Owned state

`ToyWorld`, held by `ws-sim`'s `World`:

- `seed`, `dataset_version`, and `WalkParams` (speed 1.4 m/s, wait 50 steps).
- `ws-ecs` columns: `buildings: Column<Building>` (dataset index, centre `SurfacePos`, box size); `node_lod: Column<Lod>` (each building's level, L1 or L2); `residents: Column<Resident>` (generation address `(building_index, slot)` and home building; the single identity store at both levels); the L1 store `positions: Column<SurfacePos>` and `walks: Column<Walk>` (velocity, derived; target building, arrived flag, steps left to wait, targets drawn); the L2 store `background: Column<Background>` (waiting at a building until a departure time, or walking a leg from an origin keyframe at an origin time to a destination with an arrival time; targets drawn).
- `queue: BTreeSet<(SimTime, EntityId)>`, each L2 agent's next event, and `unreconciled: BTreeSet<EntityId>`, the agents whose store differs from their governing building's level (those who chose a target at a building of the other level since the last reconciliation). Both are derived: rebuildable from the canonical columns, checked against their rebuild, never hashed. The second lets a tick reconcile without scanning every agent.

State classes (ADR 0017 §10): node levels, the L1 `SurfacePos`, `target`, `arrived`, `wait_steps`, `targets_drawn`, and the whole `Background` record are **canonical**; `EntityId`, `Resident`, and buildings are **identity / baseline**; a waiting agent's position and a promoted walker's position are **reconstructable**; `Walk::velocity`, the event queue, and the unreconciled index are **derived**. A demotion discards only reconstructable and derived state.

## Emitted event types

Domain plans expose actual `ArrivalFact`s (L1 completed boundary, L2 scheduled arrival time) and actual `LodChange`s. `ws-sim` translates them into history only after reserving publication storage. `prepare_tick` rolls back reconciliation on drop; `prepare_lod` changes nothing until commit. These plans have no dependency on `ws-events` and never read history. Low-level domain APIs remain available for domain tests; production `World` exposes only an immutable toy view.

## Invariants

- **Tier 1 identity and generation.** Building IDs are `derive(seed, "ws-world/id/building", [dataset_version, index])`, resident IDs `derive(seed, "ws-world/id/resident", [dataset_version, building_index, slot])`, both in the procedural namespace. Residents per building are `min + below(derive(seed, "ws-world/gen/resident-count", [dataset_version, index]), max - min + 1)`, 4 to 12 by default. All integer; golden values in `tests/golden/toy_world.txt` were produced by an independent Python implementation and must match on Linux and macOS CI.
- **Target choice.** An agent's `k`-th target is `next_target(seed, agent, k, buildings, current)`: draw `k` of its stream `Stream::new(seed, "ws-world/agent/next-target", [agent])`, mapped with `below` onto the buildings other than the current one, in ascending ID order. It depends only on those inputs, never on the tick of arrival, other agents, or other draws (`tests/world.rs`).
- **Movement (Tier 2).** Each step, in ascending ID order, an agent counts down its wait, draws its next target, or walks `speed × dt` straight to the target centre in its cell's ENU frame, arriving (snapped to the centre) when that is within one step. Each walking agent's position is then re-expressed, so after every step every agent lies in its own cell (`occupied_cell() == cell()`), and an agent that is not moving never changes cell. Agents stay within the buildings' bounding box.
- **Collisions fail.** Generation returns `WorldError::Duplicate` if two IDs collide; nothing is renumbered. `tests/collisions.rs` checks 10,000 seeds (450,813 IDs: 50,000 buildings and 400,813 residents) for collisions within and across worlds.
- **State hash.** `state_hash` is SipHash-2-4 over the seed, dataset version, walk parameters, and every component of every entity (IDs, integers, cell IDs, float bits), column by column in ascending ID order. It excludes storage indices, capacities, addresses, and derived values: since TP-007, `Walk::velocity` and the event queue (ADR 0017 §11). It covers node levels and every `Background` field. Stable within one build and platform (Tier 2). Worlds with the same entities hash equally however they were built.
- **Residency (ADR 0017 §3–§5).** Every resident is in exactly one of the L1 store (both `positions` and `walks`) and the L2 store (`background`), never both and never neither. Its `EntityId`, address, and home never change across transitions, and no transition draws a random number. After a transition and after a tick's reconciliation, an agent's level is its governing building's level (the building it waits at, or its destination while walking).
- **Transitions are transactions (§2, §12).** `set_lod(now, step, request)` takes a set of `(building, level)` pairs at a base-step boundary. It rejects `DuplicateNode` (even with the same level), `UnknownNode`, `UnsupportedLevel` (anything but L1 and L2), `OffGrid`, a reconstruction failure (`Coord`), and an invalid pre-state of any affected agent (`Invariant`), and then changes nothing. Otherwise it plans every move in ascending ID order and commits with batch column operations, so request order never matters. Demotion keeps an arrived agent's centre as `depart = now + (wait + 1)·Δ` and a walking agent's exact position as the leg keyframe with `arrive = now + ⌈remaining / step_m⌉·Δ`; promotion at the keyframe returns it bit-for-bit, and elsewhere places the agent at `m·step_m` along the straight leg, re-expressed into its own cell. L1 → L2 → L1 at one boundary is bit-exact, state hash included.
- **L2 advances by events (§7–§8).** `tick` reconciles residency at the start boundary, runs L1 movement, then processes every L2 event due by the end boundary in `(time, ID)` order, including events those events schedule. A departure draws the agent's next target with the same `next_target` counter L1 uses. Results do not depend on how a span is split into calls. A failed tick leaves the world unchanged.
- **Checker (§12).** `check_lod_invariants` reports every violation (`Duplicated`, `Lost`, `Orphan`, `PartialL1`, `IdentityMismatch`, `UnknownBuilding`, `MissingNodeLod`, `InvalidNodeLod`, `ResidencyMismatch`, `QueueMismatch`, `BadSchedule`, `NotAtCentre`, `NotInOwnCell`), and `ConservationSummary` compares population, identity, per-home and per-governing counts, and counters across a transaction or tick. Debug builds run both after every transaction and tick; release builds only on request. An arrived L1 agent off its target's centre is rejected on insertion.
- Integer simulation time comes in as `SimDuration` and `SimTime`; no wall clock, no threads, no `unsafe`, no unordered maps.

## Illness (TP-010)

`ToyWorld::illness()` is a column keyed by resident ID, independent of the L1 and L2 stores ([ADR 0021](../../../docs/adr/0021-god-intervention-contract.md) §3): `Illness { kind, severity, root_event }`, where `root_event` is the raw ID of the resident's `life.illness` event. It survives demote, promote, save and load by construction, is hashed only when non-empty (so worlds without illness hash as before) and is changed only by `afflict`. An ill agent draws no target: when its wait ends away from home it walks home, and at home it keeps waiting. L1 (`plan_movement`) and L2 (`process_event`) apply the same rule, so a level round trip stays exact (`ws-sim/tests/intervention.rs`). `residents_of(building)` is a derived index (rebuilt on load) so a target set resolves without scanning the world.

## Saving

`ToyWorld::to_parts` returns the canonical rows (`ToyParts`, ascending ID order) and `ToyWorld::from_parts` rebuilds a world from rows in any order, deriving the L2 event queue and the unreconciled-residency set. It rejects repeated IDs and residents whose home is missing; the caller runs `check_lod_invariants` for the rest (`World::from_parts` does). The file format is in `ws-persist`.

## Performance

About 96 bytes per L1 agent in the columns (16 `Resident` + 24 `SurfacePos` + 32 `Walk` + 3 × 8 ID copies), well inside the 0.5–1 KB L1 target of 09 §10.8, because TP-006 agents have no needs, plans, or perception yet. An L2 agent takes 64 bytes in `background` (56-byte `Background` plus the ID copy), 24 in `residents`, and a 16-byte queue entry plus `BTreeSet` node overhead (an estimate of 8 to 16 bytes per entry: leaves of 11 keys, at least half full), so 112 to 120 bytes against ADR 0017's 128 (`l2_records_fit_the_memory_target`). L2 is not smaller than L1 in the toy because the toy's L1 agent is already small (ADR 0017 §15). Step and transition costs are measured by `cargo bench -p ws-sim` (see the ws-sim README).

## Dependencies

`ws-ecs`, `ws-math`, `ws-rng`, `ws-time`.

### Movement frame reuse (#31, 2026-10-16)

On the same Apple M1 Pro Mac (arm64) with Rust 1.97.0, release `world_step/5000` measured **3.3330 ms before → 2.2909 ms after** (about 31% faster; after 95% interval 2.2826–2.3001 ms). `world_step/50` measured 35.443 → 23.904 µs. This reaches #31's approximate 2.4 ms target on this M1 Pro; the independent Linux Xeon result was 2.62–2.65 ms, so the target remains unmet on that reference machine. At ×10, an average 3.33 ticks per 30 fps frame costs about 7.64 ms; individual four-tick frames and device overhead still require measurement, so this is not an iPhone/frame-budget guarantee.

The L1 planner reuses one exact frame per occupied cell per step for target projection and re-expression; the full cell lookup and ADR 0016 edge rule remain unchanged. The ordered cache is temporary derived data and adds no component/save/hash state. It allocates by distinct moving cells; the all-L2 path returns before using it. See the [verification report](../../../docs/verification/issue-31/README.md) for full LOD comparisons, reproducible commands and direct baseline replay evidence. Older numbers above describe their original machines/revisions.

### Inside-cell fast path (#48, 2026-10-17)

The movement step's per-cell frames are built with `CellFrame::with_inside_test`, so an agent that stays well inside its cell skips the full cell lookup (see the ws-math README). Results are unchanged: the issue-31 replay probe (four seeds, all-L1 and mixed LOD, 80,000 tick rows) gives byte-identical output on `main` and with this change (SHA-256 `7a0d6e46…dce7b`, the same value Codex recorded on macOS for #31). On the Linux Xeon of the #31 review, `world_step/5000` is 2.65–2.79 ms on `main` and 0.35–0.38 ms with this change. The numbers and the frame budget are in the ws-sim README.
