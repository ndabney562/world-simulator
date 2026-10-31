# World Simulator: Technical Architecture Proposal

Status: **Accepted direction, under review** (no implementation yet)
Current revision: **4** (2026-10-14), after ChatGPT's independent review.
Authors: Claude (Anthropic) for Noah; reviewed by ChatGPT.

This directory is the architecture and technical plan for World Simulator: a continuous-scale simulation of **real Earth**, from a single Bay Ridge street to the solar system, on **iPhone**. It contains no implementation code. Its job is to make the hard decisions explicit, define a foundation that can realistically grow from a one-block prototype to solar-system scale, and be honest about what must be approximated.

## Decisions so far

| ADR | Decision |
|---|---|
| [0001](../adr/0001-real-earth.md) | The world is real Earth: open data for simulation, synthetic people |
| [0002](../adr/0002-realistic-visuals.md) | Realistic visuals, within iPhone limits |
| [0003](../adr/0003-single-player.md) | Single-player |
| [0037](../adr/0037-minimum-device-iphone-16-pro.md) | Minimum device: iPhone 16 Pro (supersedes [0004](../adr/0004-minimum-device-iphone-15-pro.md)) |
| [0005](../adr/0005-stack-rust-core-unity-cesium.md) | Rust simulation core → Unity 6 / URP → Cesium for Unity → iPhone |
| [0006](../adr/0006-mvp-area-bay-ridge.md) | First area: Bay Ridge, Brooklyn |
| [0007](../adr/0007-zero-cost-policy.md) | Zero-cost policy |
| [0008](../adr/0008-visual-backdrop-providers.md) | Visual backdrops (Google, open data, NASA) are optional, swappable providers, never simulation state |
| [0009](../adr/0009-stage-progression.md) | Technical Prototype → Vertical Slice → Neighbourhood MVP → City → Planet → Solar System |
| [0010](../adr/0010-tiered-determinism.md) | Tiered reproducibility instead of strict cross-platform bit-identical determinism |
| [0011](../adr/0011-world-event-history-system.md) | A first-class world event and history system |
| [0012](../adr/0012-causal-provenance.md) | Practical causal provenance for interventions and major events |

## How to read this

Each file maps to one or more of the 22 points in the original brief; files 22–25 cover topics added during review.

| # | Topic | File |
|---|-------|------|
| 1 | Hardest engineering problems, and a reality check | [01-hard-problems.md](01-hard-problems.md) |
| 2, 3 | Technology stacks and the chosen stack | [02-technology-stacks.md](02-technology-stacks.md) |
| 4 | Rendering architecture | [03-rendering-architecture.md](03-rendering-architecture.md) |
| 5 | Simulation architecture | [04-simulation-architecture.md](04-simulation-architecture.md) |
| 6 | Simulation LOD | [05-simulation-lod.md](05-simulation-lod.md) |
| 7 | Rendering LOD | [06-rendering-lod.md](06-rendering-lod.md) |
| 8 | Coordinate system and time | [07-coordinates-and-time.md](07-coordinates-and-time.md) |
| 9 | World-state architecture | [08-world-state.md](08-world-state.md) |
| 10 | Entities and agents | [09-entities-and-agents.md](09-entities-and-agents.md) |
| 11 | Physics | [10-physics.md](10-physics.md) |
| 12 | Data ingestion and procedural generation | [11-procedural-generation.md](11-procedural-generation.md) |
| 13 | Materialization / dematerialization | [12-materialization.md](12-materialization.md) |
| 14 | God interventions (including causal provenance) | [13-god-interventions.md](13-god-interventions.md) |
| 15 | Persistence and saves | [14-persistence.md](14-persistence.md) |
| 16 | iPhone performance and memory | [15-ios-performance-memory.md](15-ios-performance-memory.md) |
| 17 | AI / LLM augmentation of NPCs | [16-ai-llm-npcs.md](16-ai-llm-npcs.md) |
| 18 | Repository structure | [17-repository-structure.md](17-repository-structure.md) |
| 19 | Testing strategy | [18-testing-strategy.md](18-testing-strategy.md) |
| 20 | Staged roadmap | [19-roadmap.md](19-roadmap.md) |
| 21 | Technical Prototype, Vertical Slice, Neighbourhood MVP | [20-mvp.md](20-mvp.md) |
| 22 | Decided, decide-now, and postponed decisions | [21-decisions-now-vs-later.md](21-decisions-now-vs-later.md) |
| — | Real Earth: data sources, licensing, realism, cost | [22-real-earth-and-realism.md](22-real-earth-and-realism.md) |
| — | Visual backdrop provider abstraction | [23-visual-providers.md](23-visual-providers.md) |
| — | World events, history, and causal provenance | [24-world-events-and-causality.md](24-world-events-and-causality.md) |
| — | Determinism and reproducibility policy | [25-determinism-policy.md](25-determinism-policy.md) |
| — | Glossary | [glossary.md](glossary.md) |

## Executive summary

**The core insight.** "Continuous scale from street to solar system" is not one simulation running at every scale. It is a hierarchy of different simulations, each valid at its own scale, with a disciplined contract for moving information up (aggregation) and down (materialization) between them. The world is fully detailed only where someone is looking or something important is happening. Everywhere else it is statistics, schedules, and a deterministic baseline (real open data plus synthetic people) that can re-create plausible detail on demand. Getting that contract right is the project.

**Stack.** A headless **Rust simulation core** (built and tested on Linux and macOS, shipped to iOS as a static library) drives a **Unity 6 / URP** front end, with **Cesium for Unity** for georeferencing and 3D Tiles where useful. The core owns all world state. Unity only draws snapshots and sends commands.

**Real Earth, legally and for free.** Simulation data comes from open sources (the NYC 3-D Building Model, footprints, LiDAR, and orthoimagery; OpenStreetMap/Overture; Census/ACS; NASA/JPL). People are synthetic. **Google Photorealistic 3D Tiles are an optional, capped visual backdrop** behind a provider abstraction. Switching providers never changes simulation state.

**Explainable history.** A first-class **world event system** records what happened, where, to whom, and why, with causal links. It feeds NPC memories, history views, statistics, intervention consequences ("what did my intervention do?"), and later LLM dialogue. Causal provenance is practical, with stated confidence, not a universal causal-inference engine.

**Reproducibility, by tier.** Exact where identity and structure depend on it, replay-exact on the same build and platform, and bounded differences acceptable elsewhere.

**Staging.** A **Technical Prototype** proves the foundations; a **Vertical Slice** of about one Bay Ridge block (10–30 people, 3–5 buildings, one intervention) runs every spine end to end without visual polish; the **Neighbourhood MVP** (6–12 Bay Ridge blocks) is the first playable version; then **City**, **Planet**, and **Solar System**.

## Inputs and remaining gaps

- The original Project Instructions text was not available to the session that wrote the first draft. The vision as understood is reflected in the decisions above. Committing the Project Instructions to `docs/vision/` is still recommended.
- The repository currently has only the architecture branch, no `main`. Creating `main` from the accepted proposal is the first step of the Technical Prototype.
