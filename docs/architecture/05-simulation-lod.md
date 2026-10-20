# 6. Simulation Level-of-Detail (LOD) Architecture

Covers point 6. The mechanics of moving between levels (materialization) are in [12](12-materialization.md); rendering LOD is separate and in [06](06-rendering-lod.md).

## 6.1 Simulation LOD is not rendering LOD

Rendering LOD decides how something is *drawn*. Simulation LOD decides how something is *represented and advanced in time*. They are related (the renderer can only draw individuals that exist) but independent: a district can be simulated at aggregate level while drawn as plausible crowds from visual proxies, and an important person far from the camera can stay fully simulated while drawn as a dot.

## 6.2 The level ladder

Every spatial node (cell, building, block, neighbourhood, city, region, planet, system) is simulated at exactly one level at a time. Proposed levels:

| Level | Name | Representation | Typical step | Typical count on device |
|---|---|---|---|---|
| L0 | **Focused** | Individual with full mind: needs, utility decisions, relationships, memory, conversations; physics-capable body | 0.05–0.1 s | tens |
| L1 | **Active** | Individual with needs and decisions, steering and pathfinding, simplified social interactions | 0.1–1 s | hundreds to a few thousand |
| L2 | **Background** | Individual as a schedule: current activity, location at building granularity, next event time | Discrete events | tens of thousands |
| L3 | **Household / building** | Occupancy counts, household budgets, building condition | Hourly | thousands of buildings |
| L4 | **Neighbourhood / district** | Cohorts (age × occupation × wealth ...), flows (commute, migration), services, crime, mood | Daily | hundreds |
| L5 | **Settlement / city** | System-dynamics model: economy, population growth, infrastructure, governance, culture traits | Daily to monthly | dozens |
| L6 | **Region / planet** | Climate zones, biomes, macro-demography, trade between settlements, factions, technology | Monthly to yearly | one per body |
| L7 | **Celestial** | Orbital elements and physical parameters | Analytic | dozens of bodies |

Notes:
- A level applies to the *contents* of a node. A building at L3 has occupancy counts but no individual occupants; the same building at L2 has a list of individuals with schedules.
- Levels are a ladder, not a requirement to pass through every rung. A district can jump from L4 to L2 if the camera arrives quickly, using the disaggregation functions of L4 → L3 → L2 in one transaction.
- **Notable individuals** are an exception to the spatial ladder: they always exist at least at L2, wherever they are, because the player cares about them (see [12](12-materialization.md)).

## 6.3 The aggregation / disaggregation contract

For every adjacent pair of levels (Lk, Lk+1), the design must define:

1. **Aggregation** `A: state(Lk) → state(Lk+1)`: summarises fine state into coarse state (for example individuals → cohort counts and distributions).
2. **Disaggregation** `D: state(Lk+1), seed, persistent overrides → state(Lk)`: generates fine state consistent with coarse state (for example cohort counts → specific individuals with deterministic identities).
3. **Conservation invariants** that must hold across the round trip, for example:
   - Population counts by cohort are exactly preserved: `A(D(x)) = x` for counted quantities.
   - Money, goods, and resources are conserved (no creation or destruction from a level change).
   - Persistent (notable) entities are never lost or duplicated.
   - Distributions (age, wealth) are preserved within tolerance where exact preservation is impossible.
4. **Equivalence tolerance** between models: advancing at Lk then aggregating should statistically match advancing at Lk+1. This is tested by running both paths headless over many seeds ([18](18-testing-strategy.md)). Exact equality is not expected; bounded drift is.

This contract is the backbone of the simulation. Each new domain (economy, disease, crime) added to the simulation must provide its A, D, and invariants for every level it exists at, or it is not allowed to cross levels.

The first executable pair, L1 ↔ L2 for the Technical Prototype's toy world (the two transitions, exact and toleranced invariants, tick order, randomness, and required tests), is detailed in [ADR 0017](../adr/0017-simulation-lod-l1-l2-round-trip.md).

## 6.4 Interest and LOD selection

An **interest manager** assigns a level to every node each LOD update (a few times per second, not every frame).

Interest sources, each producing a score per node:
- Camera: distance and screen-space size of the node, and the camera's current scale band.
- Player focus: the selected entity or region, the possessed character (if the design adds possession), bookmarks.
- Active interventions and their area of effect.
- Notable entities and ongoing significant events (a fire, a riot, an election).
- Time scale: higher warp caps the maximum allowed level (at 1,000× nothing is L0/L1; at orbital warp nothing is finer than L5).

Selection is **budget-driven**: each level has a cost model (per-entity CPU time and memory), and the manager assigns the finest levels to the highest-interest nodes until the CPU and memory budgets are reached. On a hot device (thermal state elevated), budgets shrink and nodes are demoted.

**Hysteresis:** promotion and demotion thresholds differ (for example promote at interest 1.0, demote below 0.7) and a node must stay at a level for a minimum time, to avoid thrashing when the camera hovers near a boundary.

## 6.5 Catch-up: what happens to a node that was not being simulated finely

When a node is promoted after being coarse for a while, its fine state is produced by disaggregation from the **current** coarse state, not by replaying the elapsed time at fine resolution. Combined with schedules (where each person "should" be at this time of day), this gives a plausible present without simulating the past in detail.

For nodes that are not observed at all and do not influence others, the scheduler may **freeze** them and fast-forward lazily when they are next needed (simulate on read), using coarse steps or closed-form models (for example compound population growth). Nodes that do influence others (a city that trades with the observed city) continue at their coarse level so the influence stays consistent.

## 6.6 Cross-level and cross-node interactions

- **Flows** connect coarse nodes: commuting, migration, trade, disease transmission. A flow is defined at the coarsest level shared by its endpoints.
- When one endpoint is fine and the other coarse (an L1 commuter leaving a materialized district for an L4 district), the agent crosses the boundary and is **demoted to L2** (a schedule with "at work in district B until 17:30"), which is cheap and keeps identity. When it comes back, it is promoted again.
- A coarse node cannot affect an individual in a fine node except through defined channels (flows, events, environmental fields). This keeps the interactions enumerable and testable.

## 6.7 What this means for game design

The player must never be able to "catch" the LOD system lying about something they can check. The design rules that follow:

1. Facts the player has observed are preserved (persistent entities, short-term observation cache).
2. Aggregates shown in the UI are the same numbers the simulation uses, never separate estimates.
3. Detail generated on zoom-in must be consistent with aggregates shown at the coarser zoom.
4. Where coarse models cannot produce a specific outcome (who exactly started the riot), the outcome is decided at materialization time and then becomes fact, recorded as a refining world event linked to the coarse event ([24](24-world-events-and-causality.md)).

## 6.8 Events across levels

Each level emits world events at its own granularity: individual events at L0–L2, and **aggregate events** at L3 and coarser ("14 births in this district in March"). The aggregation contract in 6.3 extends to events: individual events of a type roll up into the aggregate events of the next level, and aggregate counts must match ([24 §24.10](24-world-events-and-causality.md)). Causal attribution at coarse levels is statistical ([24 §24.6](24-world-events-and-causality.md)).

## Accepted real-slice reconciliation

[ADR 0028](../adr/0028-slice-l1-needs-and-navigation.md) defines the VS-005 real-slice L1/L2 state classification and exact need/decision/path reconciliation alongside the unchanged toy rules of ADR 0017. The contract preserves schedule deadlines and off-map identities; camera interests, hysteresis and transition budgeting remain VS-006/#70. It is **Accepted** under the standing technical delegation recorded on #77 and PR #88 after independent review, and ships no runtime behavior.

[Accepted ADR 0029](../adr/0029-slice-camera-interest-and-aggregates.md) specifies VS-006/#70's coarse camera input, deterministic building policy, retained-resident L3 summaries, exact count checks and new slice-only LOD facts. Acceptance follows clean independent review under the technical delegation on#77/PR#90; runtime implementation follows separately. Existing toy LOD/event formats are unchanged.
