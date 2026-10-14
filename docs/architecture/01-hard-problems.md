# 1. The Hardest Engineering Problems (and a Reality Check)

Covers point 1 of the brief, plus the explicit challenge to unrealistic assumptions that the brief requested.

The span of the vision is roughly **15 orders of magnitude in space** (about 1 cm of hand-scale interaction to about 7.5 × 10¹² m, the radius of the Kuiper belt) and **about 10 orders of magnitude in time** (sub-second physics steps to centuries of history and orbital periods). No single data structure, numeric type, simulation model or renderer is valid across that range. Every hard problem below is a consequence of that fact.

## 1.1 The hardest problems, ranked

### P1. Macro–micro consistency (the defining problem)

The world must tell the same story whether you look at it as 4,000 individual residents or as "District 7: population 4,000, unemployment 6%, mood: tense". When you zoom out, detail must collapse into aggregates without losing facts the player can check. When you zoom back in, detail must be regenerated so that it is consistent with (a) the aggregates, (b) everything the player has already seen, and (c) everything that happened while nobody was looking.

Why it is hard:

- Information is genuinely lost when 4,000 individuals become a handful of statistics. We must decide, explicitly, *which* information survives (see "notability" in [12](12-materialization.md)).
- Emergent micro-level effects (a riot started by three angry people) are not captured by macro models unless we design for it.
- Aggregates updated by macro rules and individuals updated by micro rules will drift apart unless each LOD level has a well-defined aggregation and disaggregation function and conservation is tested.

This is closer to multi-scale modelling in computational science than to anything game engines provide, and it is the main reason for the custom simulation core recommended in [02](02-technology-stacks.md).

### P2. Numeric precision across 15 orders of magnitude

- 32-bit floats (what GPUs and most engines use for transforms) have a 24-bit mantissa. At Earth's radius (6.4 × 10⁶ m) the spacing between representable values is **0.5 m**; at 1 AU it is **16 km**. A planet-centred `float` position makes a walking person jitter visibly; a Sun-centred one is useless.
- 64-bit floats are fine for positions inside one planet frame (sub-nanometre at Earth's radius) and still about **1 mm** at Neptune's distance from the Sun, but GPUs are slow or lack support for them, and velocities and accumulated integration error still need care.
- Depth buffers have the same problem: a single near/far range cannot cover a street-level object 0.5 m away and a planet 4 × 10⁸ m away.

Solution sketch in [07](07-coordinates-and-time.md): a hierarchy of reference frames, 64-bit math per frame, cell-relative 32-bit offsets for dense surface entities, and camera-relative 32-bit data on the GPU.

### P3. Simulating the unobserved world cheaply but credibly

A planet's worth of people cannot be simulated individually (see the reality check below). Unobserved regions must advance with coarse models, sometimes lazily ("simulate on read": compute what happened only when someone looks), while still producing coherent history. Getting the cheapness/credibility trade-off right per domain (population, economy, ecology, weather) is a long-running design problem, not a one-time decision.

### P4. Time-scale coupling

Street-level agents need ticks of ~0.1–1 s. Orbits need days to years per second of wall-clock time. You cannot run street-level simulation at "1 year per second". **Time warp and simulation LOD are therefore coupled:** as the player speeds up time, fine levels must be suspended or dematerialized and their behaviour taken over by coarser models, just as when the player zooms out. The architecture must treat time scale as a second LOD axis.

### P5. Rendering continuity from 1 m to 10¹² m on a phone

Continuous zoom requires: multi-frustum or logarithmic depth, terrain that streams from planet-scale to street-scale (chunked LOD on a sphere), atmospheric scattering that looks right from the ground and from orbit, cities that read correctly as buildings, then blocks, then light patterns, and people that degrade from animated characters to dots to density fields. All of this must fit in the thermal envelope of a phone and stream fast enough that a quick pinch-zoom does not show holes.

### P6. Mobile constraints: memory, thermals, storage, battery

iOS kills apps that exceed their memory limit (jetsam) with no warning to the user. iPhones throttle under sustained load, so peak benchmarks are misleading; sustained performance after 10+ minutes is what matters. App size and on-device storage limit how much pre-baked content and save data we can carry. Every budget in the system must be explicit and enforced. See [15](15-ios-performance-memory.md).

### P7. Determinism and reproducibility

Determinism, meaning the same seed plus the same inputs gives the same world, underpins: regenerating the same people when you zoom back in, compact saves (store inputs and deltas, not everything), undoing god interventions, reproducing bugs, and regression tests. Floating-point non-determinism (different math libraries, fused multiply-add, parallel reduction order, thread scheduling) quietly destroys this. It must be designed in from the first line of code; it cannot be retrofitted.

### P8. Interventions whose consequences cross scales

"Drop a meteor on this city", "make this person a charismatic prophet", "double the moon's mass": each intervention has to act on whatever representation currently exists (individuals if materialized, aggregates if not) and remain consistent when the representation changes later. A generalized system needs a finite effect vocabulary with well-defined semantics at each LOD level. See [13](13-god-interventions.md).

### P9. Persistence of a mostly generated world

If everything the player touches is persisted forever, save files grow without bound. If too little is persisted, the world forgets things the player cares about. The solution (seed + deltas + notability policy + compaction) requires stable identities and versioned generators, because a generator change silently alters every un-persisted part of every existing save.

### P10. Streaming under fast camera motion

A pinch from street level to orbit crosses six orders of magnitude in under a second. Rendering, procedural generation and simulation LOD transitions must be asynchronous, prioritised and budgeted, with graceful placeholders, or the frame rate collapses exactly when the player is most impressed.

### P11. Debuggability of emergent behaviour

When a district's population collapses in year 40, someone has to find out why. That needs deterministic replay, event logs, inspectors for every level, and headless batch runs. Tooling is a first-class part of the architecture, not an afterthought.

### P12. Process: AI-assisted development across two models

Large parts of the code will be written by AI collaborators who see only part of the codebase at a time. This favours: strongly typed languages, small crates with narrow interfaces, extensive automated tests that run in Linux containers, architecture decision records (ADRs), and invariants that are checked in code rather than remembered.

## 1.2 Reality check: where the vision must be approximated

The brief asked for an explicit challenge to unrealistic assumptions. These are the places where the long-term vision cannot be achieved literally and must rely on approximation, abstraction, procedural generation, simulation LOD, or other compromises.

| Vision element (as understood) | Literal version | Why it is not feasible | What we do instead |
|---|---|---|---|
| Every person on a planet is simulated | ~8 billion individual agents | At even 200 bytes each that is 1.6 TB of state, and updating it would take far longer than real time on any phone. | Individuals exist only where materialized (thousands at once). Everyone else is represented as cohorts and statistics, with deterministic identities that can be re-materialized on demand. |
| "Continuous" scale | One seamless model from 1 cm to 10¹³ m | No numeric type, model, or renderer spans that range. | Discrete LOD bands with **perceptually** continuous transitions (cross-fades, hysteresis, morphing). Continuity is a property of the experience, not the data. |
| Real physics everywhere | Global rigid-body, fluid and climate simulation | Computationally impossible on any hardware, let alone a phone. | Rigid-body physics only inside a small "physics bubble" near the camera. Everything else uses kinematic, statistical, parametric or scripted models. See [10](10-physics.md). |
| Realistic weather and climate | CFD atmosphere | Supercomputer-scale. | Coarse climate zones plus procedural weather systems (fronts, storms) on a coarse grid, with local visual detail generated on demand. |
| A faithful copy of real Earth | Real streets, buildings, terrain, population | Terabytes of data; licensing (OpenStreetMap is ODbL share-alike); data quality varies; still needs procedural fill-in for interiors and people. | Assume a fictional procedurally generated planet (assumption A5). Keep generators data-driven so a real-data region could be plugged in later as an experiment. |
| Photorealism at every scale on iPhone | AAA visuals from street to orbit | Thermal and memory budgets cannot sustain it, and procedural content cannot match authored realism. | Committed stylized art direction; quality concentrated near the camera; MetalFX upscaling; dynamic resolution. |
| Anything a god can imagine | Open-ended interventions with emergent consequences | Every effect needs defined semantics at every LOD level; unbounded effects cannot be simulated. | A finite, extensible **effect vocabulary** composed into interventions. LLMs may later translate free text into that vocabulary, but cannot invent new physics. Extreme interventions (destroy a moon) use parametric or scripted outcomes. |
| Centuries of emergent history and culture | Agent-level simulation over centuries | Cost scales with population × time. | Macro-level historical models (demography, economy, factions, culture traits) for long time spans; individual-level detail only in observed windows. Notable individuals persist as "legends" in the record. |
| Solar system "to scale" and "alive" | Every body inhabited and detailed | Space is overwhelmingly empty; distances make travel slow; detail on other bodies multiplies cost. | Bodies on analytic orbits; time warp; other bodies either uninhabited or represented at macro level only until the design calls for more. |
| Persist everything the player ever saw | Unbounded save growth | Storage and load-time costs. | Notability policy: anything the player interacted with, named, or that took part in significant events is persisted; anonymous background detail is regenerated rather than stored. Short-term observation cache avoids "the person I just saw vanished". |
| Same results on every device | Bit-identical cross-platform floating point | Achievable only with strict discipline and still risky. | Guarantee determinism for the same build on the same architecture (all shipping iPhones are arm64). Test cross-architecture (Linux x86-64 vs. Apple arm64) and treat divergence as a bug, but do not make saves depend on it. |
| Real-time LLM-driven NPCs | Every NPC thinks via an LLM | Cost, latency, offline play, determinism, and battery all rule it out. | Rule-based deterministic NPC minds; LLMs only as an optional presentation and suggestion layer whose outputs are recorded as inputs. See [16](16-ai-llm-npcs.md). |

The general principle these compromises follow:

> **Simulate at the coarsest level that preserves every fact the player can check, and keep enough information to re-create the finer levels deterministically when they look.**
