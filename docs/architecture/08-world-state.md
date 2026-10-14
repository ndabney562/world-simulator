# 9. World-State Architecture

Covers point 9.

## 9.1 Three kinds of state

Everything in the world falls into exactly one of three categories. Keeping them separate is what makes the world both huge and storable.

| Kind | Definition | Stored in saves? | Examples |
|---|---|---|---|
| **Canonical** | Source of truth that cannot be regenerated | Yes | Aggregates of touched regions, notable entities, everything changed by interventions, history log, RNG stream positions where needed |
| **Procedural baseline** | Content that is a pure function of `(world seed, address, generator version, upstream canonical state)` | No (regenerated) | Terrain, road layout, buildings, the default residents of a building, their names and traits |
| **Derived / transient** | Caches, indices, materialized detail with no lasting changes, render data | No | Spatial indices, pathfinding graphs, materialized anonymous NPCs, GPU meshes |

The world is therefore:

> **World = procedural baseline(seed, generator versions) + canonical deltas + journal**

A region nobody has touched costs zero bytes in a save. A region the player has interacted with costs only what changed.

## 9.2 The world tree

All spatial state hangs off one hierarchy, which doubles as the simulation-LOD hierarchy ([05](05-simulation-lod.md)) and the streaming hierarchy:

```
World
 └─ Star system
     └─ Body (planet / moon)               L6–L7 state: orbit, climate, macro-demography
         └─ Region (cube-sphere cell, ~100–1,000 km)     biomes, settlements list, trade
             └─ Settlement (city / town)   L5 state: economy, governance, culture
                 └─ District (~1–5 km)     L4 state: cohorts, services, flows
                     └─ Sector cell (~1 km)   unit of materialization, streaming, persistence
                         └─ Parcel / building    L3 state: occupancy, condition, ownership
                             └─ (unit / room)    generated on demand
```

Two kinds of hierarchy coexist:
- **Spatial cells** (the cube-sphere quadtree) give addressing, partitioning, and streaming.
- **Semantic nodes** (settlement, district, building) give simulation meaning. Each semantic node is anchored in one or more cells; a city spans many sector cells. The mapping between them is generated procedurally and stored only when changed.

## 9.3 Node state

Each node carries:
- **Identity**: a stable ID (see [09](09-entities-and-agents.md)) and its address in the tree.
- **Baseline reference**: generator ID and version used to create it.
- **Current LOD level** and its level-specific state (for example a district at L4 holds cohort tables; at L2 it holds a list of resident IDs and schedules).
- **Aggregate summary** that is always present regardless of level (population, key indicators). This is what parents read and what the UI shows when zoomed out, so the zoomed-out view never needs to materialize anything.
- **Delta set**: canonical changes relative to the baseline (a building demolished, a park added, a resident moved out).
- **Dirty flags** for incremental saving.

## 9.4 Storage layout in memory

- **Active detail** (materialized entities at L0–L2) lives in an ECS (archetype or sparse-set storage) for cache-friendly iteration by systems.
- **Aggregates** live in compact per-level tables (struct-of-arrays), keyed by node ID, because they are iterated by coarse systems in bulk.
- **Cold canonical state** (touched but not currently loaded regions) lives on disk ([14](14-persistence.md)) and is paged in by the streaming system in sector-cell or district chunks.
- **Spatial indices** (per-cell grids and a hierarchical index over cells) are derived and rebuilt on load.

## 9.5 Ownership and mutation rules

1. Every piece of state has exactly one owning system. Other systems read it or send events/commands to the owner.
2. External mutation only via commands ([04](04-simulation-architecture.md)).
3. Cross-node effects only via flows, events, and environmental fields, never by reaching into another node's internals. This keeps nodes independently simulatable at different levels and independently storable.
4. All mutations are applied at tick boundaries in a deterministic order.

## 9.6 Environmental fields

Some state is naturally continuous over space rather than attached to entities: temperature, precipitation, pollution, noise, light, disease prevalence, land value. These are stored as **multi-resolution grids** attached to the cell hierarchy: coarse values at region level, refined on demand near the camera by procedural detail conditioned on the coarse value. Interventions (rain here, plague there) often act on fields.

## 9.7 History and facts

- A **history log** of significant events (births, deaths, marriages, fires, elections, interventions) is part of canonical state, indexed by entity, node, and time.
- **Facts the player has observed** (that a specific named person lives in this building, that a shop was closed) are canonical. This is how the "the world does not lie" guarantee from [05](05-simulation-lod.md) is enforced.
- Log retention: full detail for notable entities and recent events; older insignificant events are compacted into statistics.

## 9.8 Consistency between levels

The aggregate summary of a node must always equal the aggregation of its children (for counted quantities) or be within tolerance (for distributions). This is checked:
- After every LOD transition (in debug builds and tests).
- Periodically by a background validator in debug builds.
- Exhaustively in headless soak tests ([18](18-testing-strategy.md)).
