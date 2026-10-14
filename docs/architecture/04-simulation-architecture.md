# 5. Simulation Architecture

Covers point 5. Simulation LOD is in [05](05-simulation-lod.md); entities in [09](09-entities-and-agents.md).

## 5.1 Shape of the core

The simulation core is a **headless, deterministic, multi-rate, hybrid time-stepped / discrete-event simulator** organised as a hierarchy of spatial nodes, each simulated at a chosen level of detail.

```
                ┌──────────────────────── Core API ────────────────────────┐
 commands ────► │ Command queue ─► validate ─► apply at tick boundary      │
                │                                   │                      │
                │                                   ▼                      │
                │  Scheduler (multi-rate)  ◄── Interest / LOD manager      │
                │   ├─ time-stepped systems (active regions)               │
                │   ├─ discrete-event queue (background agents, aggregates)│
                │   └─ analytic models (orbits, slow processes)            │
                │                                   │                      │
                │                       World state (see 08)               │
                │                                   │                      │
                │  Journal (commands + significant events) ─► persistence  │
                │  Snapshot builder ─► render / inspector snapshots        │
                └──────────────────────────────────────────────────────────┘
```

## 5.2 Core properties

1. **Headless.** No rendering or platform dependencies. Runs identically in the iOS app, the headless CLI, CI, and tests.
2. **Deterministic.** Given the same world seed, generator and rules versions, build, and command journal, the core produces identical state. Requirements:
   - All randomness comes from **counter-based deterministic RNG streams** keyed by stable identifiers (for example `hash(world_seed, entity_id, purpose, tick)`), never from a shared global RNG whose sequence depends on call order. This lets systems run in parallel or be skipped without changing other entities' outcomes.
   - No iteration over hash maps with randomised ordering; collections that affect outcomes iterate in a defined order (by ID).
   - Parallel work is partitioned by region or entity; results are merged in canonical order; no parallel floating-point reductions whose order depends on scheduling.
   - Transcendental math (sin, cos, exp, pow) uses a software implementation (for example the `libm` crate) rather than the platform library, and FMA contraction is controlled. Hot paths may use fast approximations only if they are themselves deterministic.
   - Wall-clock time never affects simulation results; only simulation time does.
3. **Command-driven.** The only way to change state from outside is a command (player action, god intervention, settings that affect simulation, later an adopted LLM proposal). Commands are validated, applied at a tick boundary, and recorded in the journal. This one mechanism gives saves, replay, undo, and debugging.
4. **Budgeted.** Each `advance()` call receives a wall-clock budget; the scheduler does the highest-priority work first and defers the rest. The simulation can fall behind real time gracefully (lower-priority regions update less often) instead of dropping frames.

## 5.3 Time

- Simulation time is a 64-bit integer count of microseconds from the world epoch (see [07](07-coordinates-and-time.md)). It is never a float.
- The player controls a **time scale** (paused, 1×, 10×, 100×, ... up to orbital time warp). The time scale is an input to LOD selection: at high warp, fine levels are suspended and coarse models take over (see [05](05-simulation-lod.md)).
- Each system declares its native timestep. Examples (tunable):

| System | Typical step | Mechanism |
|---|---|---|
| Rigid-body physics (physics bubble only) | 1/60 s | Time-stepped |
| Active agent locomotion and steering | 1/10–1/20 s | Time-stepped |
| Active agent decisions | 0.5–2 s, or on event | Time-stepped plus events |
| Background agent activities | Event-driven ("arrives at work at 08:47") | Discrete-event |
| Household and building aggregates | Hourly | Time-stepped (coarse) |
| District / city aggregates (economy, demographics) | Daily | Time-stepped (coarse) |
| Region / planet (climate, macro-history) | Monthly to yearly | Time-stepped or analytic |
| Orbits | Any t | Analytic (closed-form Kepler) |

## 5.4 Hybrid scheduling: why discrete-event matters

Most of the world is in the background most of the time. Background agents with daily schedules do not need per-frame updates; they need to change state a handful of times a day. A **discrete-event queue** (a priority queue of `(time, target, event)`) lets thousands of background agents cost almost nothing between events. When a region becomes active, its agents' scheduled events are converted into continuous time-stepped behaviour; when it becomes inactive, continuous behaviour is converted back into scheduled events. This is the simulation-side half of materialization ([12](12-materialization.md)).

## 5.5 Systems and rules as data

- Systems (needs decay, schedule selection, commuting, economy tick, disease spread, weather) are code, but their **parameters are data**: versioned rule files loaded at start (for example need decay rates, job types, building types, disease parameters). The rules version is recorded in every save.
- Systems declare which components they read and write and at which LOD levels they run. The scheduler uses this to order systems, parallelise safely, and validate that no system writes state it does not own.

## 5.6 Parallelism on iPhone

- A small job system (or `rayon` with deterministic partitioning) sized to the device: high-priority simulation work on performance cores, background generation and coarse updates on efficiency cores (via thread quality-of-service on iOS).
- Partitioning unit: spatial region (cell) for agents, so most interactions are local. Cross-region interactions (an agent walking from cell A to cell B) are exchanged through message buffers applied at a deterministic merge step.

## 5.7 The core's API surface (conceptual)

| Call | Purpose |
|---|---|
| `create_world(seed, settings)` / `load(save)` | Start a world. |
| `submit(commands)` | Queue commands (player, interventions). |
| `set_view(camera, time_scale, focus)` | Tell the interest manager what the player is looking at. |
| `advance(wall_clock_budget)` | Run the scheduler. |
| `fill_render_snapshot(buffers)` | Write the current render snapshot into provided buffers. |
| `query(request)` | Inspector queries (entity details, region stats, history). |
| `save(target)` | Incremental or full save. |

This list is conceptual and will be finalised as an ADR in Phase 0. It is shown here because its *shape* (commands in, snapshots out) is a decision to make now.

## 5.8 Events and history

The core emits **domain events** (birth, death, move, job change, fire started, intervention applied, ...). Significant events are appended to a persistent **history log** indexed by entity and region. This log feeds the inspector ("what happened here?"), NPC memories, news/narrative features, and later LLM context ([16](16-ai-llm-npcs.md)). Insignificant events are aggregated into statistics and discarded.
