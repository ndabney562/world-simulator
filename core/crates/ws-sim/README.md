# ws-world

## Responsibility

The world model. As of TP-006 (#7) it holds the Technical Prototype **toy world**: five box buildings at fixed offsets from the Bay Ridge reference point and a few dozen residents who walk between them ([ADR 0016](../../../docs/adr/0016-entity-identity-storage-and-randomness.md)). It proves identity, storage, randomness, movement across cell edges, and state hashing; it is not content. As of TP-007 (#8) residents run at one of two simulation levels, L1 (stepped) or L2 (a discrete-event background schedule), chosen per building ([ADR 0017](../../../docs/adr/0017-simulation-lod-l1-l2-round-trip.md)). VS-002 (#66) adds the real block structural baseline described below; the world tree and aggregates arrive later.

| Module | Provides |
|---|---|
| `slice` | `SliceWorld`, fixed-point source geometry, four detailed buildings and the 58-footprint context baseline (VS-002) |
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

## Real block baseline (VS-002)

`slice::SliceWorld::new(seed)` builds the four detailed buildings selected in
[ADR 0023](../../../docs/adr/0023-vertical-slice-block.md). Its immutable
`buildings()` column iterates in ascending `EntityId` order. Every row retains
BIN, BBL, lot, address, PLUTO use class, lot-level floors and residential units,
roof height above ground, source ground elevation, centroid and full footprint.
`slice::BLOCK_BUILDINGS` retains all 58 source footprints (including 6 garages)
for surrounding context; only the four `featured` rows become entities here.
No synthetic residents or events are emitted by this structural constructor.

The reviewed [VS-001 extract](../../../data/vertical-slice/bay-ridge-block-6049/README.md)
is compiled offline by `scripts/vs_core_buildings.py` into `src/block_6049.rs`.
This small adapter only reads the normalized extract, never the raw upstream
snapshots. It has no runtime I/O, new dependencies, simplification, projection,
or generic simulation-tile pipeline. Refresh with `python3 scripts/vs_core_buildings.py build`
and verify with `python3 scripts/vs_core_buildings.py check` from the repo root.
The existing CI Python discovery checks exact regeneration for every source row.

**Exact geometry (Tier 1):** coordinates retain all twelve decimal places as
signed picodegrees; roof heights and original NAVD88 ground elevations use
integer millimetres. MultiPolygon parts, holes, winding, source vertex order,
and closing vertices are retained. The centroid retains the extract's six
places (about 0.1 m); it is not recomputed from a simplified box. Validation
checks coordinate range, closure, minimum ring length, nonzero integer area,
and positive height; it does not claim general polygon-topology validation.

**Coordinates and vertical datum:** `GeoPoint::surface_position(ellipsoidal_height_mm)`
projects to ADR 0015's `SurfacePos` through portable `ws-math`. The caller must
explicitly supply WGS84 ellipsoidal ground height. Passing zero means the
ellipsoid, not real street elevation. The source `ground_navd88_mm` is retained
as NAVD88 metadata and is never implicitly treated as ellipsoidal; terrain and
a geoid conversion are outside VS-002 (ADR 0023). Roof height remains relative
to ground and does not get added to the footprint location.

**Identity and baseline:** `baseline()` retains seed, dataset version
`0x85183751d4f793b9` and structural generator version 1 for the later save header.
The dataset version is the first 64 bits (big-endian) of the exact GeoJSON's
SHA-256, whose full value is exported as `EXTRACT_SHA256`. Building IDs reuse
ADR 0016's existing purpose and integer address `[dataset_version, BIN]`;
BIN replaces the toy's ordinal, so source order cannot renumber buildings.
No float, address string, centroid, or runtime allocation enters ID derivation.
A changed extract gives a new version; migration between dataset versions is
not currently supported. Later slice persistence must pin/validate its version
before regenerating (#53 / #75); no codec or older save fixture changes here.

**Collisions (#30):** duplicate BINs are rejected before hashing, and duplicate
entity IDs fail the whole constructor with `SliceError::Collision`. There is no
silent overwrite, salt, renumbering, or chosen resolution policy. Unit tests
inject a collision between distinct addresses. The existing City-stage ADR
follow-up remains open; it is not a dependency of a four-building slice.

**Verification:** `tests/golden/block_6049.txt` freezes IDs, integer centroids,
heights and use for seeds 0, 42 and `u64::MAX`; the ID oracle was an independent
Python SipHash-2-4 implementation checked against every existing toy building
golden. Both Rust CI platforms run it. New tests also cover exact extract
regeneration, nested geometry/closure, rejected malformed data, source-order
independence, 10,000 seeds (40,000 building IDs), and fixed-seed properties for
`SurfacePos` containment, idempotence and ECEF error below 0.1 mm at explicit
ellipsoidal heights. Existing toy goldens remain unchanged.

This constructor owns only structural baseline data. `ws-sim::World`, the CLI,
save formats 1/2 and Unity ABI4 continues using the toy world. Resident domain
integration belongs to #67; real-footprint snapshots and rendering belong to
#72, and slice save/migration support to #53 / #75. No ADR decision changes
are needed: real-data building keys are already allowed by ADR 0016.

Load time is negligible at 58 validated source rows and four inserted entities
(#66); there is no tick-loop change and no separate performance measurement.


## Synthetic block population (VS-003)

`ws_world::slice::population::PopulatedSlice::new(seed)` wraps the validated real-building `SliceWorld` with an immutable synthetic population. The original building-only constructor and all toy constructors are unchanged. No CLI, save, FFI, schedules, movement or Unity switch-over is included; those belong to subsequent slice issues.

The four selected source classes/residential-unit counts (S1=1, A5=1, B2=2, C2=6) give exactly ten occupied homes. One household is generated per home; each gets 1–3 people from its own counter-based stream, guaranteeing 10–30 people for every seed. The first member is an adult aged 22–80; other members are 0–90. This is a small explicit synthetic policy, not a Census-calibrated demographic model or a claim about actual occupants. Ages 0–4 are preschool, 5–21 students, 22–64 employed, 65–90 retired. Names come from small hand-authored tables; no real-person dataset is used. Household members share a generated family name.

Household IDs use `ws-world/id/household` with `[dataset_version, BIN, unit]`. Resident IDs reuse ADR 0016's purpose/address, with BIN as building key and flat slot `unit*3 + member_slot`. Workplace IDs use `ws-world/id/workplace` with `[dataset_version, BIN, 0]` for the shop and `[dataset_version, 0, 0|1]` for off-map work/school. New IDs are checked against **every** existing building/household/resident/workplace ID; any collision fails without renumbering or a partial result. Adoption of ADR 0024's City collision policy remains a separate implementation, not a silent change to these generators.

Random attributes use separate `Stream(seed, purpose, [stable_id]).draw(0)` choices and integer `below`; source order and unrelated draws cannot shift identities. The immutable columns iterate by ascending ID. The shop's synthetic job capacity comes from its floor count (clamped 1–3). Canonical lowest-ID employed residents fill that capacity; other employed residents use an off-map employment destination, and all students use an off-map school. Preschool/retired residents have no workplace. Every resident has exactly one home and household.

Two exact-integer synthetic gateways sit on the west/east edges of the pinned extract's footprint extent, halfway between its north/south extremes. They are coarse map exits, **not surveyed sidewalks, road intersections or converted ground elevations**. Employment/school records reference their exit index; no resident disappears simply because the destination is off-map. VS-005 supplies the actual sidewalk graph.

`PopulationBaseline` retains the unchanged structural seed/dataset/loader version and a separate population generator version (1). Changes to address mapping, size/age/assignment policy or name tables require a population-version bump and normal golden review. Existing building/toy Tier 1 fixtures are unchanged. An independent Python SipHash oracle, first checked against every existing real-building ID golden, freezes all household/resident/workplace IDs, names/ages/roles, memberships, assignments and gateway coordinates for seeds 0, 42 and u64::MAX. Rust compares the entire canonical output on both CI platforms. Regenerate only after an authorized version change:

```sh
python3 scripts/vs_population_golden.py check
# After version/fixture review approval only:
python3 scripts/vs_population_golden.py build
```

Fixed-seed property tests verify the home/unit/member counts, adult head, exact role ranges, cross-kind unique IDs, accepted resident address derivation, family/name attributes, destination/capacity references and regeneration. Unit tests force cross-kind collisions, reverse source order, reject changed source-home counts and verify purpose separation. No tick-path cost is added; measurements are unnecessary at this baseline's scale per #67. No new dependency or data source.

## VS-007 derived calendar/sky

`environment` composes exact UTC, bounded New York policy1 and pure solar rule1. `SOLAR_SITE` is computed at compile time from the complete58-source-footprint envelope with i128 midpoint/floor arithmetic; the pinned registered dataset supplies its version. `calendar_sky(time, render_basis)` returns exact UTC throughout SimTime and explicit unavailable local/sun status outside UTC2000–2099. The sun is rotated site ENU→ECEF→the supplied render ENU basis; azimuth/elevation remain fixed-site values. `ToyWorld::render_basis` returns precisely the basis already used by render records, without treating navigation millimetres as ENU. The dedicated query direction has new exact Tier1 goldens across four seeds; general coordinate policy stays Tier3. This owns no mutable state/events and consumes no RNG.

## VS-007 committed calendar/sky

`World::calendar_sky()` returns one immutable boundary: tick, SimTime, canonical state hash, committed scale, exact UTC and optional core-derived New York/sun output in the paired render basis. It does not flush pending commands, tick, consume RNG or retain derived state. `SliceConfig::calendar_aligned_v1` constructs an explicit stored model-week epoch using NY policy1; `config()` queries it without recomputing alignment. Raw configuration remains compatible and all old schedule/behavior rules/goldens remain unchanged; elapsed model weeks stay uniform across later DST/leap boundaries. New tests compare calendar/state/checkpoint output at every script boundary in separate processes with pause/1/10/100 scales, and query-independence tests preserve pending commands. [ADR0030](../../../docs/adr/0030-calendar-sun-and-sky-query.md); [verification](../../../docs/verification/issue-71/README.md).
