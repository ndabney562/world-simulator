# 16. Performance and Memory Strategy for Modern iPhones

Covers point 16. Platform: iPhone 15 Pro and newer (ADR 0004); front end Unity 6 / URP with Cesium for Unity; simulation in the Rust core (ADR 0005).

## 16.1 Target devices

- **Minimum:** iPhone 15 Pro (A17 Pro, 8 GB RAM). All later iPhones meet or exceed it.
- **OS:** the current major iOS version and the one before it.
- Per-device capabilities (memory limit, sustained GPU performance, upscaler behaviour) are **measured on hardware in the Technical Prototype**, not assumed from documentation.

## 16.2 The real constraints

1. **Memory limit, not memory size.** iOS terminates apps that exceed a per-app limit (jetsam) well below physical RAM. The increased-memory-limit entitlement raises it on supported devices. Budgets assume we stay comfortably inside the limit measured on an iPhone 15 Pro, including Unity's and Cesium's own overhead.
2. **Sustained thermals.** Phones throttle after minutes of heavy load. Budgets are calibrated after 15–20 minutes of continuous play.
3. **Unified memory.** CPU and GPU share RAM and bandwidth, so duplicated data wastes both.
4. **Heterogeneous CPU.** Unity's main and render threads, Cesium's tile loading, and the core's simulation jobs compete for a small number of performance cores. Background simulation, generation, and saving run on efficiency cores at lower priority.
5. **Network and battery.** Optional streamed backdrops ([23](23-visual-providers.md)) cost radio power and data, and are capped. A paused world should cost almost nothing.

## 16.3 Frame budget (30 fps sustained, 33.3 ms)

| Work | Budget |
|---|---|
| Core: commands, high-priority simulation (L0/L1), physics bubble | ≤ 8 ms on a worker thread |
| Core: render snapshot fill | ≤ 2 ms |
| Unity: snapshot decoding, provider manager, culling, scripts | ≤ 6 ms main thread |
| GPU frame (backdrop + simulation geometry + effects) | ≤ 25 ms |
| Background (generation, L2+ simulation, saving, tile streaming) | Continuous on efficiency cores, budget-limited |

60 fps is offered where the device stays cool, with the simulation tick unchanged.

## 16.4 Memory budget (initial; validated in the Technical Prototype)

| Category | Budget |
|---|---|
| Unity runtime, Cesium, managed heap, app code | 250–350 MB (measure) |
| Simulation: materialized entities (L0–L2) | 150–250 MB |
| Simulation: aggregates, world tree, indices, event index cache | 100–150 MB |
| Data and procedural caches (simulation tiles, descriptors, navigation graphs) | 100 MB |
| GPU: backdrop tiles | 200–300 MB |
| GPU: simulation geometry, characters, textures, render targets | 250–350 MB |
| Observation cache, save staging, misc | 100 MB |
| **Total working set target** | **≈ 1.2–1.6 GB**, kept well under the measured jetsam limit |

Each category has a hard cap enforced by its owner (provider manager and residency for GPU content, the interest manager for simulation), and the app shrinks all caps on iOS memory warnings.

## 16.5 Techniques

**CPU and data layout (core)**
- Data-oriented design: struct-of-arrays for hot data, ECS archetypes, no per-entity heap allocations in hot paths.
- Arena and pool allocators per tick; no steady-state allocation in frame code.
- SIMD math (`glam`), bulk utility scoring, spatial hashing.
- Multi-rate and event-driven simulation ([04](04-simulation-architecture.md)), so most agents cost nothing most of the time.

**Front end (Unity)**
- No per-frame managed allocations; snapshot data read from native buffers without copying to managed objects.
- GPU instancing for people, vehicles, and vegetation; merged meshes for buildings at distance.
- URP mobile-friendly settings: forward rendering, limited real-time shadows, baked or cheap ambient terms, dynamic resolution with upscaling.
- Compressed textures (ASTC); facade detail from shaders and tiling materials rather than unique textures.
- Cesium tile cache sized to the backdrop budget; maximum screen-space error raised under thermal pressure.

**Adaptivity**
- A **quality governor** reads frame times and the iOS thermal state and adjusts, in order: render resolution, the rendering LOD error threshold (Cesium and our own), animated character count, simulation LOD budgets (materialization radius and level caps), and finally the frame-rate target.
- Background generation, tile streaming, and saving slow down when the thermal state is serious or critical.

**Storage and app size**
- The MVP area's simulation and visual tiles ship in the app; the size is measured in the Technical Prototype data spike.
- Later regions are downloaded on demand from free-tier static hosting and cached within a storage budget (ADR 0007).

## 16.6 Measuring

- An in-app performance overlay (per-stage timings, memory per category, LOD state, provider status and quota, thermal state).
- The Unity Profiler and Memory Profiler; Xcode Instruments (Time Profiler, Allocations, Metal System Trace, Energy) on device every stage.
- Automated **benchmark flights** (scripted camera paths from street level upward) run on device, with results recorded per build ([18](18-testing-strategy.md)).
- Desktop `criterion` benchmarks of the core catch CPU regressions in CI long before device testing.
