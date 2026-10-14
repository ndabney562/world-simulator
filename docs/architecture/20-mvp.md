# 21. Technical Prototype, Vertical Slice, and Neighbourhood MVP

Covers point 21. Revised in Revision 4 (ADR 0009): the first implementation target is now the **Technical Prototype**, followed by a **Vertical Slice** of about one Bay Ridge block. The 6–12-block **Neighbourhood MVP** is kept as the first *playable* release, not the first implementation target. The roadmap context is in [19](19-roadmap.md).

## 21.1 Principle

Each stage is a **vertical slice of the architecture, not of the content**. They all use the real coordinate system, the real LOD and materialization pipeline, the real command journal and event log, and the real persistence format, so that everything built later extends them rather than replaces them.

## 21.2 Stage 1: Technical Prototype ("MVP-0")

**Goal:** prove the foundational architecture only.

| Proves | How |
|---|---|
| Rust core builds and is tested on Linux and macOS, and runs on iPhone inside Unity | CI for all three; native plugin static library |
| Commands-in, snapshots-out boundary | Unity sends camera and commands; the core fills shared buffers |
| Coordinates | Entities at real Bay Ridge WGS84 coordinates, cell-local offsets, camera-relative rendering through Cesium georeferencing |
| Time | Integer simulation time with pause and speed controls |
| Determinism tiers | Tier 1 golden tests (IDs, procedural decisions) identical on Linux and macOS; Tier 2 replay hash equality |
| Journal and event log | Commands journaled; events with IDs and causal links; queried by a test |
| LOD round trip | Headless L1 ⇄ L2 transitions with conservation checks |
| Persistence | Save and load of the toy world; schema version present |
| Provider abstraction | Open-data provider and a quota-capped Google provider switchable at runtime, with the core state hash unchanged |

Not in scope: real buildings, real population calibration, gameplay, visual quality.

## 21.3 Stage 2: Vertical Slice (about one real Bay Ridge block)

**Goal:** every architectural spine working together on real data at the smallest useful scale. Visual polish is **not** a requirement.

### Scope

| Area | Scope |
|---|---|
| Area | About one real Bay Ridge block, for example one block face pair between 3rd and 4th Avenues; exact block chosen in the Technical Prototype |
| Buildings | 3–5 representative real buildings from NYC footprints and the 3D building model (for example rowhouses, a small apartment building, a corner shop), as plain but correctly placed geometry; surroundings from the open-data provider or simple massing |
| People | About 10–30 synthetic residents and workers with deterministic identities, households, homes, a workplace or school (possibly "off-map", leaving through an edge point), and daily schedules |
| Agent behaviour | L2 schedules for all; L1 needs and utility decisions with pathfinding on the block's sidewalks when the camera is close |
| Simulation LOD | At least one real transition: residents switch between L1 and L2 (and building aggregates at L3) as the camera moves in and out, with conservation checks |
| Camera | Basic orbit, pan, and pinch-zoom from street level to a few hundred metres |
| Rust ↔ Unity | The full snapshot and command boundary in use |
| Time | Simulation time running, with real date and sun position, pause, 1×, 10×, and 100× |
| Persistence | Save and load of the slice, including notables, deltas, and the event log |
| Inspection | Tap a person or a building to see identity, schedule, current activity, needs, and recent events |
| God intervention (1) | One intervention, for example **"Curse with illness"** on a resident. It must produce linked events (the illness, missed work, a neighbour visiting) whose causal links back to the intervention are visible in the inspector |
| Events | Births, deaths, and the like are not required at this scale; schedule-level events (left for work, went shopping) and intervention consequences are |

### Exit criteria

1. Zooming in and out repeatedly never loses or duplicates a person; re-materialized people are consistent with their schedules (or exactly as left, within the observation-cache window).
2. The block's aggregates equal the aggregation of individuals at all times (automated check).
3. Save, quit, and load reproduce the same state hash (Tier 2).
4. The intervention's consequences are listed by a "consequences of this intervention" query, with confidence classes ([24](24-world-events-and-causality.md)).
5. Toggling the Google backdrop on or off does not change the state hash.
6. Stable frame rate on the iPhone 15 Pro for 20 minutes (any reasonable frame rate; polish is not the goal).

## 21.4 Stage 3: Neighbourhood MVP (6–12 Bay Ridge blocks)

**Goal:** the first playable version. This is the original MVP concept, preserved.

### Scope

| Area | Scope |
|---|---|
| Area | 6–12 real Bay Ridge blocks, proposed around 3rd–5th Avenue and 75th–86th Street (bounds to be confirmed with Noah), including the 3rd and 5th Avenue commercial corridors and the R train on 4th Avenue as commuter entry and exit points |
| Data | NYC 3-D Building Model, NYC footprints, 2017 LiDAR, orthoimagery, OSM (streets, sidewalks, shops), Census/ACS for population calibration; processed and shipped inside the app |
| Population | Synthetic residents calibrated to Census density and demographics: likely a few thousand in this area, with most at L2/L3 at any moment and hundreds at L1/L0 near the camera |
| Agent behaviour | Schedules, needs and utility AI, simple social interactions (greetings, friendships), households and simple household budgets, shops and workplaces |
| Simulation LOD | L0–L3 with materialization, the observation cache, and notability |
| Visuals | Realistic open-data visuals (PBR buildings from real massing and footprints, orthoimagery ground, real sun position, weather effects); optional Google backdrop beyond about 1 km, capped below the free allowance |
| Camera | Street level to about 1–2 km altitude |
| Time | Day and night, pause, 1×, 10×, 100× |
| Weather | Clear and rain, as an environmental field that changes behaviour |
| God interventions (3) | **Make it rain** (an environment field over an area and a duration); **spawn or remove a person** (existence, integrated with households); **bless or curse a person** (wealth, health, or mood), each with traceable consequences |
| History | "What happened here?" for any building or block; per-person life timelines; a neighbourhood summary from aggregate events |
| Inspector | Person, building, and neighbourhood panels using the same aggregates the simulation uses |
| Persistence | Autosave and manual save/load; notables persisted; event store with retention |

### Explicitly out of scope

Other neighbourhoods; city, planet, and space scales; vehicles beyond parked props (or none); rigid-body physics; fire or disasters; economy beyond households, shops, and wages; interiors; LLM features; iCloud; undo as a player feature (supported internally).

### The player loop

1. Watch a real Bay Ridge neighbourhood wake up, commute, work, shop, meet, and sleep.
2. Zoom in on someone, learn who they are, and follow them.
3. Intervene and watch the consequences ripple through schedules, relationships, and neighbourhood statistics.
4. Ask "what happened here?" and "what did my intervention do?" and get answers from the event history.
5. Zoom out and back in and find the world exactly as consistent as you left it.
6. Save, quit, return later, and find your notable people still living their lives.

### Acceptance criteria

1. All Vertical Slice criteria hold at neighbourhood scale.
2. A 30-day headless run in `ws-cli` finishes with every invariant holding (conservation, event invariants, ID uniqueness). The Linux and macOS arm64 runs agree on all Tier 1 checks and stay within the agreed Tier 3 drift tolerances ([25](25-determinism-policy.md)).
3. Each intervention has a scenario test covering materialized and non-materialized targets and its expected linked consequence events.
4. Sustained target frame rate and memory within budget for 20 minutes on an iPhone 15 Pro, with the Google backdrop both on and off.
5. A tester who has never seen the project can identify, without help, at least one consequence of an intervention they made, using the in-game history.
