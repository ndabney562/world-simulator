# 4. Rendering Architecture

> **Revision 2 (2026-10-14):** The world is now real Earth with a realistic look, so rendering is a **hybrid**: Google Photorealistic 3D Tiles for the backdrop and our own semantic geometry near the camera. It runs on Unity and Cesium rather than custom Metal. See [22 §R.4](22-real-earth-and-realism.md#r4-revised-rendering-approach-hybrid). The snapshot interface, camera-relative coordinates, budgets, and LOD principles below still apply. The "stylized" and Metal-specific parts are superseded.

Covers point 4. Rendering LOD specifically is in [06](06-rendering-lod.md).

## 4.1 Principles

1. **The renderer is a consumer, never an owner, of world state.** It reads immutable snapshots produced by the core and holds only GPU-side caches (meshes, textures, instance buffers) that can be thrown away and rebuilt at any time.
2. **The GPU never sees absolute coordinates.** All positions sent to the GPU are 32-bit floats relative to the camera (or to a nearby render origin), computed on the CPU from 64-bit frame-relative positions. See [07](07-coordinates-and-time.md).
3. **Stylized and budget-first.** Every pass has a fixed millisecond and memory budget; quality scales down before frame rate does.
4. **Designed for Apple's tile-based GPUs.** Use memoryless attachments, minimise render-pass switches and bandwidth, prefer forward+ over fat G-buffers.
5. **Scale-independent pipeline.** The same pipeline renders a street, a city from a plane, and a planet from orbit; what changes is which representations are submitted, not the pipeline.

## 4.2 Threading and data flow

```
 Core (Rust)                               iOS app (Swift / Metal)
 ─────────────                             ───────────────────────
 sim tick N  ──► builds RenderSnapshot N ──► triple-buffered snapshot ring
                 (camera-relative later)        │
                                                ▼
                                   Frame: interpolate N-1 → N for smooth motion
                                   Visibility & LOD selection (CPU, then GPU compute)
                                   Encode command buffers ──► GPU
```

- The simulation tick rate (for example 10–20 Hz for active agents) is decoupled from the display rate (30, 60, or 120 Hz with ProMotion). The renderer interpolates positions between the last two snapshots so motion is smooth at any display rate.
- A **RenderSnapshot** is a compact, versioned description of what is visible: camera and frame chain, terrain tile requests, building instance descriptors, agent instance arrays (position, heading, animation state, appearance seed), aggregate visualizations (density fields, traffic flows, city light maps), celestial bodies, weather and lighting parameters.
- To avoid copies on unified memory, the Swift side allocates shared-storage `MTLBuffer`s and passes raw pointers to the core, which fills instance data directly. Ownership rules (which buffer the GPU is reading) are enforced by the triple-buffer ring.
- Procedural mesh generation (buildings, terrain tiles) runs on background job threads in the core and hands finished vertex/index data to the renderer, which uploads it asynchronously. A missing mesh is drawn with a placeholder (lower LOD or impostor), never blocking the frame.

## 4.3 Frame structure (initial)

A **forward+ (clustered) renderer**, chosen over deferred because it is simpler, works well with Apple's tile memory, handles many small lights (street lamps, windows at night) via light clustering, and keeps bandwidth low.

Passes, in order:

1. **Depth-range partitioning.** The scene is split into depth layers, each with its own near/far planes, rendered far-to-near and composited: (a) sky, stars and distant celestial bodies; (b) planet-scale layer (terrain, oceans, clouds, atmosphere); (c) near layer (buildings, people, vehicles, props). Each layer uses **reversed-Z with a 32-bit float depth buffer**, which on its own gives excellent precision over a wide range. With layering, depth precision is never a limiting factor at any zoom level. Logarithmic depth was considered but breaks early-Z and hardware depth optimisations on tile-based GPUs, so it is not the default.
2. **Shadow pass**: a small cascaded shadow map (2–3 cascades) only for the near layer; at planet scale, terrain self-shadowing comes from precomputed horizon or ambient terms.
3. **Light culling**: compute pass assigning lights to clusters.
4. **Opaque pass**: terrain, buildings, characters (GPU-instanced), props.
5. **Atmosphere and aerial perspective**: physically based but precomputed scattering lookup tables (in the style of Bruneton or Hillaire), which give a consistent look from street level through to orbit. This is one of the main tools that makes scale transitions feel continuous.
6. **Transparent pass**: water, glass, particles (rain, smoke, fire), clouds (layered impostor or raymarched at low resolution and upscaled).
7. **Post-processing**: tone mapping, a stylized colour grade, bloom for city lights, optional depth-of-field for "tilt-shift" at neighbourhood scale (which also hides LOD transitions).
8. **Upscale**: render at dynamic internal resolution and upscale with **MetalFX** (temporal where it is supported and stable, spatial otherwise).
9. **UI**: SwiftUI/UIKit overlay for menus and inspectors; in-world labels (names, statistics) rendered by the Metal pass for performance.

## 4.4 Geometry sources

| Content | Source | Notes |
|---|---|---|
| Terrain | Procedural heightfield tiles on a cube-sphere quadtree | Generated by the core on demand (possibly GPU compute later), cached per tile. |
| Water | Sea level plus procedural rivers and lakes | Stylized shader; no fluid simulation. |
| Roads | Procedural road graph to mesh strips | Generated per city cell. |
| Buildings | Procedural grammar from parcel, zoning and style seed | Assembled from a small library of modular parts (walls, roofs, windows) plus generated footprints; merged per block for distant LODs. |
| Interiors | Generated on demand only when entered or looked into | Not needed for the MVP. |
| People | A few base meshes plus appearance parameters (proportions, colours, clothing variants) | GPU instancing; animation via vertex-animation textures for crowds, skinned meshes only near the camera. |
| Vehicles | Small library of modular meshes | Instanced. |
| Vegetation | Instanced meshes near; impostor cards mid; texture tint far | Density from biome data. |
| Celestial bodies | Sphere meshes with procedural or baked textures | Planet surfaces transition into the terrain system when approached. |

Authored assets are deliberately few: a modular kit of parts plus shaders. Procedural generation provides the variety, which keeps the app small and the content consistent with the simulation.

## 4.5 Renderer–simulation interface contract

- The renderer may **request** detail (for example "the camera is looking at cell X at this resolution"). Requests are hints to the core's interest system; they never directly create entities.
- The core decides what is materialized (simulation LOD) and the renderer decides how to draw whatever exists (rendering LOD). Where the simulation holds only aggregates, the core provides **visual proxies**: density fields, flow lines, and seeds that let the renderer draw plausible crowds or traffic without simulated individuals behind them. See [06](06-rendering-lod.md).
- Picking (tapping a person or building) goes through the core: the renderer returns a ray in frame coordinates, the core resolves it against its own spatial index and returns a stable entity ID. If the tap lands on a visual proxy, the core materializes the entity on demand (see [12](12-materialization.md)).

## 4.6 Tooling

- Metal debugging via Xcode's GPU frame capture and Instruments (Metal System Trace) on device.
- A built-in **debug overlay** in the app: frame-time breakdown per pass, memory per category, LOD band, materialized-entity counts, thermal state.
- The desktop inspector in `tools/` renders 2D and simple 3D debug views via `wgpu`; it is a debugging tool, not a second production renderer.
