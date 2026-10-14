# 1. The Hardest Engineering Problems (and a Reality Check)

Covers point 1 of the brief, plus the explicit challenge to unrealistic assumptions that the brief requested.

The span of the vision is roughly **15 orders of magnitude in space** (about 1 cm of hand-scale interaction to about 7.5 × 10¹² m, the Kuiper belt) and **about 10 orders of magnitude in time** (sub-second physics steps to centuries of history and orbital periods). No single data structure, numeric type, simulation model, or renderer is valid across that range, and every hard problem below follows from that. The world is real Earth (ADR 0001) with realistic visuals (ADR 0002) on iPhone (ADR 0004), at zero cost (ADR 0007), which adds data and licensing problems on top.

## 1.1 The hardest problems, ranked

### P1. Macro–micro consistency (the defining problem)

The world must tell the same story whether you look at it as 4,000 individual residents of Bay Ridge or as "District: population 4,000, unemployment 6%, mood: tense". Zooming out must collapse detail into aggregates without losing facts the player can check. Zooming back in must regenerate detail consistent with (a) the aggregates, (b) everything the player has already seen, and (c) everything that happened while nobody was looking.

Why it is hard:
- Information is genuinely lost when individuals become statistics; we must decide explicitly which information survives (notability, [09](09-entities-and-agents.md); the event history, [24](24-world-events-and-causality.md)).
- Emergent micro effects are not captured by macro models unless we design for it.
- Aggregates and individuals drift apart unless each LOD level has defined aggregation and disaggregation functions and conservation is tested ([05](05-simulation-lod.md)).

This is closer to multi-scale modelling in computational science than to anything game engines provide, and it is the main reason the simulation lives in its own core ([02](02-technology-stacks.md)).

### P2. Numeric precision across 15 orders of magnitude

32-bit floats (what GPUs and engines use) have 0.5 m spacing at Earth's radius and 16 km at 1 AU. 64-bit floats suffice within one frame but are slow or unsupported on GPUs. Depth buffers face the same range problem. Solution: a frame hierarchy, 64-bit math per frame, cell-relative 32-bit offsets, and camera-relative GPU data ([07](07-coordinates-and-time.md)).

### P3. Simulating the unobserved world cheaply but credibly

Earth's population cannot be simulated individually. Unobserved regions must advance with coarse models, sometimes lazily ("simulate on read"), while still producing coherent history. The cheapness/credibility trade-off has to be tuned per domain (population, economy, ecology, weather), and that tuning is a long-running design task.

### P4. Time-scale coupling

Street-level agents need ticks of 0.1–1 s; orbits need years per second. Fine levels cannot run at high time warp, so **time warp and simulation LOD are coupled**: speeding up time demotes fine levels just as zooming out does.

### P5. Real-Earth data at planetary scale, on a phone, for free

Real geography needs large datasets (building footprints and heights, terrain, roads, population). They must be legally usable for simulation, which rules out Google data ([22](22-real-earth-and-realism.md)). They must be processed by our own pipeline, versioned so saves stay valid, and delivered within app-size and zero-cost limits. Data quality varies hugely by region; New York City is unusually good, which is why the MVP is in Bay Ridge.

### P6. Realistic rendering continuity from 1 m to 10¹² m on a phone

Continuous zoom requires depth handling across ranges, terrain and building streaming from planet to street, an atmosphere that looks right from the ground and from orbit, cities that read as buildings, then blocks, then lights, and people that degrade from animated characters to density fields. Realism adds a hard extra problem: blending photoreal backdrops (optional Google tiles) with our own editable geometry ([23](23-visual-providers.md)).

### P7. Mobile constraints: memory, thermals, storage, battery

iOS terminates apps that exceed their memory limit without warning. iPhones throttle under sustained load, so sustained performance after 15–20 minutes is what counts. App size limits the data we can ship. Every budget must be explicit and enforced ([15](15-ios-performance-memory.md)).

### P8. Reproducibility

Regenerating the same people on zoom-in, compact saves, undoing interventions, reproducing bugs, and regression tests all need reproducible simulation. The policy is tiered: exact where identity and structure depend on it, same-build-exact for replay, and bounded differences acceptable elsewhere ([25](25-determinism-policy.md)). It must be designed in from the first line of code.

### P9. Interventions and their consequences across scales

"Drop a meteor on Brooklyn", "make this person a charismatic organiser": each intervention must act on whichever representation currently exists and stay consistent as representations change ([13](13-god-interventions.md)). The player will also want to know what their intervention *did*, which needs a causal record ([24](24-world-events-and-causality.md)).

### P10. Persistence of a mostly derived world

Saves stay small only if regenerable content (data-driven and procedural baseline) is not stored. That requires stable identities, versioned generators and datasets, a notability policy, and bounded event retention ([14](14-persistence.md)).

### P11. Streaming under fast camera motion

A pinch from street to orbit crosses six orders of magnitude in under a second. Data loading, mesh generation, provider tiles, and LOD transitions must be asynchronous, prioritised, and budgeted, with graceful placeholders.

### P12. Debuggability and explainability of emergent behaviour

When a district's population collapses in year 40, someone must find out why. That needs replay, a structured event history with causal links, inspectors at every level, and headless batch runs. Players need the same explanations ("why did this city decline?").

### P13. Process: AI-assisted development across two models

Most of the code will be written by AI collaborators who see part of the codebase at a time. That favours strongly typed languages, small crates with narrow interfaces, extensive Linux-runnable tests, ADRs, and invariants checked in code.

## 1.2 Reality check: where the vision must be approximated

| Vision element | Literal version | Why it is not feasible | What we do instead |
|---|---|---|---|
| Every person on Earth simulated | ~8 billion individual agents | At 200 bytes each that is 1.6 TB of state, and far slower than real time | Individuals exist only where materialized (thousands at once); everyone else lives in cohorts and statistics with deterministic identities that can be re-materialized |
| Real people | Actual residents of Bay Ridge | Privacy, ethics, and no data | **Synthetic population** calibrated to Census statistics; no real individuals modelled |
| "Continuous" scale | One seamless model from 1 cm to 10¹³ m | No numeric type, model, or renderer spans that range | Discrete LOD bands with perceptually continuous transitions |
| Real physics everywhere | Global rigid-body, fluid, and climate simulation | Computationally impossible | Rigid bodies only in a small bubble; kinematic, statistical, parametric, or scripted models elsewhere ([10](10-physics.md)) |
| Realistic weather and climate | An atmospheric CFD model | Supercomputer-scale | Coarse climate zones plus procedural weather systems; local detail generated on demand |
| A faithful copy of real Earth | Every building, interior, and detail | Open data has gaps (heights, entrances, interiors); Google data cannot be used for simulation; planet-wide data is huge | Real skeleton from open data (best in NYC), procedural fill for gaps and interiors, staged coverage, pinned dataset versions ([22](22-real-earth-and-realism.md)) |
| Photorealism everywhere on iPhone | AAA visuals from street to orbit | Thermal and memory budgets; Google's mesh is static, has baked lighting, and cannot be edited | Realism from real data (massing, LiDAR, imagery) and PBR near the camera; optional photoreal Google backdrop at distance; a few dozen realistic characters with crowd LOD beyond |
| Anything a god can imagine | Unbounded interventions | Every effect needs defined semantics at every LOD level | A finite, extensible effect vocabulary; parametric or scripted outcomes for extreme interventions ([13](13-god-interventions.md)) |
| Perfect causal explanations | A universal causal-inference engine | Unsolvable in general and too expensive | Practical provenance: direct links, bounded provenance tags, aggregate attribution, optional counterfactual runs, each with a stated confidence ([24](24-world-events-and-causality.md)) |
| Centuries of emergent history | Agent-level simulation over centuries | Cost scales with population × time | Macro historical models for long spans; individual detail only in observed windows; notables kept as legends |
| A living solar system | Every body inhabited and detailed | Space is empty; travel is slow; detail multiplies cost | Real ephemerides; time warp; other bodies uninhabited (as in reality) unless the design adds macro-only settlements |
| Persist everything ever seen | Unbounded saves | Storage and load time | Notability policy, event retention by importance, regenerate anonymous detail, short-term observation cache |
| Bit-identical results on every device | Cross-platform exact floating point | Costly and brittle | Tiered reproducibility policy ([25](25-determinism-policy.md)) |
| LLM-driven NPC minds | Every NPC thinks via an LLM | Cost, latency, offline play, reproducibility, battery | Deterministic minds; LLMs only as an optional presentation and suggestion layer ([16](16-ai-llm-npcs.md)) |
| Entirely free at any scale | No costs ever | Hosting planet-scale data and metered services cost money at scale; Apple distribution costs $99 per year | Free open data and tools; MVP data shipped in the app; Google optional and capped; revisit hosting when scale requires it (ADR 0007) |

The principle behind these compromises:

> **Simulate at the coarsest level that preserves every fact the player can check, and keep enough information to re-create the finer levels, and to explain what happened, when they look.**
