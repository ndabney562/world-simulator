# 12. Data Ingestion and Procedural Generation Strategy

Covers point 12. The world is real Earth (ADR 0001), so generation is **data-driven first and procedural second**: real open data defines the world's skeleton, and procedural generation fills everything the data does not contain. Data sources and licensing are in [22](22-real-earth-and-realism.md). Google data is never an input to this pipeline ([23](23-visual-providers.md)).

## 12.1 Requirements

Every generator, whether it reads data, generates procedurally, or both, must be:

1. **Deterministic**: a pure function of `(dataset version, world seed, address, generator version, upstream inputs)`. Identity-defining and structural outputs are exact on all platforms (Tier 1, [25](25-determinism-policy.md)).
2. **Local**: able to generate one cell without generating the whole world. Dependencies on neighbours go only through coarse summaries.
3. **Hierarchically consistent**: fine detail is conditioned on coarse data. A neighbourhood with 4,000 residents in its cohort tables disaggregates into exactly 4,000 residents.
4. **Overridable**: canonical deltas and hand-authored overrides apply on top of the baseline at every level.
5. **Versioned**: generators and datasets both have versions, recorded in saves (12.6).
6. **Budgeted and asynchronous**: runtime generation runs on background jobs prioritised by the interest manager.

## 12.2 Two pipelines

**Offline data pipeline** (a Rust tool in `tools/`, run on a developer machine or in CI, not on device):
1. Download pinned releases of source datasets.
2. Reproject everything to WGS84/ECEF with ellipsoidal heights (geoid and NAVD88 conversions, [07](07-coordinates-and-time.md)).
3. Clean, align, and reconcile sources: match OSM entrances and sidewalks to NYC footprints; correct small misalignments; fill missing heights from LiDAR.
4. Derive simulation attributes: building use, number of units and floors, sidewalk network, street-crossing graph, land use, and Census-derived cohort tables per block.
5. Write **simulation tiles** keyed by S2-style cell ID, in a versioned binary format, plus the matching visual tiles for the open-data provider ([23](23-visual-providers.md)).
6. Record the **dataset version**: a manifest of source releases, pipeline version, and content hashes.

The tiles for the MVP area ship inside the app. Later stages serve them from free-tier static hosting (ADR 0007).

**Runtime generation** (in the core, on device): reads simulation tiles and generates everything that is not stored: synthetic people, households, schedules, social graphs, facades, interiors, and gap-filling details.

## 12.3 The generation cascade

| Stage | Source | Produces | When |
|---|---|---|---|
| Solar system | Real ephemerides (JPL) | Body positions at any time | Always (analytic) |
| Earth macro | Climate normals, land cover, population grids (GHSL/WorldPop) | Climate zones, biomes, macro-demography, settlement list | Pipeline (coarse), staged by region |
| Terrain | NYC LiDAR (MVP); Copernicus DEM elsewhere | Height tiles (ellipsoidal) | Pipeline; runtime refinement near camera |
| Streets and paths | OSM/Overture | Road, sidewalk, crossing, and transit graphs | Pipeline |
| Buildings | NYC 3-D Building Model, NYC footprints (MVP); Overture/OSM elsewhere | Footprint, height, roof shape, floors, use (where known) | Pipeline |
| Building gaps | Procedural rules conditioned on zoning, era, and neighbours | Missing heights, uses, unit counts | Pipeline (stored) or runtime (Tier 1) |
| Facades and appearance | Procedural grammars matched to regional styles, checked against orthoimagery colours | Facade descriptors and materials | Runtime, on demand |
| Interiors | Procedural, from building descriptor | Room layout, furniture | Runtime, only when entered or viewed |
| Population | Census/ACS cohort tables per block (MVP); GHSL plus national statistics elsewhere | Households, then individuals (age, occupation, traits, appearance, name) | Runtime, on materialization |
| Social graph | Households, workplaces, schools | Family, colleague, and friend links (sampled with locality) | Runtime, on materialization; stored when notable |
| Schedules | Occupation, age, commute mode, traits | Daily and weekly routines | Runtime, on materialization |

## 12.4 Techniques

- **Population synthesis:** iterative proportional fitting or similar, against Census marginal tables, producing households with exact block-level counts (so aggregates are conserved), then stratified, seeded sampling of individuals. Names come from distributions appropriate to the area's demographics, never from real residents' records.
- **Facades:** split grammars producing compact descriptors; geometry built from descriptors in Unity ([03](03-rendering-architecture.md)).
- **Gap filling:** rules conditioned on what is known (zoning, building class, neighbour heights), with outputs flagged as inferred so the inspector can show "estimated".
- **Terrain refinement:** where only coarse DEM exists, noise detail conditioned on the coarse values. Not needed for the NYC MVP, which has LiDAR.

## 12.5 Randomness discipline

- Seeds are derived hierarchically: `seed(child) = hash(seed(parent), child_address, purpose)`, using a counter-based, keyed integer hash. This gives independent, order-free streams.
- Each purpose (names, traits, schedules, facades) gets its own stream, so adding a random draw in one place does not shift other results.
- Random draws that decide identity or structure use integer operations only (Tier 1).

## 12.6 Versioning (an important long-term risk)

If a generator or dataset changes, every unstored part of every existing save changes silently: the house the player saw yesterday is now a different shape, or its residents have different names. Policy:
- Each generator has a **semantic version**, and each processed dataset has a **dataset version**. Saves record both.
- **Golden tests** hash generator output for fixed seeds, addresses, and dataset versions; any change fails CI unless the version is bumped deliberately ([18](18-testing-strategy.md)).
- Existing saves either keep using old versions (old dataset tiles kept available; old generator code paths kept), or are migrated by materializing and storing affected content before switching. Which approach is used is decided at the first breaking change ([21](21-decisions-now-vs-later.md)); recording versions is required from day one.

## 12.7 Hand authoring

The pipeline supports **authored overrides** at any address (a landmark's interior, a scenario's special event). Overrides are canonical deltas shipped with the game, so they flow through the same mechanisms.

## 12.8 Pre-history

A world starting at the current date needs plausible recent history (people have jobs, relationships, and pasts). World creation runs a short **coarse history pass** at L4–L5 over the preceding years to set up relationships, tenure, and a seed set of notable past events, recorded as low-importance aggregate events. Real historical events are not simulated; the history is synthetic and plausible, consistent with the data-driven present.
