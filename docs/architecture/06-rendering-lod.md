# 7. Rendering LOD Architecture

> **Revision 2 (2026-10-14):** In the planet, region, and city bands, Google Photorealistic 3D Tiles (streamed and selected by Cesium) replace the procedural terrain and building HLOD described below. Screen-space-error selection, crowd and vehicle LOD, visual proxies, and budgets still apply. See [22 §R.4](22-real-earth-and-realism.md#r4-revised-rendering-approach-hybrid).

Covers point 7. See [03](03-rendering-architecture.md) for the overall renderer and [05](05-simulation-lod.md) for simulation LOD.

## 7.1 Selection metric

Rendering LOD is driven by **projected screen-space error** (how many pixels of error a representation would introduce), not by fixed distances. This works identically at every scale: a 10 m building and a 1,000 km continent are both handled by "is the coarser version within N pixels of the finer one?". A global quality factor scales N with the device's thermal state and the frame-time budget.

## 7.2 Scale bands

Bands are not hard modes; they are configuration describing which representations exist and which systems are active. Transitions between bands are blended.

| Band | Camera altitude (approx.) | What is drawn |
|---|---|---|
| Street | 0–200 m | Full meshes, skinned/animated people, props, decals, interiors if visible, near shadows |
| Neighbourhood | 200 m–3 km | Building meshes (simplified), instanced people with vertex animation, cars, trees |
| City | 3–50 km | Merged block meshes (HLOD), people and cars as point sprites or flow-line shaders driven by aggregates, road network as textured strips |
| Region | 50–1,000 km | Terrain tiles, settlement footprints as textured overlays, night light maps, clouds, rivers |
| Planet | 1,000 km–1 million km | Planet sphere with procedural surface textures, atmosphere, cloud layer, city lights; moons |
| System | Beyond | Bodies as spheres or sprites with minimum on-screen size, orbit lines, labels; star field |

## 7.3 Representations per content type

**Terrain and planet surface.** A cube-sphere: six faces, each a quadtree of tiles (the same cell hierarchy used for simulation addressing, see [07](07-coordinates-and-time.md)). Tiles are selected by screen-space error (in the style of chunked LOD / CDLOD), with **geomorphing** (vertices blend between levels) to avoid popping. Far away, the planet is a single sphere mesh with baked tile textures; there is no separate "planet model", just the top of the same quadtree.

**Buildings.** Four representations, generated from the same procedural descriptor:
1. Full mesh with window and door detail (and interior when relevant).
2. Simplified mesh (box-and-roof plus facade texture or shader pattern).
3. **Hierarchical LOD (HLOD):** all buildings in a block merged into one mesh; then a district merged into one low-poly mesh.
4. Surface texture and height-map contribution at region scale; night-time light map at planet scale.

**People.** Four representations:
1. Skinned mesh with full animation (only tens, near the camera).
2. Instanced mesh with vertex-animation textures (hundreds to low thousands).
3. Point sprites or tiny impostors (thousands to tens of thousands), positions from simulation or from visual proxies.
4. Density field: a shader that draws crowd texture from an aggregate density map; no individual positions at all.

**Vehicles.** Same pattern as people: meshes, then instanced simple meshes, then flow-line shading on roads driven by aggregate traffic volumes.

**Vegetation.** Instanced meshes, then impostor cards, then biome colour on terrain.

**Celestial bodies.** When the projected size falls below a few pixels, bodies are drawn as sprites with a minimum size, plus optional orbit lines and labels, because realistic scale makes planets invisible from almost anywhere in the solar system. (This is a deliberate presentation compromise.)

## 7.4 Visual proxies: drawing what is not simulated

When the simulation is at L3 or coarser for a node (see [05](05-simulation-lod.md)), there are no individuals to draw. The core provides **visual proxies** derived from aggregates:
- Density per cell and time of day, flows per road segment, activity per building (lit windows, smoke, construction).
- A deterministic seed per proxy so the renderer can scatter people or cars that look consistent frame to frame.

Proxies are explicitly decorative: tapping one asks the core to materialize the underlying entity ([12](12-materialization.md)). This is what lets a city at night look alive from 10 km up without simulating a million people individually.

## 7.5 Transitions

- **Dithered cross-fades** between representations over a short duration (temporal and screen-door dither, cheap on tile GPUs and compatible with temporal upscaling).
- **Geomorphing** for terrain.
- **Hysteresis** on switch thresholds, as for simulation LOD.
- **Atmosphere and depth-of-field** help hide transitions in the city and region bands.
- Mismatch between rendering and simulation transitions is expected; the rule is that rendering never shows *fewer* individuals than are simulated near the camera, and proxies fade out as real individuals are materialized.

## 7.6 Streaming and residency

- Every representation is a cacheable GPU resource with a known size, tracked by a residency manager with per-category memory budgets (terrain, buildings, characters, textures).
- Requests are prioritised by screen-space error reduction per byte and by predicted camera motion (during a fast zoom, prefetch the next coarser or finer band along the zoom direction).
- Eviction is least-recently-visible within budget. Because everything is procedurally regenerable, eviction never loses data.

## 7.7 Budgets (initial targets, to be validated on device in Phase 1)

| Budget | Target |
|---|---|
| Frame time (GPU) | ≤ 25 ms at 30 fps sustained; 60 fps where thermals allow |
| Draw calls per frame | Low hundreds (instancing and HLOD) |
| Animated skinned characters | ≤ 50 |
| Vertex-animated instanced characters | ≤ 2,000 |
| Point-sprite people | ≤ 50,000 |
| GPU resident memory for world content | ~600–800 MB on an 8 GB device (see [15](15-ios-performance-memory.md)) |

These numbers are starting hypotheses; Phase 1 measures them on real hardware and the budgets become ADRs.
