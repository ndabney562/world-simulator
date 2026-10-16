# World Events, History, and Causal Provenance

The initial TP-008 record subset, sequential 64-bit ID assignment in deterministic commit order, causal validation and in-memory query contract are proposed in [ADR 0019](../adr/0019-command-journal-and-world-events.md). This does not implement persistence, retention or provenance propagation.

Revision 4 addition (ChatGPT review items 5 and 6). Related: [04](04-simulation-architecture.md), [08](08-world-state.md), [09](09-entities-and-agents.md), [13](13-god-interventions.md), [14](14-persistence.md), [16](16-ai-llm-npcs.md), ADRs 0011 and 0012.

## 24.1 Why a first-class event system

Many parts of the design need the same thing: a structured record of **what happened, when, where, to whom, and why**:
- NPC memories ("my shop burned down last spring").
- History summaries ("Bay Ridge, 2026–2031").
- Statistics (births per month, fires per year).
- Causal explanations ("why did this city decline?").
- The consequences of god interventions.
- Inspector timelines.
- LLM dialogue context.
- Tests that check what happened.

Building each separately would produce inconsistent stories. Instead there is **one world event system**, and every consumer reads from it.

## 24.2 Journal versus event log

These are two distinct, complementary records:

| | Command journal | World event log |
|---|---|---|
| Records | **Inputs**: what was asked of the world (player commands, interventions, adopted LLM suggestions, settings) | **Outputs**: what happened in the world (births, fires, job changes, the effects of interventions) |
| Used for | Replay, undo, bug reproduction | History, memory, statistics, explanations, UI, LLM context, tests |
| Completeness | Complete: every input is recorded | Selective: meaningful events only, with retention by importance |
| Can be regenerated? | No | Partly, by replaying the journal on the same build ([25](25-determinism-policy.md)), but in practice it is stored |

An applied intervention appears in both: as a command in the journal and as an `intervention.applied` event in the event log, which is the root of its causal tree.

## 24.3 The event record

Each event is a structured, versioned record:

| Field | Meaning |
|---|---|
| **Event ID** | 64-bit, unique per world, assigned deterministically at commit as (tick, system order, sequence within the system). It is stable across save and load. |
| **Simulation timestamp** | Integer simulation time ([07](07-coordinates-and-time.md)). Long processes have a start and an end, or an "ongoing" status. |
| **Location** | Cell ID at the appropriate level, plus an optional precise position, plus the semantic node path (planet, region, settlement, district, building). Celestial events use a frame and body. |
| **Actors** | A list of (entity ID, role) pairs, with roles such as subject, object, agent, victim, beneficiary, witness, or institution. Entities can be individuals, groups, buildings, or nodes. |
| **Event type** | A hierarchical, versioned taxonomy (24.4), with a typed payload per type. |
| **Cause / provenance** | Causal links to earlier events (24.6), plus an **origin**: the simulation rule or system that produced it, an intervention ID, a data import, an adopted LLM suggestion, or materialization sampling. |
| **Effects** | A compact summary of the state changes the event made: entity component changes, aggregate deltas, created or removed entities. Not a full diff. |
| **Importance** | A score from 0 to 1 computed when the event is created, from the type's base importance, scale (how many entities or how much value is affected), the notability of the actors, player relevance (watched, bookmarked, intervention-related), and rarity. Retention, summaries, and memory selection use it. |
| **Related events** | Non-causal links: part-of (a sub-event of a larger event), follows-up, same-episode, aggregated-into. |
| **LOD and aggregation** | The simulation level the event was produced at; whether it is an **individual** event or an **aggregate** event (for example "14 births in this district during March"); and, for aggregate events, the count and distribution summary. |
| **Schema version** | For payload migration. |

## 24.4 Event taxonomy (initial)

| Domain | Example types |
|---|---|
| `life` | birth, death, illness, recovery, ageing milestone |
| `social` | relationship formed, relationship changed, marriage, separation, friendship, conflict, move in, move out |
| `work` | hired, fired, retired, business opened, business closed, promotion |
| `built` | building constructed, damaged, repaired, demolished, change of use |
| `disaster` | fire, flood, storm damage, earthquake, epidemic outbreak |
| `economy` | price shock, boom, recession, bankruptcy, trade-flow change |
| `politics` | election, law passed, protest, governance change |
| `ecology` | drought, bloom, species decline, land-cover change |
| `climate` / `weather` | heatwave, cold snap, notable storm |
| `celestial` | impact, orbit change, eclipse (natural and intervention-caused) |
| `player` | intervention applied, intervention expired, entity inspected or named (used as a relevance signal) |
| `meta` | materialization decided a specific fact (sampling provenance), notable retired into a legend |

Types are data plus code: each type defines its payload schema, base importance, the levels it can occur at, and its aggregation rule (how many individual events of this type roll up into one aggregate event at coarser levels).

## 24.5 Producing events

- Systems emit candidate events into a per-tick buffer. At the tick boundary they are committed in a deterministic order, IDs are assigned, and indices are updated.
- **Not every state change is an event.** A person walking is not an event; a person moving house is. Each domain defines what is meaningful. Very high-frequency happenings become aggregate events.
- Events at coarse levels are usually aggregate events. When a node is materialized, a coarse fact can be **refined**: the aggregate event "a fire occurred in this block" gains a child individual event naming the building and the victims, linked by `part-of`, with `meta` provenance saying the specifics were sampled at materialization ([12](12-materialization.md)).

## 24.6 Causal provenance: a practical model

The aim is **useful, explainable causal links**, not a universal causal-inference engine. Four mechanisms, from most to least precise:

**1. Direct causation (exact).** When a system creates an event because of a specific earlier event, it records the link explicitly: the fire caused these injuries; the injury caused this job loss; the intervention caused the rain. Link types: `caused_by` (direct), `enabled_by` (precondition), `triggered_by` (immediate trigger).

**2. Provenance tags on state (approximate, bounded).** State that an important event changed carries a small **provenance set**: a bounded list of (root event ID, weight). For example, a person cursed with illness, an environmental field changed by an intervention, or a building damaged by a fire. When a system produces a new event while reading tagged state (an agent chooses to move away while their `wealth` component carries tag I), the new event lists I as `contributed_to` with a derived weight.
- Weights decay with each hop and with time. Below a threshold the tag is dropped.
- Sets are capped (for example the top 4 causes by weight), so cost is bounded.
- Only roots worth tracking get tags: player interventions always, plus high-importance events (disasters, major economic shocks).
- This is the mechanism behind most answers to "what consequences resulted from my intervention?".

**3. Aggregate attribution (statistical).** Coarse models (L3+) track intervention-attributable components separately where practical. The epidemic model counts infections descending from intervention-seeded cases separately from background cases; the economy model keeps an attributable delta for shocks. Aggregate events carry these attributions, for example "population decline 2028: 40% attributable to the plague intervention, 35% to the factory closure event, 25% background trend".

**4. Counterfactual comparison (optional, expensive).** Because the simulation is reproducible on the same build ([25](25-determinism-policy.md)), the game can, on request and in the background, branch from the snapshot before an intervention and run without it at a coarse level. Comparing the two branches estimates the intervention's total effect. It is used sparingly, for summary screens, and never for per-event links.

Each causal claim carries its **confidence class**: `direct`, `contributing`, `statistical`, or `counterfactual`. The UI and LLM layer must present them differently: "caused" versus "contributed to" versus "is associated with".

## 24.7 Answering the three example questions

| Question | How it is answered |
|---|---|
| "What happened here?" | Query events by location (cell and its sub-cells) and time window, sort by importance, group by episode (`part-of` links), and summarise. Aggregate events cover periods when the area was simulated coarsely. |
| "Why did this city decline?" | Find the aggregate decline events or indicator trends for the city, follow their aggregate attributions and causal links back through a bounded depth, then rank root causes by weight and confidence. |
| "What consequences resulted from my intervention?" | Start at the `intervention.applied` event and traverse the reverse index of causal links (`caused_by`, `contributed_to`) forward through a bounded depth, plus the aggregate attributions that name it, plus the counterfactual summary if one was computed. |

These queries run over structured data. An LLM may later phrase the answer ([16](16-ai-llm-npcs.md)), but the content comes from the event system.

## 24.8 Storage, indexing, and retention

- **Append-only store** in the save's SQLite database ([14](14-persistence.md)), partitioned by time and region.
- **Indices** by actor, by location (Hilbert-ordered cell IDs, so region queries are range scans), by type, by time, and a **reverse causal index** (from cause to effects).
- **Retention by importance:**
  - Kept forever: high-importance events, events involving notables, intervention roots, and every event on a causal chain from an intervention, up to a depth and weight limit.
  - Rolled up after a window: low-importance individual events are compacted into aggregate events (counts and distributions per type, region, and period), keeping the aggregate's causal attributions.
  - When a notable is retired ([09](09-entities-and-agents.md)), their events are summarised into a "legend" record.
- Budgets per region and period keep the store bounded. Compaction is deterministic.

## 24.9 Consumers

| Consumer | What it reads |
|---|---|
| NPC memory | Each agent holds references to events (IDs), plus personal salience and emotional valence; not copies or free text. |
| History summaries | Importance-filtered queries per region and period, grouped by episode. |
| Statistics and UI charts | Aggregate events and counts, the same numbers the simulation uses. |
| Causal explanations | Causal links, provenance tags, and aggregate attributions (24.6). |
| Inspector timelines | Per-entity and per-building event lists. |
| LLM dialogue and narration | Structured event facts serialised into prompts; never free-form memories ([16](16-ai-llm-npcs.md)). |
| Tests | Scenario assertions ("after the intervention, at least one `disaster.fire` event links to it") and invariant checks (no event references a non-existent entity; every causal link points backwards in time). |

## 24.10 Invariants

1. Causal links always point to earlier or simultaneous events (no cycles).
2. Every actor and location referenced by a retained event can be resolved, as a live entity or a retired-entity legend.
3. Aggregate events equal the sum of the individual events they replaced (counts by type, region, and period).
4. Event IDs are unique and stable across save and load.
5. The event log never influences the simulation except through declared reads: agent memory references, and systems that explicitly query recent events, such as "avoid the street where the fire was". These reads are deterministic.
