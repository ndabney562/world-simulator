# 4. Rendering Architecture

Covers point 4. Rendering LOD specifically is in [06](06-rendering-lod.md); visual providers in [23](23-visual-providers.md).

## 4.1 Principles

1. **The front end is a consumer, never an owner, of world state.** Unity reads immutable snapshots produced by the Rust core. It holds only GPU-side caches (meshes, textures, instance buffers) that can be thrown away and rebuilt at any time.
2. **The GPU never sees absolute coordinates.** Positions reach the GPU as 32-bit values relative to the camera or to a nearby origin. The core computes them from 64-bit frame-relative positions, and Cesium's georeference handles origin shifting for 3D Tiles ([07](07-coordinates-and-time.md)).
3. **Realistic, within a budget.** Realism comes from real data (real massing, real terrain, aerial imagery, real sun position) plus physically based materials and lighting. Every pass has a fixed millisecond and memory budget, and quality scales down before frame rate does.
4. **Backdrop and simulation are separate layers.** Visual providers draw the backdrop; our own code draws everything the simulation owns ([23](23-visual-providers.md)).
5. **Scale-independent pipeline.** The same pipeline renders a street, a city, and a planet; what changes is which representations and providers are active.

## 4.2 Threading and data flow

```
 Rust core                                   Unity 6 / URP (C#)
 ─────────                                   ──────────────────
 sim tick N ──► RenderSnapshot N ──────────► triple-buffered snapshot ring
               (simulation geometry,            │
                exclusion zones, overlays,      ▼
                camera-relative transforms)  Frame: interpolate N-1 → N
                                             Provider manager: backdrop tiles (Cesium)
                                             Simulation layer: instanced meshes, effects
                                             URP render ──► GPU
```

- The simulation tick rate (for example 10–20 Hz for active agents) is decoupled from the display rate (30 or 60 Hz, with ProMotion used for UI and camera). Unity interpolates between the last two snapshots.
- The **RenderSnapshot** is a compact, versioned description of what the simulation wants drawn: camera and frame chain, simulation-owned building descriptors and changes, agent instance arrays (position, heading, animation state, appearance seed), visual proxies for aggregate areas (density, flows), exclusion zones and overlays for providers, and weather and lighting parameters.
- To avoid copies, Unity allocates native buffers (for example `NativeArray`s backing compute or graphics buffers) and passes pointers to the core, which fills instance data directly. Buffer ownership follows the triple-buffer ring.
- Mesh generation for simulation geometry (buildings from footprints and massing, terrain patches) runs on background job threads in the core and is uploaded asynchronously by Unity. A missing mesh is drawn with a placeholder (a lower LOD, or the provider's backdrop), never blocking the frame.

## 4.3 Layers

| Layer | Drawn by | Content |
|---|---|---|
| Sky and space | Unity + NASA imagery provider | Sun, moon, stars, planets (by ephemeris) |
| Backdrop | Provider manager (Cesium 3D Tiles and imagery) | Open-data visual tiles by default; optional Google Photorealistic 3D Tiles at distance; NASA imagery from orbit |
| Simulation geometry | Our Unity code from snapshots | Buildings the simulation owns near the camera and wherever changed; terrain patches; props |
| Agents | Our Unity code from snapshots | People and vehicles (instanced) |
| Overlays and effects | Our Unity code from snapshots | Fire, smoke, water, rain, density and flow visualizations, selection |
| UI | Unity UI Toolkit | Inspector, history, time controls, attribution line |

Depth precision across scales comes from Unity's reversed-Z depth, Cesium's origin shifting, and camera near/far planes adjusted per scale band. At planet and system scale, distant bodies are drawn in a separate camera pass with its own near and far planes.

## 4.4 Frame structure (URP)

1. Shadow maps: a small cascade count, near field only.
2. Opaque pass: backdrop tiles, simulation buildings, agents (GPU instanced), props.
3. Sky and atmosphere: physically based sky and aerial perspective, consistent from street level to orbit, which is one of the main tools for making scale transitions feel continuous.
4. Transparent pass: water, glass, particles (rain, smoke, fire), clouds.
5. Post-processing: tone mapping, a restrained colour grade, bloom for night lighting.
6. Upscaling: dynamic resolution with URP's upscaling options (evaluated on device in the Technical Prototype).
7. UI overlay, including the combined data attribution ([23](23-visual-providers.md)).

Lighting uses the **real sun position** for the simulation date and time at the camera's location. Google tiles have baked lighting, which cannot match this exactly. That is accepted, and fog, atmosphere, and the distance at which they are used reduce the mismatch.

## 4.5 Geometry sources

| Content | Source |
|---|---|
| Terrain | NYC 2017 LiDAR for the MVP area (Copernicus DEM elsewhere later), as heightfield patches |
| Ground texture | NYC orthoimagery |
| Buildings | NYC 3-D Building Model and footprints with heights; facades and roofs generated procedurally with realistic PBR materials matched to the area's building types (brick rowhouses, limestone and brownstone, apartment blocks) |
| Streets and sidewalks | OSM geometry, aligned to the footprints and imagery |
| Interiors | Generated on demand only when entered or viewed (not before the City stage) |
| People | A small set of realistic base characters with appearance parameters (body proportions, skin and hair, clothing variants); instanced, with animation LOD |
| Vehicles | A small library of realistic modular meshes (City stage) |
| Vegetation | Street trees from NYC open data (the street tree census) where available; instanced meshes near, impostors far |

## 4.6 Front-end and simulation interface contract

- The front end may **request** detail ("the camera is looking at this cell at this resolution"). Requests are hints to the core's interest manager, never direct entity creation.
- The core decides what is materialized (simulation LOD). The front end decides how to draw whatever exists (rendering LOD). Where the simulation holds only aggregates, the core provides visual proxies ([06](06-rendering-lod.md)).
- **Picking** goes through the core: Unity sends a ray in frame coordinates, and the core resolves it against its own spatial index (never against provider geometry) and returns a stable entity ID. Tapping a visual proxy asks the core to materialize the entity on demand ([12](12-materialization.md)).

## 4.7 Tooling

- The Unity Profiler and Frame Debugger, Xcode GPU frame capture, and Instruments on device.
- An in-app debug overlay: frame time per stage, memory per category, LOD state, materialized counts, provider status and quota, and thermal state.
- The desktop `ws-inspector` (Rust) for simulation debugging without Unity.

## Proposed core calendar and light input

[Proposed ADR0030](../adr/0030-calendar-sun-and-sky-query.md) describes a coherent read-only native calendar/sun record paired with the render tick, an explicitly transformed direction in the snapshot's ENU basis, and a Unity HUD/light that only presents core output. Its ABI migration preserves prior behavioral checks and old-client refusal. The design is pending acceptance; the current ABI/render path is unchanged by this docs-only proposal.
