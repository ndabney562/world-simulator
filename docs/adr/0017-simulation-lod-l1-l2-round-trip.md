# ADR 0017: Simulation LOD: the L1 ↔ L2 round trip (D-10, TP-007)

- Status: Accepted; toy-world parts superseded by [0040](0040-delete-toy-world-and-old-formats.md)
- Date: 2026-10-15
- Decided by: Noah (Product Owner), on Claude's proposal for TP-007; reviewed by ChatGPT
- Issue: #8 (TP-007)
- Supersedes: none. Details D-10 of [21-decisions-now-vs-later.md](../architecture/21-decisions-now-vs-later.md) for the first executable pair of levels, and settles the `WorldConfig::start` alignment question that the TP-004 review (PR #21) left to TP-007. ADRs 0014, 0015, and 0016 are unchanged.

## Context

Architecture V1 makes the aggregation / disaggregation contract "the backbone of the simulation" ([05-simulation-lod.md](../architecture/05-simulation-lod.md) §6.3): every pair of adjacent levels needs an aggregation `A`, a disaggregation `D`, conservation invariants, and an equivalence tolerance. TP-007 (#8) is the first time the core has two levels. Before any code is written, this ADR fixes what L1 and L2 mean for the toy world of TP-006, the two transitions, their place in the tick, what is conserved exactly and what only within a tolerance, and the tests the implementation must pass, so that the implementation PR does not invent architecture while coding.

### What exists on `main` (2a24bfe)

- **Time** ([ADR 0014](0014-time-representation.md)): `SimTime` is `i64` microseconds; a `Rate`'s tick boundaries are aligned to the epoch. `ws-sim`'s `Clock` advances in fixed 100 ms base steps from `WorldConfig::start`, which may be any instant, so a start that is not a whole number of base steps from the epoch puts every clock step off the `Rate` grid. The `ws-sim` README and the `WorldConfig::start` doc comment defer that to TP-007.
- **Identity, storage, randomness** ([ADR 0016](0016-entity-identity-storage-and-randomness.md)): `EntityId` (procedural or spawned namespace); `ws-ecs` sorted columns iterated in ascending ID order, with `insert_batch` (all or nothing) and `remove_batch` (silently ignores absent IDs); SipHash-2-4 counter streams in `ws-rng`; `SurfacePos::reexpress` and `from_ecef` always return a position inside its own cell.
- **The toy world** (`ws-world`, TP-006): five box buildings; residents with three columns: `Resident { address, home }` (identity), `SurfacePos`, and `Walk { velocity, target, arrived, wait_steps, targets_drawn }`. Each 100 ms step, in ascending ID order, an agent waiting with `wait_steps > 0` counts down; one waiting with `wait_steps == 0` draws target number `targets_drawn` with `next_target(seed, id, k, buildings, current)` and starts walking in the same step; a walking agent moves `speed × Δ` (0.14 m) straight at the target's centre and, when the remaining distance is at most one step, snaps to the centre, sets `arrived` and `wait_steps = 50`. Positions are re-expressed after every step. `velocity` is written by the step but read by nothing. `state_hash` covers every column, `velocity` included.
- **Clock and world** (`ws-sim`): `World::step` advances the clock one base step and runs the movement system; a failed step leaves the world unchanged.

### Measured: how closely an analytic leg model follows L1 walking

A throwaway probe (not committed) ran the TP-006 movement system on `main` at `2a24bfe` for 4 seeds × 1,000 agents (`Population::exactly(200)`) × 1 simulated hour (36,000 steps) and compared every walk with the analytic model of §5 below (a straight line in ECEF from the leg's origin at `speed × Δ` per step, arriving after `n = max(1, ⌈distance / (speed × Δ)⌉)` steps):

| Result | Value |
|---|---|
| Legs that started at a building | 5,854 |
| of which the analytic arrival step equals the L1 arrival step | 5,854 (all) |
| Largest distance between the L1 position and the analytic position on the same step | 47.5 mm |
| Predictions of the remaining step count made from an L1 agent's current position in mid-leg (one every 7 steps) | 3,793,573 |
| of which exact | 3,740,507 (98.6%) |
| of which L1 arrived one step earlier than predicted | 53,066 (1.4%); never later, never by more than one step |
| Analytic positions, turned into a `SurfacePos` with `from_ecef`, that lie outside their own cell | 0 |

The mid-leg disagreements come from rounding that L1 accumulates on the way (`f32` offsets, re-expression at cell edges): the remaining distance measured from the actual position differs from the ideal one by up to a few centimetres, which moves `⌈rem / step⌉` across an integer in about 1.4% of samples. Legs that start at a building agree exactly because both models start from the same `SurfacePos` and the error never grows to a whole step.

## Options considered

### What an L2 agent keeps

1. **The L1 components unchanged, in another column** ("L2" that is only a different container). Exact, but L2 would still need per-step integration to advance, so it would cost what L1 costs. Rejected: it proves nothing about LOD.
2. **Location at building granularity only** (the issue's sketch: drop position and velocity, keep the building, the activity, and the next event time). Cheap, but an agent demoted mid-street would reappear at a building or at a reconstructed point, so a zero-time round trip would move it. That is information loss Architecture V1 does not permit for state the player can see ([05](../architecture/05-simulation-lod.md) §6.7).
3. **A schedule of legs and waits with one exact keyframe** (chosen): an L2 agent is a `Waiting` or `Walking` record whose `Walking` leg keeps its origin, its origin time, its destination, and its integer arrival time. The origin is a building ID whenever the leg started at a building and an exact `SurfacePos` only for the remainder of a leg during which the agent was demoted. L2 advances only at discrete events (an arrival, a departure); position is a function of the leg and the time, evaluated only when materializing. A zero-time round trip is exact, and after the agent's next arrival its L2 state is at building granularity again.

### Which node governs an agent's level

1. **Per-agent levels with no nodes.** Simplest storage, but Architecture V1 assigns levels to nodes ([05](../architecture/05-simulation-lod.md) §6.2), and the issue asks for per-node levels.
2. **Sector cells as nodes.** The architecture's eventual unit of materialization, but an L2 walker's cell changes mid-leg, so membership changes would need analytic edge-crossing times: too much for TP-007, and the toy's five buildings would share two or three cells, so cell-level transitions would be nearly all-or-nothing.
3. **Buildings as nodes, an agent governed by its destination building** (chosen): the building it is waiting at, or the one it is walking to. This is the rule of [12-materialization.md](../architecture/12-materialization.md) §13.6 and [05](../architecture/05-simulation-lod.md) §6.6 ("demoted to L2 with a scheduled arrival in the destination"), needs no new L1 state, and changes only at departures, which are discrete. A walking agent's origin building is not stored at L1, so an origin-governed rule would have needed a new field.

### Where agents' levels are stored

1. **A `Lod` component on every agent plus one set of state columns.** The level would be a second copy of information already implied by which components exist, and could disagree with them.
2. **Mutually exclusive state stores, identity stored once** (chosen): the `Resident` identity column holds every agent at every level; an agent's L1 state is its entries in the `positions` and `walks` columns, and its L2 state is its entry in a new `background` column. Residency is *which store holds the ID*; the invariant checker verifies that exactly one does.

### `WorldConfig::start` alignment

1. **Leave it unaligned and define all grids relative to `start`.** Contradicts ADR 0014 §6 (tick indices aligned to the epoch, so rates with commensurate steps share boundaries) and would make the L2 event grid differ between worlds.
2. **Round `start` down to the grid.** Silently changes a host-supplied input.
3. **Reject an unaligned start with a typed error** (chosen). The host aligns its own input with `Rate::tick_start(Rate::tick_index(t))` if it wants the floor.

## Decision

Notation: `Δ` is the world's base step (100 ms), fixed for the life of a world. Tick boundary `j` is `t_j = start + j·Δ`; step `j` takes the state at `t_{j−1}` to the state at `t_j`. `step_m = speed_m_s × Δ_s` is computed exactly as the L1 movement system computes it. `centre(B)` is building `B`'s `SurfacePos`.

### 1. What L1 and L2 mean

**L1 Active** (unchanged from TP-006): an individually represented agent with its `EntityId`, its identity component, an explicit `SurfacePos`, and its behavioural state `(target, arrived, wait_steps, targets_drawn)`, advanced every base step by the movement system.

**L2 Background**: the same individual, with the same `EntityId` and identity component, represented as a schedule and advanced only at discrete events:

```
Background {
    targets_drawn: u32,          // the next-target stream counter, exactly as at L1
    activity: Activity,
}
Activity =
    Waiting { at: EntityId,      // building; the agent stands at centre(at)
              depart: SimTime }  // the step in which it draws its next target
  | Walking { origin: Origin,    // where the leg started
              origin_time: SimTime, // tick boundary at which the agent was at the origin
              dest: EntityId,    // building it is walking to
              arrive: SimTime }  // the step in which it arrives
Origin = Building(EntityId)      // centre of that building
       | Point(SurfacePos)       // exact position at a mid-leg demotion
```

The agent's **next event time** is `depart` or `arrive`. All four times lie on the base-step grid (§8).

What happens to each piece of L1 state:

| L1 state | At L2 | Class (§10) |
|---|---|---|
| `EntityId`, `Resident { address, home }` | Retained in the identity column, untouched by any transition | Canonical identity (procedural: also regenerable from the address) |
| `targets_drawn` | Retained exactly | Canonical |
| `target` | Retained as `at` (waiting) or `dest` (walking) | Canonical |
| `arrived` | Retained as the activity variant | Canonical |
| `wait_steps` | Folded into `depart = T + (wait_steps + 1)·Δ`; recovered exactly | Canonical (as a time) |
| `SurfacePos` of a waiting agent | Discarded; it is `centre(at)` by the L1 invariant in §6 | Reconstructable exactly |
| `SurfacePos` of a walking agent | Folded into the leg (`Point` origin and `origin_time = T`); the position at any later boundary is a function of the leg | Reconstructable exactly at `T`, by the L2 model afterwards |
| `velocity` | Discarded | Derived (§10): no system reads it |
| Per-step progress | Not represented; the leg's integer `arrive` replaces it | Derived |

L2 introduces **no stored aggregate**. Per-building and per-cell counts are computed from the individual records whenever they are needed, so no cached copy can disagree with them. The first stored aggregates (occupancy counts) belong to L3 (§16).

### 2. The two transitions

`demote` (L1 → L2, the toy's `A`) and `promote` (L2 → L1, the toy's `D`) are functions of one agent's state at one tick boundary `T`. Neither draws a random number, reads another agent, or changes the identity column.

**`demote(id, T)`**, given L1 `(pos, walk)`:

- If `walk.arrived`: `Waiting { at: walk.target, depart: T + (walk.wait_steps + 1)·Δ }`. Requires `pos == centre(walk.target)` bit for bit.
- Otherwise: `rem = |ecef(centre(walk.target)) − ecef(pos)|` (f64), `n = max(1, ⌈rem / step_m⌉)`, and `Walking { origin: Point(pos), origin_time: T, dest: walk.target, arrive: T + n·Δ }`.
- `targets_drawn` is copied.

**`promote(id, T)`**, given L2 `Background`:

- `Waiting { at, depart }`: `pos = centre(at)`, `walk = { target: at, arrived: true, wait_steps: (depart − T)/Δ − 1, velocity: 0 }`. Requires `depart − T` to be a positive multiple of `Δ`.
- `Walking { origin, origin_time, dest, arrive }`: `m = (T − origin_time)/Δ`, an exact integer division (§8). If `m == 0`, `pos` is the origin, bit for bit (`Point(p)` gives `p`, `Building(b)` gives `centre(b)`). Otherwise, with `O = ecef(origin)`, `G = ecef(centre(dest))`, `u = (G − O)/|G − O|`: `pos = SurfacePos::from_ecef(O + u·(m·step_m))`, which always lies in its own cell (ADR 0016 §4). `walk = { target: dest, arrived: false, wait_steps: 0, velocity: speed·u in pos's cell frame }`. Requires `origin_time ≤ T < arrive`. Because `arrive` was computed as `n` steps with `n·step_m ≥ |G − O|` and `m < n`, the point never overshoots the destination.
- `targets_drawn` is copied.

**Inputs and outputs of a transition request.** A request is a set of `(building ID, target level)` pairs, applied as one transaction at the current tick boundary:

1. **Validate** (no mutation): every building exists; no building appears twice in the request (`DuplicateNode`, even with the same level); every level is `L1` or `L2` (`UnsupportedLevel` otherwise); the pre-state of every affected agent passes the per-entity checks of §12.
2. **Set node levels** for the listed buildings. A building already at its requested level is a no-op, not an error, so an interest manager can restate its decisions every update.
3. **Reconcile** (§4, item 3): every agent whose residency differs from the level of its governing building is selected, in ascending ID order, and the new state of each is computed with `demote` or `promote`. Any error aborts the whole transaction.
4. **Commit**: `remove_batch` and `insert_batch` on the affected columns, node levels written, event-queue entries removed and inserted. The plan has already established every precondition of these calls (each ID is present in exactly the store it leaves and absent from the one it joins), so the commit cannot fail; a failure there is a bug and panics with the name of the violated invariant (as integer overflow does).
5. **Check** (debug builds and every test): the full invariant checker of §12, and the conservation comparison of §5 between the pre- and post-transaction summaries.

The result depends only on the set of pairs and the world state: request order, the order agents were inserted in, and the order of columns in memory cannot change it.

**Who requests transitions.** In TP-007 a transition is a plain call on the world, `World::set_lod(request)` (TP-006 style; the name is not binding), between ticks. TP-008 turns it into a journaled command with the same payload, applied at the same point. The movement-driven part (§4, item 3) is an internal deterministic decision. The future interest manager ([05](../architecture/05-simulation-lod.md) §6.4) is a simulation system that emits the same request type from simulation-relevant inputs only: the view state arrives as a journaled host input (`set_view`), quantised to integers (cell IDs, distance bands) before it influences a decision, never read from Unity's camera, the renderer, or a visual provider. Hysteresis and budgets live in the manager, not in the transition.

**Forbidden inputs.** No transition, and no part of L2, depends on hash-map iteration order, thread timing, the wall clock, rendering or camera state, visual providers, or memory addresses. Every set is a sorted `ws-ecs` column or a `BTreeSet`/`BTreeMap`.

### 3. Storage

- **Identity is stored once.** The `residents: Column<Resident>` column holds every agent at every level and is never touched by a transition.
- **Mutually exclusive state stores.** L1 is the pair of columns `positions: Column<SurfacePos>` and `walks: Column<Walk>` (existing); L2 is one new column `background: Column<Background>`. An agent's residency is the store that holds its ID; there is no separate residency component.
- **Node levels**: a new column `node_lod: Column<Lod>` keyed by building ID. `Lod` is a `u8`-backed enum with the eight Architecture V1 levels (`L0 = 0` … `L7 = 7`), of which TP-007 accepts only `L1` and `L2`. Every building starts at `L1`, so a new world behaves as in TP-006.
- **The L2 event queue** is a `BTreeSet<(SimTime, EntityId)>` holding exactly one entry per L2 agent, at its next event time. It is a derived index of the `background` column: it is not hashed, it is rebuilt from the column on load (TP-009), and the checker verifies that it equals that rebuild.
- **Batch operations only.** Transitions move agents between stores with `remove_batch` and `insert_batch` (O(n + m) per column); per-agent `insert`/`remove` loops (O(n·m)) are not allowed in transitions. Because `remove_batch` ignores absent IDs, the validate step proves every ID is present before the commit.
- **No new `ws-ecs` primitive** is needed: columns, batch insert and remove, and `join` suffice. No general ECS framework, archetypes, scheduler, or threads are introduced.
- **Crate placement**: the L2 types, transitions, queue, and checker live in `ws-world`, next to the toy world whose components they convert (the issue allows this or a `ws-lod` crate). A domain-layer `ws-lod` crate could not be depended on by `ws-world`, and a shared LOD abstraction would be premature with one domain. When several domains need the A/D contract and the interest manager arrives, `ws-lod` is created then, with shared level types in `ws-world`. `ws-sim` owns only the phase order (§8).
- **Memory target**: at most 64 bytes per agent in the `background` column (ID copy included; `size_of` asserted in a test as TP-006 does), and at most 128 bytes per L2 agent in total including its identity entry and queue entry (the upper end of 09 §10.8).

### 4. Identity preservation

1. **The same ID at every level.** An agent that goes L1 → L2 → L1 (any number of times, with or without elapsed time) keeps the same `EntityId`. No transition calls an ID derivation function, allocates a spawned ID, or writes the identity column.
2. **The generation address survives.** For a procedural resident, `Resident.address` stays in the identity column, so `resident_id(seed, dataset_version, address) == id` holds at every level; the checker verifies it for every procedural resident (§12).
3. **Governing node.** An agent's governing building is `walk.target` at L1 and `at` or `dest` at L2: the building it is waiting at or walking to. After every transaction and at the start of every tick (§8), each agent's residency equals its governing building's level. An agent that departs during a step towards a building at another level keeps its residency until the next reconciliation, at most one step, and is then demoted or promoted with its exact position (a `Point` origin when demoted, `m ≥ 1` when promoted).
4. **Spawned entities** (none in TP-007) use exactly the same stores and transitions; nothing in this ADR reads the namespace bits. A spawned entity has no generation address, so it can never be regenerated: it is canonical from creation, and later levels that discard anonymous individuals (L3+) must never discard it. Its identity record (TP-008: the spawn event that created it) stays in the identity store at every level.
5. **Future overrides.** The required residency of an agent is a function `required(agent) = level(governing node)`. Notables pinned to a minimum level, or a possessed or selected agent pinned to L1/L0, become `min_finer(level(node), pin(agent))` later without changing the transaction.

### 5. Conservation

A **conservation summary** is computed in canonical order before and after every transaction in tests and debug builds:

| Quantity | Rule across a transaction (any `T`) | Rule across L1 → L2 → L1 at the same `T` |
|---|---|---|
| Population: `|L1| + |L2|` | Exact, and equal to `|residents|` | Exact |
| ID set: `ids(L1) ⊎ ids(L2) == ids(residents)` (disjoint union) | Exact; no ID in both stores, none in neither | Exact |
| Identity column (address, home) | Exact: untouched | Exact |
| Per-home counts | Exact | Exact |
| Per-governing-building counts | Exact (transitions do not change targets) | Exact |
| `targets_drawn` per agent | Exact (transitions never draw) | Exact |
| Next-event / behavioural progress | `wait_steps` ↔ `depart` exact; a walker's leg is re-planned from its exact position | Exact |
| Positions | Exact for waiting agents and for walkers promoted at `m == 0` | Bit-exact |
| Per-cell counts | Exact, positions being exact at the boundary | Exact |
| Simulation time, tick, clock | Unchanged by a transition | Unchanged |
| Node levels | Changed only as requested | Restored |
| `velocity` | Not conserved: re-derived | Re-derived (excluded from the hash, §11) |

Exact means bit-for-bit equality of canonical state. **L1 → L2 → L1 at the same boundary must leave the canonical state hash unchanged.**

The reverse round trip, **L2 → L1 → L2 at the same boundary**, is exact for waiting agents. A walking agent is re-based: its leg becomes `Point(promoted position)` with `origin_time = T`, and its `arrive` is recomputed from the remaining distance, which may differ from the previous `arrive` by one step (the probe saw this on 1.4% of mid-leg re-plans, always earlier). Its destination, `targets_drawn`, and position at `T` are exact. This is the one place where a round trip is bounded rather than exact, and it affects timing only, never identity, counts, or random choices.

**Equivalence over time** (L2 advanced, then promoted, against the same agents kept at L1; 05 §6.3 item 4): the sequence of target buildings is identical (§9); a leg that starts at a building arrives at the same step at both levels (expected always, from the probe; Tier 2, so a difference of at most one step is the tolerance); each re-plan of a leg from a mid-leg position may shift that leg's arrival, and every later event of that agent, by at most one step; positions differ by at most 0.1 m when the two are on the same leg and step (probe: 47.5 mm). Population, identity, and counts are never toleranced.

The issue's acceptance criterion `A(D(x)) == x` for counted quantities is the zero-time row above: the set of agent IDs and per-building counts are identical across L1 → L2 → L1.

### 6. Position semantics

- **A waiting L2 agent has no stored position.** It is at `centre(at)` by definition, and promotion writes exactly that `SurfacePos`. TP-007 makes this an L1 invariant too: an L1 agent with `arrived == true` stands exactly at `centre(target)` (the movement system already snaps it there; `insert_resident_state` starts rejecting other combinations, and `demote` rejects them as an invariant violation).
- **A walking L2 agent stores where its leg started, not where it is.** The position at boundary `T` is the analytic point of §2, evaluated only when materializing. The leg's origin is a building for every leg that starts at L2, and an exact `Point` only for the leg during which the agent was demoted.
- **No seeded scatter in TP-007.** The issue sketches positions "re-derived from `(seed, agent ID, building, tick)`". TP-006's L1 agents wait exactly at the building centre, so seeded placement would make a zero-time round trip move them. When L1 agents later occupy places inside buildings, a materialized placement will be drawn from a stream keyed by `(seed, "ws-world/lod/placement", [agent, building, visit])`, where `visit` is the arrival count (equal to `targets_drawn` in the toy), never the tick, so the same visit always gives the same spot.
- **Error bound.** A promoted walker's position is within 0.1 m of the position an always-L1 agent would have on the same leg and step (Tier 2/3; probe: 47.5 mm), and is the same at every boundary whatever the transition history, given the same leg.
- **Cell boundaries.** Building centres and `Point` origins are self-consistent `SurfacePos` values (built by `from_ecef` or `reexpress`), promoted positions come from `from_ecef`, and `m == 0` returns the stored origin bit for bit. So every materialized agent lies in its own cell (`cell() == occupied_cell()`), a waiting agent's cell never changes however many round trips it makes, and cycling a walker every tick changes its cell only where its straight leg crosses an edge (no back-and-forth). L2 stores no cell; an L2 agent's cell, where needed, is the cell of its analytic position.

The trade-off: storing a `Point` for one leg costs 24 bytes in the walking variant but makes every zero-time round trip exact and keeps the player from catching an agent jumping. Dropping it would save little and create the visible loss 05 §6.7 forbids. Storing an exact position for every L2 agent at every step would be option 1 above.

### 7. Background behaviour

L2 runs the TP-006 behaviour as a discrete-event model. Two event kinds exist, both processed in `(time, entity ID)` order:

- **Depart** at `τ = depart` (Waiting): draw target number `k = targets_drawn` with the TP-006 `next_target(seed, id, k, buildings, at)` (§9), set `targets_drawn = k + 1` (checked; overflow is a typed error), and start `Walking { origin: Building(at), origin_time: τ − Δ, dest, arrive: τ − Δ + n·Δ }` with `n = max(1, ⌈|G − O| / step_m⌉)`. This matches L1, which draws and takes its first step in the same step.
- **Arrive** at `τ = arrive` (Walking): become `Waiting { at: dest, depart: τ + (W + 1)·Δ }`, where `W` is `WalkParams::wait_steps`. This matches L1, which snaps on arrival, counts `W` down over the next `W` steps, and departs in the step after.

A one-step leg (`n = 1`) arrives in its departure step, so an agent can have two events at the same `τ` (Depart, then Arrive); every other event is scheduled strictly later. **Draining to a horizon `t`** pops the smallest `(τ, id)` with `τ ≤ t`, processes it, inserts the agent's next event, and repeats until the head is later than `t`. It always terminates, because each agent has at most two events per `τ`.

The cases the issue lists:

| Situation while at L2 | Behaviour |
|---|---|
| Reaches the destination | Arrive event at the leg's integer `arrive` step |
| Waits there | `Waiting` until `depart`; nothing is processed in between |
| Chooses another destination | Depart event: the next draw of its own stream |
| Crosses several arrivals, waits, and departures in one long advance | Each is a separate event in `(τ, id)` order; the result equals draining one tick at a time (cadence independence, below) |
| Materializes mid-trip | `promote` at the boundary: the analytic point on the current leg (§2, §6) |
| Its destination's node becomes L1 | It is promoted at the next reconciliation, at its exact analytic position |

**Cadence independence.** Draining is a function of the event times only: for `t1 ≤ t2`, draining to `t1` and then to `t2` gives the same state as draining straight to `t2`. Host frames, `advance_by_host_elapsed` call splitting, and time scale therefore cannot change L2 results. In TP-007 the world drains every step (§8), because a departure towards an L1 building must be reconciled at the next boundary; lazily draining frozen nodes ("simulate on read", 05 §6.5) is the same function and becomes available when a node provably has no finer neighbours.

**Why L2 is cheaper.** An L1 agent costs one movement step (about 0.8 µs, mostly re-expression) every 100 ms: 36,000 per simulated hour. An L2 agent costs nothing between events, and the probe saw about 7.3 legs per agent-hour, so about 15 events per agent-hour. §15 turns this into targets.

### 8. Time and the canonical tick order

**Start alignment (resolves the TP-004 caveat).** `World::new` rejects a `WorldConfig::start` that is not a tick boundary of the base step (`!base_step.is_tick_start(start)`) with a typed error; it never rounds. So every clock time is a `Rate` boundary of the base step and of every rate whose step is a multiple of it, and every L2 event time is on the same grid. The doc comment on `WorldConfig::start`, the `ws-sim` README invariant, and the `ws-sim` test that starts at `-7 µs` change accordingly in the implementation PR.

**Grid rule.** Every L2 time (`depart`, `origin_time`, `arrive`) is `start + k·Δ` for an integer `k`. Transitions happen only at tick boundaries, so `T − origin_time` and `depart − T` are exact multiples of `Δ`; a time off the grid is an invariant violation. L2 has no coarse step of its own: it is event-driven on the base-step grid. A future coarse rate (hourly L3 updates) must have a step that is a multiple of `Δ`.

**Materialization at arbitrary timestamps** is not permitted. The L2 position function is defined for any grid time at or after the leg's origin, which is what simulate-on-read will use, but promotion happens only at the world's current boundary.

**One canonical order per tick.** Step `j` (from `t_{j−1}` to `t_j`) runs:

1. *(TP-008: journaled commands for boundary `t_{j−1}`, in journal order.)*
2. **Reconcile** at `t_{j−1}`: agents whose residency differs from their governing building's level are transitioned in one transaction (§2, step 3).
3. **L1 movement** for every L1 agent, in ascending ID order (TP-006, unchanged).
4. **L2 drain** to `t_j`: every event with `τ ≤ t_j`, in `(τ, id)` order.
5. **Clock** advances to `t_j`.
6. *(Debug builds and tests: the invariant checker.)*

The step is atomic: if any phase fails, the world, clock included, is unchanged.

**Requests between ticks.** `World::set_lod(request)` is applied immediately at the current boundary `t_now` as one transaction, including its own reconciliation, before the next step's phase 2. Several calls at the same boundary apply in call order (journal order in TP-008). This is what makes a zero-time round trip possible: `set_lod(B → L2)` then `set_lod(B → L1)` at the same `t_now`.

### 9. Randomness across levels

**Invariant: changing level never changes an entity's future random sequence.**

- The only random decision in the toy is the next target. Its `k`-th value is `next_target(seed, id, k, buildings, current)`, a Tier 1 function of the seed, the ID, the counter `k`, the sorted building set, and the building the agent is at. It does not depend on the time, the tick, the level, or the order agents are processed in.
- `targets_drawn` is carried exactly by both transitions and advanced only by a departure (L1 movement step or L2 Depart event), by exactly one per departure. A transition never draws, skips, or resets a counter.
- When an L2 drain passes several departures, each consumes the next counter value in order, so an L2 agent makes exactly the draws an L1 agent would make for the same departures. Timing differences (§5) can shift *when* the `k`-th draw happens, never *what* it is.
- Rule for future systems: every per-entity stream is keyed by `(seed, purpose, [entity ID, …])` and indexed by an event counter that is part of the entity's canonical state at every level that can make that decision. A per-tick stream (`[entity_id, tick_index, k]`, ADR 0016 §3) is allowed only for decisions that exist at one level alone; a decision that both L1 and L2 can make uses an event counter.

### 10. State classes and retained facts

| Class | TP-007 contents | Rule |
|---|---|---|
| **Canonical** | Node levels; L1 `SurfacePos`, `target`, `arrived`, `wait_steps`, `targets_drawn`; the whole `Background` record; the world seed, dataset version, and `WalkParams` | Never discarded by a transition; hashed; saved by TP-009 |
| **Identity / baseline** | `EntityId`, `Resident { address, home }`, buildings | Regenerable from seed, dataset version, and address; untouched by transitions |
| **Reconstructable** | A waiting agent's position; a promoted walker's position | Recomputed by `promote`; exactly at `m == 0`, by the L2 model otherwise |
| **Derived / transient** | `velocity`; the L2 event queue; per-building and per-cell counts | Recomputed on demand or rebuilt; not hashed; no system may read `velocity` |
| **Observed / committed facts** | None in TP-007 | See below |

**Observed facts.** TP-007 has no player, so nothing is observed. To keep the observation cache and notability ([12](../architecture/12-materialization.md) §13.4, [09](../architecture/09-entities-and-agents.md) §10.6) possible:

1. Every component that can exist at a finer level declares its class in this table's terms (in its crate README), and a demotion may discard only *reconstructable* and *derived* state. In TP-007 that is exactly the waiting position, the per-step position of a walker beyond its keyframe, and `velocity`.
2. Facts that the L1 representation exposes (who exists, which building each agent is at or heading to, how many targets it has chosen) are canonical at L2 and survive every round trip exactly. "The world does not lie" holds for the toy without an observation cache.
3. The later observation cache sits beside `demote`: it may keep the discarded remainder (exact fine state) keyed by `(EntityId, demotion boundary)` for a bounded window, and `promote` consults it before the L2 model. It never replaces canonical L2 state, so the conservation rules of §5 hold with or without it.
4. A refining decision taken at materialization (who exactly, where exactly) becomes canonical when taken and is recorded as a world event (TP-008, [24](../architecture/24-world-events-and-causality.md)); TP-007 takes none, because the toy's L2 state already determines every L1 field.

### 11. State hashing

`ToyWorld::state_hash` changes in the implementation PR as follows (still SipHash-2-4 over canonical words in ascending ID order, each column prefixed by a tag and its length, Tier 2):

- **Added**: the `node_lod` column (building ID, level as `u8`) and the `background` column (ID, `targets_drawn`, activity tag, building IDs, every `SimTime` as its `i64` bits, origin tag, and a `Point`'s cell and offset bits).
- **Kept**: the identity, `positions`, and `walks` columns, so an agent's residency is visible in the hash through which column lists its ID.
- **Removed**: `Walk::velocity`, reclassified as derived (§10). The TP-006 test `state_hash_covers_every_component` asserts that changing `velocity` changes the hash; the implementation PR inverts that one assertion (changing `velocity` must *not* change the hash) and adds assertions for every new canonical field. Under [testing-policy.md](../process/testing-policy.md) §2 that edit needs the Product Owner's approval; accepting this ADR is that approval.
- **Never hashed**: the event queue, column capacities, storage indices, memory addresses, counts, and anything for rendering.

Expected comparisons (all on one build and platform, Tier 2):

| Comparison | Expectation |
|---|---|
| L1 state, then L1 → L2 → L1 at the same boundary | Identical hash |
| Two runs with the same seed, start, and the same transition requests at the same ticks | Identical per-tick hashes |
| A run whose transitions were requested in a different order within each request, or whose worlds were built in a different insertion order | Identical per-tick hashes |
| An L2 store drained every tick versus in one jump to the same boundary (no reconciliation in between) | Identical hash at that boundary |
| Runs with different LOD histories over elapsed time | Not hash-comparable: compared with §5's invariants and tolerances, never with hashes |
| The same scenario on Linux and macOS | Not hash-comparable (Tier 3) |

### 12. Errors, invariants, and atomicity

The repository's philosophy applies: invalid inputs return typed errors and leave the world unchanged; nothing is resolved or repaired silently; internal invariants are checked by a checker that is run after every transition in debug builds and everywhere in tests; impossible states found inside a commit are bugs and panic.

**Typed errors** (`LodError`, returned by `set_lod` and by a failing step; world unchanged):

| Situation | Error |
|---|---|
| A request names something that is not a building | `UnknownNode(id)` |
| A request lists a building twice | `DuplicateNode(id)` (even with the same level; a request is a set) |
| A requested level other than L1 or L2 | `UnsupportedLevel(level)` |
| Position reconstruction fails (`from_ecef`, frame conversion) | `Coord(CoordError)`; the whole transaction is rejected |
| An event or `depart` time would pass `SimTime::MAX` | `Time(TimeOverflow)` |
| `targets_drawn` would pass `u32::MAX` at a departure (about 9,500 simulated years at the probe's rate) | `CounterExhausted(id)`; the L1 movement step uses the same checked increment instead of relying on the overflow panic |
| A transition or step finds an invariant violation in the state it is about to change (an ID in both stores or in neither, an L2 time off the grid or already past, an arrived L1 agent not at its target's centre, an unknown building in a record) | `Invariant(LodViolation)`; the world is already inconsistent, so it is reported, never repaired |

**The invariant checker** `check_lod_invariants(&ToyWorld, now) -> Result<(), Vec<LodViolation>>` reports every violation, not just the first:

- `Duplicated(id)`: in both the L1 and L2 stores.
- `Lost(id)`: in the identity column but in neither store. `Orphan(id)`: in a state store but not in the identity column. `PartialL1(id)`: in only one of `positions` and `walks`.
- `IdentityMismatch(id)`: a procedural resident whose ID is not `resident_id(seed, dataset_version, address)`.
- `ResidencyMismatch { id, governing, level }`: residency differs from the governing building's level (checked at the points where §4, item 3, guarantees it).
- `QueueMismatch(id)`: the event queue is not exactly one entry per L2 agent at its next event time.
- `BadSchedule(id)`: a time off the grid, `depart ≤ now`, `arrive ≤ now`, `origin_time > now`, `arrive − origin_time < Δ`, or an unknown building in a record.
- `NotAtCentre(id)`: an arrived L1 agent away from its target's centre. `NotInOwnCell(id)`: an L1 position outside its own cell.
- `CountMismatch { quantity, before, after }` and `CounterChanged(id)`: from the conservation comparison of §5 across a transaction.

**Atomicity.** A transaction and a step each either complete or change nothing (validate, then an infallible commit; §2). A request covering several buildings is one transaction. Silent duplication or loss is impossible by construction and detected by the checker if a bug introduces it.

**Fault injection.** A test-only hook (behind `cfg(test)` or a non-default feature, never in release builds) breaks a transition in three ways: inserting into L2 without removing from L1, removing from L1 without inserting into L2, and changing a `targets_drawn`. The checker must report `Duplicated`, `Lost`, and `CounterChanged` respectively (the issue's injected-fault criterion).

### 13. Determinism tiers

| Concept | Tier | Why |
|---|---|---|
| `EntityId`, generation addresses, the identity column | 1 | ADR 0016; untouched by transitions |
| Population and ID-set conservation, residency partition | 1 (as invariants) | Exact everywhere, by construction |
| Transition ordering (ascending ID), request validation, node levels | 1 | Integer decisions on sorted sets |
| Event ordering `(τ, id)`, grid arithmetic (`m`, `wait_steps ↔ depart`) | 1 | Integer time (ADR 0014) |
| `next_target` choices and `targets_drawn` | 1 | Integer hashing (ADR 0016 §3) |
| A leg's step count `n` and so its `arrive` time | 2 | Computed once with f64 distance; afterwards an integer. Building-to-building legs use only baseline positions and correctly rounded operations, so they are expected to agree across platforms, but that is not required |
| Which events fall in which step, and so residency at a given time | 2 | Follows from `n` |
| Promoted positions and `velocity` | 2 within a platform, 3 across | f64/f32 geometry; cell membership of the result is Tier 1 given the point (ADR 0015) |
| L1-versus-L2 equivalence | 3 | Tolerances of §5; never hashes |

Nothing in this ADR weakens an existing requirement: Tier 1 values stay integer-only, and Tier 2 code still replays bit-exactly on one build and platform.

### 14. Required implementation tests

All with fixed seeds; property tests with `proptest` at a fixed seed and no persistence file, as in TP-004 to TP-006. "Every step" means the checker of §12 runs after every transaction and every step.

| Test | What it asserts |
|---|---|
| `zero_time_round_trip_is_exact` | For several seeds, after 0, 1, and 600 L1 steps (agents waiting, departing, mid-leg, near cell edges): `set_lod(all → L2)` then `set_lod(all → L1)` at the same boundary leaves the canonical L1 state and the state hash bit-identical. Also per building, and for random subsets |
| `round_trip_conserves_ids_and_counts` (property) | Over seeds, transition times, durations, and random building subsets: the ID set, population, per-home and per-governing-building counts, and every `targets_drawn` are identical across L1 → L2 → L1 at the same boundary (`A(D(x)) == x`) |
| `repeated_cycles_never_lose_or_duplicate` (property) | Random sequences of `(steps to run, request)` over 1–50 cycles: after every transaction and step, `|L1| + |L2| == |residents|` and the ID partition holds |
| `population_is_constant_across_stores` | Total across stores equals the generated population at every tick of a mixed-level run |
| `identity_survives_every_transition` | Every agent keeps its `EntityId`, its identity component, and `resident_id(address) == id` through every transition of a long mixed run |
| `lod_does_not_perturb_target_sequences` | Each agent's sequence of targets in an all-L1 run equals its sequence under random LOD changes (one sequence a prefix of the other, differing in length by at most one at the end of the window) |
| `l2_matches_l1_over_hours` | All agents demoted at `t = 0` (all waiting at home), drained for 1 and 8 simulated hours, promoted, and compared with an all-L1 run: targets and `targets_drawn` equal; `arrived` and `wait_steps` equal for every agent whose legs all agree in step count (expected: all); waiting agents bit-exactly at their building's centre; walkers within 0.1 m of the L1 position and inside the bounding box of their origin and destination (the issue's "in the building its schedule says" criterion) |
| `long_advance_equals_tick_by_tick_drain` | An L2-only world drained to `t` in one call, in calls of 1, 7, and 600 steps, and through `advance_by_host_elapsed` with varied host inputs, gives identical L2 state and hash at `t`; includes agents that cross several arrive/wait/depart cycles in one call and one-step legs |
| `mid_trip_promotion_is_on_the_leg` | Agents promoted on every step of a leg are on the straight segment, at `m·step_m` from the origin (within 1 mm), in their own cell |
| `insertion_and_request_order_do_not_matter` | Worlds built in shuffled insertion orders (L1 and L2 agents), and requests listing buildings in shuffled orders, give identical per-tick hashes |
| `cell_edges_do_not_oscillate` | A dataset with a building centre within 100 µm of a cell edge and legs crossing edges; transitions every tick: every agent stays in its own cell, waiting agents never change cell, and no walker's cell sequence within a leg returns to a cell it left unless its straight line does |
| `state_hash_covers_lod_state` | Changing a node level, any `Background` field, or an agent's store changes the hash; changing `velocity` or the queue's construction does not |
| `replay_with_transitions` | Same seed, same requests at the same ticks: identical per-tick hashes for 10,000 ticks, including in a separate process (extends `tests/replay.rs`) |
| `event_queue_order_is_total` | Events with equal times are processed in ascending ID order; a one-step leg arrives in its departure step; the queue always equals its rebuild from the `background` column |
| `requests_are_validated_atomically` | Unknown building, duplicate building, unsupported level, and an injected `Coord` failure each return their error and leave the hash unchanged; restating a building's current level is a no-op |
| `checker_catches_injected_faults` | The three injected faults of §12 are reported as `Duplicated`, `Lost`, and `CounterChanged`; a clean run reports nothing |
| `start_must_be_aligned` | `World::new` rejects a start 1 µs off the grid and accepts `Rate::tick_start` of any index |
| `lod_soak_short` (every PR) | 1,000 agents, 30 simulated minutes, a random request every 1–100 steps, checker after every step |
| `lod_soak_long` (nightly, marked as such) | 3,000 agents, 24 simulated hours, same scheme, plus the `l2_matches_l1_over_hours` comparison at the end |

`ws-math` and `ws-rng` need no new tests. Tier 1 golden files are unchanged: TP-007 adds no Tier 1 generator output.

### 15. Performance model

| Quantity | Model or target | How measured |
|---|---|---|
| Memory per L1 agent | 96 bytes (TP-006, unchanged) | `size_of` test |
| Memory per L2 agent | ≤ 64 bytes in `background` (ID copy included); ≤ 128 bytes with identity and queue entries | `size_of` test plus an estimate of `BTreeSet` node overhead per entry |
| CPU of L1 advance | About 0.8 µs per agent per step, 36,000 steps per agent-hour (TP-006 benchmark) | `cargo bench -p ws-sim` |
| CPU of L2 advance | No per-agent work between events; a step with nothing due costs one queue peek. Target ≤ 2 µs per event, about 15 events per agent-hour, so at least 1,000× cheaper than L1 per simulated hour | New benchmark: step cost with 3,000 and 100,000 L2 agents, and per-event cost |
| Transition cost | Demote ≤ 1 µs per agent, promote ≤ 2 µs per agent (dominated by `from_ecef`), plus O(N) column merges | New benchmark for 30 and 3,000 agents (the issue's sizes) |
| Large batches | Linear in the number of agents moved plus the column sizes: no per-agent insertion | Benchmark at 100,000 agents; compare per-agent cost with 3,000 |

Against [15-ios-performance-memory.md](../architecture/15-ios-performance-memory.md) §16.3, a 3,000-agent transition should fit in half of the 8 ms high-priority budget on the Linux benchmark machine. The implementation PR records whether it does and, if not, that transitions will need to be split across frames by the later interest manager (05 §6.4, 12 §13.5), which the per-building transaction granularity already allows. In the toy, L2 is not smaller than L1 in memory (both are about 100 bytes, because TP-006's L1 agent carries no needs, plans, or perception); the memory saving appears when L1 grows towards the 0.5–1 KB of 09 §10.8, while the CPU saving is immediate. Measuring the iPhone is TP-011's job.

### 16. Extending to L0–L7

TP-007 implements L1 and L2 only. The principles it fixes are meant to hold for every pair:

| Principle | TP-007 form | Later |
|---|---|---|
| Identity stored once, outside level stores | Identity column untouched by transitions | Every level; at L3+ an anonymous procedural individual may leave the stores entirely and be regenerated from its address, a spawned or notable one never |
| Residency is store membership; node level is separate | Mutually exclusive columns; `node_lod` | L0 adds a store on top of L1's; L3+ represent people as counts in node aggregates |
| Transitions are validated transactions at tick boundaries, ascending ID, batch commits | `set_lod`, reconciliation | Same shape for every pair; multi-level jumps (L4 → L2) are one transaction composed of adjacent `D`s (05 §6.2) |
| Conservation summary before and after, exact for counts and identity | Population, IDs, homes, counters | Plus money, goods, cohort counts; distributions within tolerance (05 §6.3) |
| Per-entity counters carried at every level that can make the decision | `targets_drawn` | Schedules, needs, and life events with their own counters |
| Coarse levels are event-driven or coarse-stepped, with cadence-independent catch-up | L2 drain | L3 hourly, L4–L5 daily, L7 analytic; frozen nodes drained on read |
| State classes decide what a demotion may discard | §10 table | Observation cache and notability plug in beside `demote`/`promote` |
| Governing node by a function of entity state | Destination building | Spatial nodes (block, sector cell, district) once the coarse model can place an agent in them; the interest manager supplies levels |

What deliberately does not carry over: the toy's L2 schedule is history-dependent (the next target depends on where the agent is), so it is not a baseline projection and every L2 agent is canonical. Real L2 schedules (Vertical Slice) are projected from a baseline template and only divergences are canonical, which is what lets L3 discard unchanged individuals ([12](../architecture/12-materialization.md) §13.3).

### 17. Scope exclusions

Not part of TP-007 or of this ADR's implementation: TP-003 FFI and Unity; TP-008 commands, journal, and world events (including `sim.lod_changed`); TP-009 persistence of L2 state; TP-010 interventions; TP-011 device measurements; TP-012 Cesium and providers; real NYC data; households and economics beyond `home`; pathfinding; needs; relationships; LLM behaviour; threads or a scheduler framework; the interest manager, hysteresis, and budgets; L0 and L3–L7; the observation cache; statistical-equivalence runs beyond the two comparisons of §14.

## Consequences

- TP-007 can be implemented without architectural choices left open: types, transitions, tick order, errors, and tests are fixed here; names in code may differ.
- `ws-world` gains the `background` and `node_lod` columns, the event queue, `demote`/`promote`, reconciliation, the checker, fault injection, and a validated way to insert an agent directly in L2 state (beside `insert_resident_state`, for the insertion-order tests); `ToyWorld::step` gains the time and base step it needs for L2. `ws-sim` gains `set_lod`, the phase order, and the start-alignment error. No crate is added, no dependency is added, and the layer table is unchanged.
- The state hash changes composition (new columns added, `velocity` removed), so every TP-006 Tier 2 hash value changes. No Tier 1 golden value changes.
- `insert_resident_state` starts rejecting an arrived agent that is not at its target's centre; `WorldConfig::start` must be aligned. Both are behaviour changes of TP-006 APIs made by TP-007.
- [05-simulation-lod.md](../architecture/05-simulation-lod.md) §6.3, [09-entities-and-agents.md](../architecture/09-entities-and-agents.md) §10.3, [12-materialization.md](../architecture/12-materialization.md) §13.1, [21-decisions-now-vs-later.md](../architecture/21-decisions-now-vs-later.md) D-10, and [07-coordinates-and-time.md](../architecture/07-coordinates-and-time.md) §8.9 point here. Architecture V1's decisions are unchanged; this ADR details them for one pair of levels.
- Revisit if: the transition benchmark exceeds the frame budget at the sizes the interest manager will request; the 1.4% one-step re-plan difference becomes visible to a player (then L1 could carry its leg's arrival time); a domain needs a decision at both levels that cannot be expressed with an event counter; or spatial nodes replace buildings as governing nodes.
