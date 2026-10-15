# 10. Entity and Agent Architecture

Covers point 10. All people are **synthetic**: generated to match real Census statistics for the area, never modelled on real individuals (ADR 0001).

## 10.1 Entity model

**Entity-Component-System (ECS)** for all materialized entities (people, vehicles, buildings when detailed, props, animals):
- Entities are IDs; components are plain data; systems are functions over component sets.
- Chosen because it gives cache-friendly bulk iteration (essential for thousands of agents on a phone), easy composition (a person who becomes a firefighter gains a component rather than changing class), straightforward serialization, and natural LOD: an entity's level is expressed by which components it currently has.

Library choice (a Technical Prototype spike): evaluate `bevy_ecs` used standalone and `hecs`, against requirements of deterministic iteration order, no implicit parallel scheduling that could reorder writes, efficient add/remove of components (frequent during LOD changes), and serialization. Whatever is chosen is wrapped behind the core's own world API so it can be replaced. For the Technical Prototype, [ADR 0016](../adr/0016-entity-identity-storage-and-randomness.md) chooses a minimal sorted-column store in `ws-ecs` instead of a library.

## 10.2 Identity

Identity is the most important property for consistency across zoom levels.

- **Procedural identity**: every entity that the baseline *could* produce has a deterministic ID derived from its generation address, for example `hash(world_seed, dataset_version, building_id, household_slot, member_slot)`. Zoom in on the same building twice and the same resident gets the same ID, name, face, and traits, without anything having been stored. ID derivation is a Tier 1 operation: integer-only and identical on every platform ([25](25-determinism-policy.md)).
- **Persistent identity**: when an entity becomes notable (see 10.6), its procedural ID is registered in the canonical entity table and its state is stored. The ID does not change; only its storage status does.
- **Spawned identity**: entities created at runtime (a child is born, a god spawns a person) get IDs from a deterministic counter in canonical state, in a separate ID namespace.
- IDs are 64-bit with namespace bits; collisions in procedural IDs are checked in tests across large seed samples.

The bit layout, derivation inputs, and the counter-based randomness built on the same hash are fixed in [ADR 0016](../adr/0016-entity-identity-storage-and-randomness.md).

## 10.3 Agent representation by LOD level

The same person exists in different forms depending on level ([05](05-simulation-lod.md)):

| Level | Components present (illustrative) |
|---|---|
| L3 and coarser | None: the person is a count in a cohort, but can be re-derived from (building, slot) |
| L2 Background | Identity, home, workplace, current activity, location (building), next event time, key traits, key relationships |
| L1 Active | + exact position, movement, needs (hunger, energy, social, safety, ...), short-term plan, perception |
| L0 Focused | + full memory, emotions, detailed relationships, conversation state, physics body, animation state |

Promotion adds components (generated from the deterministic seed plus stored deltas); demotion strips them, folding anything that must persist into the canonical record first. What the Technical Prototype's L2 keeps, discards, and reconstructs is fixed in [ADR 0017](../adr/0017-simulation-lod-l1-l2-round-trip.md) §1 and §10.

## 10.4 Agent minds (deterministic, no LLM required)

A layered decision architecture, cheapest at the bottom:

1. **Schedules (all levels L2+).** Each agent has a daily and weekly template (sleep, work, meals, leisure) derived from occupation, age, culture and personal traits. At L2 this is the whole mind: events fire at scheduled times.
2. **Needs and utility AI (L1+).** Needs decay over time; candidate actions are scored by a utility function of needs, traits, opportunities and relationships; the highest-scoring action (with deterministic tie-breaking and seeded noise) is chosen. Utility AI is chosen over behaviour trees as the primary mechanism because it degrades gracefully, is data-driven, and scores can be computed for many agents in bulk.
3. **Plans (L1+).** Multi-step activities (go to shop, buy food, return home) use small hierarchical task networks (HTN) or hand-authored task templates. Full GOAP planning is reserved for L0 if needed.
4. **Social layer (L0, partially L1).** Relationships (affinity, trust, role: family, friend, colleague, rival) stored sparsely; interactions update them; gossip/rumour propagation along the social graph.
5. **Memory (L0, compact at L2).** Agents remember significant events as **references to world events** ([24](24-world-events-and-causality.md)), with personal salience and emotional valence, plus a small set of personal facts; never free text. Memories can influence decisions (avoid the street where they were robbed), and those reads are declared and deterministic.

Each layer is deterministic and testable headless. LLMs, if added, sit *above* this as a presentation or suggestion layer ([16](16-ai-llm-npcs.md)).

## 10.5 Institutions and groups

Not everything that acts is a person. **Collective agents** (households, firms, shops, schools, governments, factions, religions) are entities with their own state and decision rules, operating mostly at L3–L5. They are the natural bridge between individual and macro simulation: a firm's employees may be individuals when materialized or counts when not, but the firm's payroll, stock and reputation are always canonical.

## 10.6 Notability

An entity becomes **notable** (persistent) when any of the following happens:
- The player inspects, names, bookmarks, or interacts with it.
- It is the target of, or significantly affected by, a god intervention.
- It takes part in a significant event (crime, fire, election, founding a business).
- Its state diverges from what its procedural baseline would regenerate in a way the player could notice (it moved house, changed job, got married).
- Design-defined roles (mayor, notable families, characters with story significance).

Notable entities are always at least L2 wherever they are, are stored canonically, and appear in the world event log. A budget caps the number of notables per region; when exceeded, the least-relevant (oldest, least-viewed, never-named) are **retired**: their lasting effects are folded into aggregates, and their individual record and events are compacted to a short "legend" entry (name, life summary, key events), so the event history never references something unresolvable.

## 10.7 Movement and navigation

- **Pedestrians:** navigation over a per-cell navigation graph (sidewalks, crossings, building entrances) built from OSM and NYC data, with procedural fill where the data is missing; local avoidance via a velocity-obstacle method (RVO/ORCA style) for L0/L1 agents. Not rigid bodies.
- **Vehicles:** follow lanes on the road graph with car-following and simple intersection rules at L1; at L2 and coarser, trips are discrete events ("leaves 08:10, arrives 08:32") and traffic is an aggregate flow per road segment.
- **Pathfinding cost control:** hierarchical pathfinding (district graph, then cell graph, then local), with path caching for common origin-destination pairs and a per-tick pathfinding budget.

## 10.8 Performance shape

Rough per-agent memory targets (to be measured): L2 ≈ 64–128 bytes, L1 ≈ 0.5–1 KB, L0 ≈ several KB plus history references. With the budgets in [15](15-ios-performance-memory.md), that allows tens of thousands of L2 agents, a few thousand L1 and tens of L0 agents simultaneously on device.

## 10.9 Agents, events, and provenance

- Agent decisions that matter (changing job, moving house, forming a relationship) emit world events with causes: the event or state that triggered them ([24](24-world-events-and-causality.md)).
- Components changed by an intervention or a major event carry **provenance tags** (bounded, decaying). Decisions that read tagged state pass the tags on to the events they emit, which is how "what did my intervention lead to?" can follow consequences through agents' choices.
- Collective agents (firms, households, institutions) emit events in the same way, and their aggregate state carries attribution for interventions where practical.

## 10.10 Calibration to real data

The synthetic population for an area is generated from Census and American Community Survey tables (age, household size, income bands, occupations, commute modes), assigned to real residential buildings in proportion to their units or floor area. Workplaces are assigned to real commercial and institutional buildings from land use and points of interest; jobs outside the simulated area are reached through edge points (for example the R train stations on 4th Avenue in Bay Ridge). Calibration targets are recorded with the dataset version.
