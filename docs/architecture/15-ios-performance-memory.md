# 16. Performance and Memory Strategy for Modern iPhones

> **Revision 2 (2026-10-14):** Budgets now also need to cover Cesium tile streaming and Unity's runtime overhead. Both are measured in the Phase 0 spike on an iPhone 15 Pro. See [22](22-real-earth-and-realism.md).

Covers point 16.

## 16.1 Target devices (proposal; decision D-9)

- **Minimum:** A17 Pro class (iPhone 15 Pro / Pro Max) and newer, which have 8 GB of RAM, hardware ray tracing (not required), and current Metal features. All later iPhones meet or exceed this.
- **Rationale:** 8 GB RAM is the single most important constraint for a simulation of this kind; the A17 Pro class also covers devices that support Apple's on-device foundation model, which matters for optional LLM features ([16](16-ai-llm-npcs.md)).
- **Possible later extension** down to 6 GB devices (A15/A16) with reduced budgets, decided after Phase 1 measurements.
- OS: the current major iOS version and the one before it.

Specific per-device capabilities (MetalFX temporal support, mesh shader support, memory limits) must be **verified on hardware in Phase 0/1** rather than assumed from documentation.

## 16.2 The real constraints

1. **Memory limit, not memory size.** iOS terminates apps that exceed a per-app limit (jetsam) that is well below physical RAM. The `increased-memory-limit` entitlement raises it on supported devices. Our budgets assume we must stay comfortably inside the limit measured on the minimum device.
2. **Sustained thermals.** Phones throttle after minutes of heavy load. Design for the throttled state: budgets are calibrated after 15–20 minutes of continuous play, not on a cold device.
3. **Unified memory.** CPU and GPU share RAM and bandwidth. Duplicating data between CPU and GPU wastes both; bandwidth is a primary performance cost on tile-based GPUs.
4. **Heterogeneous CPU.** A small number of performance cores and more efficiency cores. Background simulation and generation should run on efficiency cores at lower quality-of-service so they do not steal from the frame.
5. **Battery.** A sim game is played for long sessions; frame-rate caps and idle throttling matter (a paused world should cost almost nothing).

## 16.3 Frame budget (at 30 fps sustained, 33.3 ms)

| Work | Budget (wall-clock on its thread) |
|---|---|
| Core: commands, high-priority simulation (L0/L1), physics bubble | ≤ 8 ms (performance core) |
| Core: render snapshot fill | ≤ 2 ms |
| Renderer: visibility, LOD selection, encoding | ≤ 4 ms |
| GPU frame | ≤ 25 ms |
| Background jobs (generation, L2+ simulation, saving, streaming) | Continuous on efficiency cores, budget-limited |

60 fps is offered where the device stays cool (camera motion benefits most), with the simulation tick unchanged. 120 Hz ProMotion is used only for UI and camera interpolation, not for simulation.

## 16.4 Memory budget (initial, for an 8 GB device; to be validated)

| Category | Budget |
|---|---|
| Simulation: materialized entities (L0–L2) | 150–250 MB |
| Simulation: aggregates, world tree, indices | 100–150 MB |
| Procedural caches (descriptors, nav graphs) | 100 MB |
| GPU: geometry (terrain tiles, buildings, HLOD) | 300–400 MB |
| GPU: textures (ASTC compressed), render targets | 200–300 MB |
| Observation cache, save staging, misc | 100 MB |
| **Total working set target** | **≈ 1.0–1.3 GB**, well under the measured jetsam limit |

Each category has a hard cap enforced by its own manager (residency for GPU content, the interest manager for simulation), and the app responds to iOS memory warnings by shrinking all caps.

## 16.5 Techniques

**CPU and data layout**
- Data-oriented design: struct-of-arrays for hot data, ECS archetypes, no per-entity heap allocations in hot paths.
- Arena and pool allocators per tick and per job; no allocation in steady-state frame code.
- SIMD math (`glam`), bulk utility scoring, and spatial hashing for neighbour queries.
- Multi-rate and event-driven simulation ([04](04-simulation-architecture.md)) so most agents cost nothing most of the time.

**GPU**
- Tile-friendly forward+ rendering with memoryless attachments for depth and intermediate targets.
- Instancing, HLOD, and GPU-driven culling (compute) to keep draw calls low.
- Dynamic resolution plus MetalFX upscaling; resolution drops before frame rate does.
- ASTC texture compression; procedural shading instead of large textures (stylized look).
- Shared-storage buffers written directly by the core (no copies across the FFI).

**Adaptivity**
- A **quality governor** reads frame times and `ProcessInfo.thermalState` and adjusts, in order: render resolution, rendering LOD error threshold, number of animated characters, simulation LOD budgets (materialization radius and level caps), and finally frame-rate target.
- Background generation and saving pause or slow when the thermal state is serious or critical.

**Storage and app size**
- Procedural content keeps the app small (the target is a download well under typical cellular limits).
- Optional content (higher-quality assets, voices if any) via Apple's background asset download mechanisms, not in the base app.

## 16.6 Measuring

- In-app performance overlay (per-system timings, memory per category, LOD state, thermal state).
- Xcode Instruments (Time Profiler, Allocations, Metal System Trace, Energy) on device every phase.
- Automated device performance runs (XCTest metrics) on fixed camera paths ("benchmark flights") with recorded results per build, so regressions are visible ([18](18-testing-strategy.md)).
- Desktop benchmarks of the core (`criterion`) catch CPU regressions in CI long before device testing.
