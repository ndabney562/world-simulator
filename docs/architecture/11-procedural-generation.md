# 12. Procedural Generation Strategy

> **Revision 2 (2026-10-14):** With real Earth, the macro and layout stages below (system, planet, terrain, settlements, roads, footprints) are replaced by **real open data** (OpenStreetMap/Overture, Copernicus DEM, GHSL/WorldPop). Procedural generation remains for everything the data does not contain: people, households, schedules, interiors, building use, and missing details. Google data cannot be used as simulation input. See [22 §R.2–R.3](22-real-earth-and-realism.md#r2-open-data-for-the-simulation-layer).

Covers point 12.

## 12.1 Requirements

Procedural generation is not decoration in this project; it is the mechanism that lets a planet exist in a few megabytes. Every generator must be:

1. **Deterministic**: a pure function of `(world seed, address, generator version, upstream inputs)`. No wall-clock, no global RNG, no dependence on generation order.
2. **Local**: able to generate one cell without generating the whole world. Dependencies on neighbours are bounded and go through coarser data only (a cell may read its parent's and neighbours' *coarse* summaries, never their fine detail), which prevents cascading generation.
3. **Hierarchically consistent**: fine detail is conditioned on coarse data so the two agree. A district generated at L4 with 4,000 residents must disaggregate into exactly 4,000 residents; terrain detail must average to the coarse height.
4. **Overridable**: canonical deltas and hand-authored overrides are applied on top of the generated baseline at every level.
5. **Versioned**: every generator has a version, recorded in saves. See 12.6.
6. **Budgeted and asynchronous**: generation runs on background jobs with priorities from the interest manager, never blocking a frame.

## 12.2 The generation cascade (top-down)

| Stage | Inputs | Produces | When |
|---|---|---|---|
| Star system | World seed | Star, planets, moons, orbital elements, physical parameters | World creation |
| Planet macro | Body parameters | Plate layout, coarse elevation, ocean mask, climate zones, biomes, major rivers | World creation (coarse grid) |
| Terrain detail | Coarse elevation, biome | Multi-octave noise detail plus erosion-like shaping, conditioned on coarse values | On demand per tile |
| Civilisation macro | Habitability, rivers, coast, resources | Settlement sites, sizes, cultures, historical founding dates, trade links | World creation (with a coarse "pre-history" simulation run) |
| Settlement layout | Site, size, culture, terrain | Major roads and districts (tensor-field or agent-based road growth), land use zones | On first need |
| District / block | Zones, roads | Minor streets, blocks, parcels | On demand per district |
| Buildings | Parcel, zone, culture style, era | Building descriptor (footprint, floors, style, use, units) via shape grammar | On demand per sector cell |
| Interiors | Building descriptor | Room layout, furniture | Only when entered or viewed |
| Population | District cohort tables, building units | Households, then individuals (age, occupation, traits, appearance, name) | On materialization |
| Social graph | Households, workplaces, schools | Family links, colleagues, friends (sampled with locality) | On materialization, partly stored when notable |
| Schedules | Occupation, age, culture, traits | Daily and weekly routines | On materialization |

The macro stages are run once at world creation (they are cheap at planet resolution) and their outputs are canonical. Everything finer is generated lazily.

## 12.3 Techniques by stage (initial choices)

- **Noise:** gradient/simplex noise with domain warping, evaluated in 64-bit at cell-local coordinates to avoid precision artefacts on large planets.
- **Erosion:** true hydraulic erosion is too slow to run per tile at runtime; use precomputed erosion-like noise filters and a coarse drainage network computed at world creation, refined locally.
- **Road networks:** tensor-field-guided street generation for organic or grid cities, with culture-dependent parameters, plus constraint repair (no impossible intersections or slopes).
- **Buildings:** shape grammars (split grammars for facades) producing compact descriptors; geometry is built from descriptors at render time ([03](03-rendering-architecture.md)).
- **Names and culture:** per-culture phoneme models or curated syllable sets, seeded.
- **Population sampling:** stratified sampling from cohort tables with exact-count guarantees (so aggregates are conserved), assigned to households by rules (family structures by culture).

## 12.4 Randomness discipline

- Seeds are derived hierarchically: `seed(child) = hash(seed(parent), child_address, purpose)`. A counter-based hash (for example a strong 64-bit mixer, or a keyed hash such as SipHash) gives independent, order-free streams.
- Each purpose (terrain, layout, names, traits) gets its own stream so that adding a new random draw in one place does not shift every other result.

## 12.5 Pre-history

A plausible world at game start needs history: cities grew along rivers, wars shifted borders, old buildings coexist with new ones. World creation runs a **coarse historical simulation** at L5–L6 (decades per step) from founding to the start date. It produces settlement sizes, culture distributions, building-age distributions, and a short list of legendary figures and events. This is the same macro model used during play at high time warp, which helps keep the two consistent.

## 12.6 Generator versioning (an important long-term risk)

If a generator changes, every un-stored part of every existing save silently changes: the house the player saw yesterday is now a different shape and its residents have different names. Policy:
- Each generator has a **semantic version**. Saves record the version of every generator used.
- **Golden tests** ([18](18-testing-strategy.md)) hash generator output for fixed seeds and addresses; any change in output fails CI unless the version is bumped deliberately.
- The game can keep **old generator versions** alongside new ones for existing saves, or migrate by materializing and storing affected content before switching. Which approach is chosen is postponed ([21](21-decisions-now-vs-later.md)), but versioning and golden tests are required from day one.

## 12.7 Hand authoring

The pipeline supports **authored overrides** at any address (a landmark, a scenario's special town, a story character). Overrides are just canonical deltas shipped with the game instead of created during play, which means they flow through exactly the same mechanisms.

## 12.8 Real-world data (deferred)

Generators are written to accept data sources as inputs (an elevation grid, a road graph). That keeps open a later experiment that plugs real-world datasets into one region, but the baseline assumption is a fictional world (assumption A5 in [README](README.md)).
