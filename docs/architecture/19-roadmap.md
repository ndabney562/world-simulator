# 20. Phased Development Roadmap

> **Revision 2 (2026-10-14):** The phases are unchanged, but each one now includes real-data work, and Phase 0 adds Unity/Cesium/Google-tiles spikes. See [22 §R.9](22-real-earth-and-realism.md#r9-updated-roadmap-notes).

Covers point 20: from the first technical prototype through neighbourhood, city, planetary and solar-system simulation.

Each phase has **exit criteria**. A phase is done when its criteria are met on real hardware, not when its features exist. Durations are deliberately not given; effort is shown relatively, and the plan should be re-estimated after Phase 1 with real data.

## Phase 0: Foundations

Goal: lock in the decisions that are expensive to change and build the skeleton they live in.

- Commit the vision / Project Instructions to `docs/vision/`; ChatGPT review of this proposal into `docs/reviews/`; ADRs for every "decide now" item ([21](21-decisions-now-vs-later.md)).
- Repository skeleton, CI (Linux + macOS + iOS build), conventions file for AI collaborators.
- `ws-math` (frames, typed positions, cube-sphere cell IDs), `ws-time`, `ws-rng`, with property tests.
- Spikes (throwaway, documented): ECS library choice; serialization format; Rust-to-Swift bridge with shared Metal buffers; minimal Metal renderer drawing 10,000 instanced boxes from core-written buffers on device; measure memory limit and thermal behaviour on the minimum device.
- `ws-cli` skeleton and determinism-hash harness.

Exit criteria: all "decide now" ADRs accepted; CI green; an iOS build running the core on device and drawing core-provided instances at a stable frame rate; determinism harness in place.

Relative effort: small-medium.

## Phase 1: Technical prototype ("one living street")

Goal: prove the architectural spine end to end at the smallest scale.

- One sector cell, flat-ish terrain, procedurally generated street, a handful of buildings (simple descriptors).
- ~100 agents with schedules (L2) and needs/utility (L1), pathfinding and local avoidance.
- Two simulation LOD levels (L1 ⇄ L2 ⇄ L3 for buildings) with real materialization/dematerialization and the observation cache.
- Command queue and journal; one intervention end-to-end.
- Save/load with versioned format.
- Camera from street to ~500 m with two rendering LOD representations for buildings and people.
- Inspector (tap an agent, see state).

Exit criteria: round-trip conservation tests pass over thousands of seeds; same-build determinism verified; sustained frame rate on the minimum device for 20 minutes; save/load reproduces identical state. **Re-evaluation gate**: is the custom renderer cost acceptable, or should the front end switch to Unity over the same core ([02](02-technology-stacks.md))?

Relative effort: medium.

## Phase 2: Neighbourhood (the MVP phase)

Goal: the first playable MVP ([20](20-mvp.md)).

- Several blocks (~0.5–1 km²), several hundred residents, workplaces, shops, a school, a park.
- Day/night, weather (rain) affecting behaviour; households and simple household economy.
- Notability and persistence of notables; history log; three interventions; time controls.
- Rendering: street to neighbourhood band, cross-fades, atmosphere, stylized look established.

Exit criteria: MVP criteria in [20](20-mvp.md) met; TestFlight build in testers' hands.

Relative effort: medium-large.

## Phase 3: City

Goal: tens of districts and a city of 100,000+ people, almost all at aggregate levels, with full zoom from street to city band.

- L3–L5 models: district cohorts, commuting and migration flows, city economy and services, simple governance.
- Settlement layout generator (roads, districts, zoning), building grammar at scale, HLOD.
- Interest manager with budgets across many districts; streaming of cells; visual proxies (crowd density, traffic flow) for aggregate districts.
- Vehicles (L1 lane-following near camera, flows elsewhere).
- Interventions at district and city scale (fire spreading, epidemic, economic boom/bust).
- Statistical-equivalence tests between levels become a CI requirement.

Exit criteria: a 20-minute flight from a street to whole-city view and back with no visible holes, no conservation violations, sustained frame rate, save size within budget after hours of play.

Relative effort: large.

## Phase 4: Planet

Goal: one Earth-like planet with many settlements, terrain from orbit to street, climate and biomes.

- Cube-sphere terrain LOD, atmosphere scattering, oceans, clouds.
- Planet generation (macro elevation, climate zones, biomes, rivers) and pre-history simulation (settlements, cultures).
- L6 models: climate, inter-city trade, macro-demography, factions.
- Time warp coupled with LOD (days to years per second at planet scale).
- Planet-scale interventions (climate change, disasters, plagues across continents).

Exit criteria: continuous zoom from orbit to any street on the planet; centuries of history simulated at warp without collapse of invariants; memory and thermal budgets met.

Relative effort: very large.

## Phase 5: Solar system

Goal: the planet within a solar system of bodies on analytic orbits, with continuous zoom out to system scale.

- Full frame hierarchy in use; Kepler orbits; moons; system-scale rendering (multiple depth ranges, sprites with minimum size, orbit lines, star field).
- Celestial interventions with parametric outcome models (impacts, orbit changes, tides).
- Optional: other bodies with macro-only settlement models.

Exit criteria: smooth zoom from a street to the outer system and back with precision artefacts absent at every scale (no jitter, no z-fighting), and celestial interventions propagating coherently to planetary and city levels.

Relative effort: medium (most of the hard work is in Phases 1–4).

## Phase 6 and beyond: depth

- Richer social, cultural and political models; institutions; long-term history and legends.
- Optional LLM features ([16](16-ai-llm-npcs.md)): dialogue text for notables, natural-language interventions.
- Interiors in depth, more building and culture styles, audio.
- Branching timelines, replays, world sharing.
- Additional platforms (iPad and Mac via the same Swift/Metal app, Android only with a different front end).

## Cross-cutting tracks (all phases)

- Performance measurement on device every phase.
- Documentation and ADRs kept current; `docs/architecture/` updated when reality diverges from this proposal.
- Tooling (`ws-inspector`, debug overlays) grows with every simulation feature.
