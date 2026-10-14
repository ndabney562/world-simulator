# 7. Rendering LOD Architecture

Covers point 7. See [03](03-rendering-architecture.md) for the overall front end, [23](23-visual-providers.md) for backdrop providers, and [05](05-simulation-lod.md) for simulation LOD.

## 7.1 Selection metric

Rendering LOD is driven by **projected screen-space error**: how many pixels of error a representation would introduce. This works the same at every scale, and it is also how Cesium selects 3D Tiles. A global quality factor scales the error threshold with the device's thermal state and the frame-time budget.

## 7.2 Scale bands

Bands are configuration, not hard modes: they say which representations and providers are active. Transitions between bands are blended.

| Band | Camera altitude (approx.) | Backdrop (providers) | Simulation-owned content |
|---|---|---|---|
| Street | 0–200 m | Open-data near field (our geometry); Google clipped out | Full building meshes, animated people, props, effects |
| Neighbourhood | 200 m–3 km | Open-data tiles; optional Google beyond ~1 km | Simplified buildings where changed; instanced people; overlays |
| City | 3–50 km | Open-data tiles or optional Google | Visual proxies (crowd density, traffic flow), changed areas, events (fire, flood) |
| Region | 50–1,000 km | Open-data terrain plus imagery | Settlement-scale overlays, weather |
| Planet | 1,000 km–1 million km | NASA day and night imagery on the globe; clouds; atmosphere | Planet-scale overlays (storms, impacts) |
| System | Beyond | NASA planetary imagery; star field | Bodies at minimum on-screen size, orbit lines, labels |

## 7.3 Representations of simulation-owned content

**Buildings** (from the same descriptor: real footprint and massing plus generated facade):
1. A full mesh with facade detail (and an interior when relevant).
2. A simplified mesh: real massing with a facade texture or shader.
3. Block-level merged meshes (hierarchical LOD).
4. Beyond the neighbourhood band, the backdrop provider shows buildings. Simulation-owned buildings are drawn only where the simulation changed them (damaged, demolished, new), with exclusion zones hiding the provider's version.

**People:**
1. Skinned, realistic characters with full animation (tens, near the camera).
2. Instanced meshes with baked or vertex animation (hundreds to low thousands).
3. Point sprites or tiny impostors (thousands to tens of thousands).
4. Density fields: crowd texture drawn from aggregate density, with no individual positions.

**Vehicles** (City stage on): meshes, then instanced simple meshes, then flow shading on roads driven by aggregate volumes.

**Vegetation:** instanced meshes, then impostor cards, then imagery colour.

**Celestial bodies:** below a few pixels, bodies are drawn as sprites with a minimum size, plus optional orbit lines and labels. This is a deliberate presentation compromise, because at true scale they would be invisible from most places in the solar system.

## 7.4 Visual proxies: drawing what is not simulated individually

When the simulation is at L3 or coarser for an area ([05](05-simulation-lod.md)), the core supplies **visual proxies** derived from aggregates: density per cell and time of day, flows per road or sidewalk segment, and activity per building (lit windows at night). Each comes with a deterministic seed, so scattered crowds look the same from frame to frame. Proxies are decorative; tapping one asks the core to materialize the underlying entity ([12](12-materialization.md)).

## 7.5 Transitions

- Dithered cross-fades between representations.
- Cesium's own refinement for backdrop tiles.
- Hysteresis on switch thresholds, as for simulation LOD.
- Atmosphere and aerial perspective hide transitions in the city and region bands.
- **The rule:** rendering never shows *fewer* individuals than are simulated near the camera, and proxies fade out as real individuals are materialized.
- Switching the backdrop provider ([23](23-visual-providers.md)) uses a short cross-fade and never affects simulation content.

## 7.6 Streaming and residency

- Every representation is a cacheable GPU resource of known size, tracked against per-category budgets (backdrop tiles, simulation buildings, characters, textures).
- Requests are prioritised by screen-space error reduction per byte and by predicted camera motion: during a fast zoom, the next coarser or finer band is prefetched.
- Eviction is least-recently-visible. Simulation geometry is regenerable from data and core state. Backdrop tiles follow each provider's caching rules (Google tiles are never kept beyond its terms).

## 7.7 Budgets (initial targets, validated on an iPhone 15 Pro during the Technical Prototype)

| Budget | Target |
|---|---|
| GPU frame time | ≤ 25 ms at 30 fps sustained; 60 fps where thermals allow |
| Draw calls | Low hundreds, via instancing and merging |
| Skinned realistic characters | ≤ 30–50 |
| Instanced animated characters | ≤ 2,000 |
| Point-sprite people | ≤ 50,000 |
| GPU memory for world content (backdrop + simulation) | ~600–800 MB ([15](15-ios-performance-memory.md)) |
