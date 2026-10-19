# 5. Simulation Architecture

The accepted TP-008 boundary queue, pause/resume and transaction rules are detailed in [ADR 0019](../adr/0019-command-journal-and-world-events.md). Noah accepted the ADR on 2026-10-16. TP-008 replaces production direct setters with journaled commands.

Covers point 5. Simulation LOD is in [05](05-simulation-lod.md), entities in [09](09-entities-and-agents.md), events and causality in [24](24-world-events-and-causality.md), and reproducibility in [25](25-determinism-policy.md).

## 5.1 Shape of the core

The simulation core is a **headless, reproducible, multi-rate, hybrid time-stepped / discrete-event simulator** written in Rust. It is organised as a hierarchy of spatial nodes, each simulated at a chosen level of detail.

```
                ┌──────────────────────── Core API ─────────────────────────┐
 commands ────► │ Command queue ─► validate ─► apply at tick boundary       │
                │                                   │                       │
                │  Scheduler (multi-rate)  ◄── Interest / LOD manager       │
                │   ├─ time-stepped systems (active regions)                │
                │   ├─ discrete-event queue (background agents, aggregates) │
                │   └─ analytic models (ephemerides, slow processes)        │
                │                                   │                       │
                │                     World state (see 08)                  │
                │                                   │                       │
                │  Command journal (inputs)    World event log (outputs, 24)│
                │  Persistence (14)            Snapshot builder ─► Unity    │
                └───────────────────────────────────────────────────────────┘
```

## 5.2 Core properties

1. **Headless.** No rendering or platform dependencies. It runs identically in the iOS app (inside Unity), the headless CLI, CI, and tests.
2. **Reproducible, by tier** ([25](25-determinism-policy.md)):
   - Identity and structural decisions in the regenerated baseline are exact on every platform (Tier 1).
   - Replaying the journal on the same build and platform reproduces the same state (Tier 2).
   - Continuous values may differ within bounds across platforms (Tier 3).

   The core practices that make this work: **counter-based RNG streams** keyed by stable identifiers (for example `hash(world_seed, entity_id, purpose, tick)`) rather than a shared sequential RNG; **canonical iteration orders**; **deterministic parallel merges**; **integer time**; and no wall-clock influence on simulation results.
3. **Command-driven.** The only way to change state from outside is a command (player action, god intervention, simulation-relevant setting, or an adopted LLM suggestion). Commands are validated, applied at a tick boundary, and recorded in the **journal**.
4. **Event-producing.** Meaningful outcomes are emitted as structured **world events** with causal links ([24](24-world-events-and-causality.md)). The journal records inputs; the event log records what happened.
5. **Budgeted.** Each `advance()` call receives a wall-clock budget; the scheduler does the highest-priority work first. The simulation falls behind gracefully (low-priority regions update less often) rather than dropping frames.

## 5.3 Time

- Simulation time is a signed 64-bit integer of microseconds from the J2000 epoch, mapped to the real UTC calendar ([07](07-coordinates-and-time.md)).
- The player controls a **time scale** (paused, 1×, 10×, 100×, up to orbital time warp). Time scale is an input to LOD selection: at high warp, fine levels are suspended and coarse models take over ([05](05-simulation-lod.md)).
- Each system declares its native step. Examples (tunable):

| System | Typical step | Mechanism |
|---|---|---|
| Rigid-body physics (physics bubble only) | 1/60 s | Time-stepped |
| Active agent locomotion and steering | 1/10–1/20 s | Time-stepped |
| Active agent decisions | 0.5–2 s, or on event | Time-stepped plus events |
| Background agent activities | Event-driven ("arrives at work at 08:47") | Discrete-event |
| Household and building aggregates | Hourly | Time-stepped (coarse) |
| District / city aggregates (economy, demographics) | Daily | Time-stepped (coarse) |
| Region / planet (climate, macro-history) | Monthly to yearly | Time-stepped or analytic |
| Sun, moon, planets | Any t | Analytic (ephemerides) |

## 5.4 Hybrid scheduling: why discrete events matter

Most of the world is in the background most of the time. Background agents with schedules do not need per-frame updates; they change state a handful of times a day. A **discrete-event queue** (a priority queue of `(time, target, event)`) makes thousands of background agents cost almost nothing between events. When a region becomes active, scheduled events are converted into continuous, time-stepped behaviour; when it becomes inactive, continuous behaviour is converted back into scheduled events. This is the simulation-side half of materialization ([12](12-materialization.md)). Scheduler events (internal wake-ups) are distinct from world events (meaningful history), although processing one may produce the other.

## 5.5 Systems and rules as data

- Systems (needs decay, schedule selection, commuting, the economy tick, disease spread, weather) are code; their **parameters are data**: versioned rule files loaded at start (need decay rates, job types, building use types, event importance weights). The rules version is recorded in every save.
- Systems declare which components they read and write, which LOD levels they run at, and which world-event types they can emit. The scheduler uses this to order systems, parallelise safely, and check that no system writes state it does not own.

## 5.6 Parallelism on iPhone

- A small job system (or `rayon` with deterministic partitioning) sized to the device: high-priority work on performance cores, background generation and coarse updates on efficiency cores.
- The partitioning unit is the spatial cell, so most interactions are local. Cross-cell interactions (an agent walking from one cell into another) are exchanged through message buffers applied in a deterministic merge step.

## 5.7 The core's API surface (conceptual)

| Call | Purpose |
|---|---|
| `create_world(seed, dataset_version, settings)` / `load(save)` | Start a world |
| `submit(commands)` | Queue commands (player actions, interventions) |
| `set_view(camera, time_scale, focus)` | Tell the interest manager what the player is looking at |
| `advance(wall_clock_budget)` | Run the scheduler |
| `fill_render_snapshot(buffers)` | Write the render snapshot into Unity-provided buffers |
| `query(request)` | Inspector, history, and causal queries ([24](24-world-events-and-causality.md)) |
| `save(target)` | Incremental or full save |

This list is conceptual and will be finalised as an ADR in the Technical Prototype. It is shown here because its shape (commands in, snapshots and query results out) is a decision made now.

The TP-003 synchronous prototype subset is specified in [Accepted ADR 0018](../adr/0018-rust-unity-bridge.md); the conceptual API above remains the long-term shape.

## Accepted real-slice schedule contract

[ADR 0027](../adr/0027-slice-schedules-and-events.md) defines the VS-004/#68 `ws-life` domain and `SliceSimulation` path, with integer schedule phase, typed transitions and atomic event publication. It is Accepted under Noah's recorded standing approval; separate implementation remains #68. Current toy behavior and existing formats are unchanged.
