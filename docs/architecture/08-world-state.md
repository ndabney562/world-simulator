# 9. World-State Architecture

Covers point 9. Related: [11](11-procedural-generation.md) (data and generation), [14](14-persistence.md) (saves), [24](24-world-events-and-causality.md) (events), [23](23-visual-providers.md) (why visual providers are not world state).

## 9.1 Three kinds of state

Everything in the world falls into exactly one of three categories. Keeping them separate is what makes a real-Earth world both huge and storable.

| Kind | Definition | Stored in saves? | Examples |
|---|---|---|---|
| **Canonical** | Source of truth that cannot be regenerated | Yes | Aggregates of touched areas, notable entities, everything changed by interventions, the world event log, the command journal, RNG stream positions where needed |
| **Baseline** (data-driven + procedural) | A pure function of `(dataset version, world seed, address, generator version, upstream canonical state)` | No (regenerated) | Real terrain, real streets and building footprints and heights from processed open data; synthetic residents, households, names, traits, and schedules; building use where the data is silent; facades and interiors |
| **Derived / transient** | Caches, indices, materialized detail with no lasting changes, render data | No | Spatial indices, navigation graphs, materialized anonymous NPCs, GPU meshes, **everything from visual providers** |

So the world is:

> **World = baseline(dataset version, seed, generator versions) + canonical deltas + command journal + world event log**

An area nobody has touched costs zero bytes in a save; a touched area costs only what changed. Real-world data enters only through the **versioned simulation tiles** produced by our pipeline ([11](11-procedural-generation.md)). Visual providers such as Google 3D Tiles are never part of world state ([23](23-visual-providers.md)).

## 9.2 The world tree

All spatial state hangs off one hierarchy, which doubles as the simulation-LOD hierarchy ([05](05-simulation-lod.md)) and the streaming hierarchy:

```
World
 └─ Solar system                      ephemerides (L7)
     └─ Earth                         climate, macro-demography (L6)
         └─ Region (S2 cell ~100–1,000 km)    e.g. New York metro: trade, migration
             └─ Settlement / city      L5: economy, governance (e.g. New York City)
                 └─ Neighbourhood / district   L4: cohorts, services, flows (e.g. Bay Ridge)
                     └─ Sector cell (~1 km)    unit of materialization, streaming, persistence
                         └─ Block → Parcel / building    L3: occupancy, condition, use
                             └─ Unit / room            generated on demand
```

Two kinds of hierarchy coexist:
- **Spatial cells** (the S2-style hierarchy) give addressing, partitioning, and streaming.
- **Semantic nodes** (city, neighbourhood, block, building) give simulation meaning. They come from real data where possible (administrative boundaries, neighbourhood tabulation areas, Census tracts and blocks, tax lots, and building identifiers from NYC data) and are anchored to one or more cells.

## 9.3 Node state

Each node carries:
- **Identity**: a stable ID ([09](09-entities-and-agents.md)). Where the source data has a stable real-world key (such as NYC's building identifier), it is recorded as an attribute; it is never the only key, because data versions change.
- **Baseline reference**: the dataset version and generator versions used.
- **Current LOD level** and its level-specific state.
- **Aggregate summary**, always present regardless of level (population, key indicators). This is what parents read and what the UI shows when zoomed out.
- **Delta set**: canonical changes relative to the baseline.
- **Provenance tags** on state that interventions or major events changed (bounded; [24 §24.6](24-world-events-and-causality.md)).
- **Dirty flags** for incremental saving.

## 9.4 Storage layout in memory

- **Active detail** (materialized entities at L0–L2) lives in an ECS for cache-friendly iteration.
- **Aggregates** live in compact per-level struct-of-arrays tables keyed by node ID.
- **Cold canonical state** (touched but not loaded areas) and the **event store** live on disk ([14](14-persistence.md)) and are paged in by cell.
- **Spatial indices** are derived and rebuilt on load.

## 9.5 Ownership and mutation rules

1. Every piece of state has exactly one owning system; others read it or send events and commands to the owner.
2. External mutation happens only through commands ([04](04-simulation-architecture.md)).
3. Cross-node effects go only through flows, scheduler events, and environmental fields, never by reaching into another node's internals.
4. Mutations are applied at tick boundaries in a deterministic order.
5. Meaningful mutations emit world events with cause and provenance ([24](24-world-events-and-causality.md)).

## 9.6 Environmental fields

Continuous quantities (temperature, precipitation, pollution, noise, light, disease prevalence, land value) are stored as **multi-resolution grids** on the cell hierarchy: coarse values at region level, refined on demand near the camera. They are seeded from real climate normals where available. Interventions often act on fields, and fields carry provenance tags so that consequences can be traced.

## 9.7 History and facts

- The **world event log** ([24](24-world-events-and-causality.md)) is canonical state: the structured history of the world, indexed by actor, location, type, time, and cause.
- **Facts the player has observed** (that a specific person lives in this building) are canonical, which enforces the "the world does not lie" guarantee ([05](05-simulation-lod.md)).

## 9.8 Consistency between levels

The aggregate summary of a node must equal the aggregation of its children (for counted quantities) or be within tolerance (for distributions). Aggregate events must match the individual events they summarise. These are checked after every LOD transition in debug builds, periodically by a background validator, and exhaustively in headless soak tests ([18](18-testing-strategy.md)).
