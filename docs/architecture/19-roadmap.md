# 20. Phased Development Roadmap

Covers point 20. Revised in Revision 4 to the progression agreed after ChatGPT's review (ADR 0009):

> **Technical Prototype → Vertical Slice → Neighbourhood MVP → City → Planet → Solar System**

Each stage has **exit criteria**. A stage is done when its criteria are met on real hardware (iPhone 16 Pro, ADR 0037), not when its features exist. Durations are deliberately not given; effort is shown relatively, and the plan is re-estimated after the Technical Prototype with real data. Stage definitions for the first three stages are in [20](20-mvp.md).

## Stage 1: Technical Prototype ("MVP-0")

**Purpose: prove the foundational architecture only.** No real gameplay, no visual polish, no real-data rendering required.

- Repository skeleton, CI (Linux core, macOS core, Unity iOS build), conventions file for AI collaborators, ADR process in use.
- Rust foundations: frames and typed positions (WGS84/ECEF, cell IDs, local ENU), integer time, counter-based RNG, ID scheme, minimal ECS wrapper, command queue and journal, minimal event log with causal links, determinism tiers and state hashing ([25](25-determinism-policy.md)).
- Rust ↔ Unity bridge on iPhone: the core as a native static library, commands in, render snapshot out through shared buffers, with no per-entity calls.
- A synthetic test scene placed at real Bay Ridge coordinates: a flat cell, a few boxes as "buildings", and a few dozen moving agents. Unity draws them with Cesium georeferencing.
- Headless `ws-cli` runner; save and load of the trivial world; one headless LOD round trip (L1 ⇄ L2) with conservation checks.
- Spikes, each documented: ECS library choice; serialization format; Cesium plus open-data geometry on device; Google tiles through the provider abstraction with quota enforcement ([23](23-visual-providers.md)); memory and thermal limits on an iPhone 16 Pro.

Exit criteria: a Tier 1 and Tier 2 determinism harness running in CI; the iPhone build runs the core and draws core-provided instances at a stable frame rate for 20 minutes; save/load round-trips state hashes; all spikes written up as ADRs or notes.

Relative effort: small to medium.

## Stage 2: Vertical Slice (about one real Bay Ridge block)

**Purpose: run every architectural spine end to end, at the smallest real scale.** Visual polish is not a requirement.

Scope (as specified by Noah; details in [20 §20.3](20-mvp.md)): about one real Bay Ridge block; about 10–30 synthetic people; 3–5 representative buildings built from real data; basic camera and zoom; Rust ↔ Unity communication; simulation time; at least one simulation-LOD transition; save/load; entity inspection; and one god intervention whose consequences are recorded as linked events.

Exit criteria: in [20 §20.3](20-mvp.md).

Relative effort: medium.

## Stage 3: Neighbourhood MVP (6–12 Bay Ridge blocks)

**Purpose: the first playable version**, testing whether the core fantasy (a living, real neighbourhood you can observe and influence) is engaging.

Scope: in [20 §20.4](20-mvp.md). Summary: 6–12 blocks around 3rd–5th Avenue and 75th–86th Street (bounds to be confirmed); hundreds to a few thousand synthetic residents (L0–L3); day and night with the real sun position; weather; three interventions with causal-consequence views; history and "what happened here?"; realistic open-data visuals; Google backdrop optional.

Exit criteria: in [20 §20.4](20-mvp.md); TestFlight build in testers' hands (requires the Apple developer membership, the one known cost in ADR 0007).

Relative effort: medium-large.

## Stage 4: City (New York City, starting with Brooklyn)

- Data pipeline for a whole metro area (NYC Open Data citywide, OSM/Overture, Census/ACS).
- L3–L5 models: district cohorts, commuting and migration flows, city economy and services, simple governance; subway and road networks as flow networks.
- Interest manager with budgets across many districts; streaming of simulation tiles from a free-tier host; visual proxies for aggregate districts.
- Vehicles: lane-following near the camera, flows elsewhere.
- City-scale interventions (fire spreading, epidemic, economic shock) with aggregate causal attribution ([24](24-world-events-and-causality.md)).
- Statistical-equivalence tests between levels become a CI requirement.

Exit criteria: a 20-minute flight from a Bay Ridge street to a whole-city view and back with no visible holes and no conservation violations; sustained frame rate; save size and event store within budget after hours of play; "why did this district decline?" answered from events.

Relative effort: large.

## Stage 5: Planet (real Earth)

- A global open-data pipeline (Overture, Copernicus DEM, GHSL/WorldPop, ESA WorldCover) staged region by region; synthetic populations for every settlement.
- Globe rendering through Cesium with the open-data provider; NASA imagery from orbit; optional Google backdrop.
- L6 models: climate zones, inter-city trade, macro-demography, factions.
- Time warp coupled with LOD (days to years per second).
- Planet-scale interventions and causal attribution across regions.

Exit criteria: continuous zoom from orbit to any covered street; decades of history simulated at warp with invariants holding; memory and thermal budgets met; hosting still within free tiers, or a new cost decision taken (ADR 0007).

Relative effort: very large.

## Stage 6: Solar System

- Real ephemerides (JPL) for planets and moons, deterministic evaluation; the full frame hierarchy in use.
- System-scale rendering (depth ranges, minimum-size sprites, orbit lines) with NASA imagery providers.
- Celestial interventions with parametric outcome models whose consequences propagate as linked events to planet and city levels.

Exit criteria: smooth zoom from a Bay Ridge street to the outer solar system and back with no precision artefacts; celestial interventions propagating coherently, with traceable causal chains.

Relative effort: medium (most of the hard work is in earlier stages).

## Beyond: depth

Richer social, political, and cultural models; optional LLM features ([16](16-ai-llm-npcs.md)); interiors in depth; audio; branching timelines; replays; iPad and Mac builds.

## Cross-cutting tracks (all stages)

- Device performance measurement every stage.
- ADRs and `docs/architecture/` kept current; git history records each revision.
- Tooling (`ws-inspector`, debug overlays, event and causality browser) grows with every simulation feature.
- Licence and attribution compliance for every new data source and provider.
