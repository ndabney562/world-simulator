# 2–3. Technology Stacks and Recommendation

Covers points 2 (2–3 viable stacks with pros and cons) and 3 (the recommended stack, with emphasis on iPhone performance and deployment).

**Decision (ADR 0005, accepted):** a headless **Rust simulation core** with a **Unity 6 (URP) front end**, using **Cesium for Unity** for geospatial integration where useful, targeting iPhone. This document records the analysis behind that decision.

## 2.1 What the stack must do well, in priority order

1. **Multi-scale coordinates and precision** under our control ([07](07-coordinates-and-time.md)).
2. **Simulation LOD and materialization** as first-class concepts ([05](05-simulation-lod.md), [12](12-materialization.md)), not bolted onto a scene graph.
3. **Reproducibility** of the simulation core ([25](25-determinism-policy.md)).
4. **Sustained performance and memory discipline on iPhone.**
5. **Testability in Linux containers**, where AI collaborators work without Xcode or Unity.
6. **Real-Earth geospatial support**: WGS84/ECEF georeferencing and 3D Tiles streaming for optional backdrops ([22](22-real-earth-and-realism.md), [23](23-visual-providers.md)).
7. **Realistic rendering** within iPhone limits (ADR 0002).
8. **Zero cost** to build and run (ADR 0007).

## 2.2 The key structural choice: separate the simulation from the engine

All the viable options share one decision: **the simulation lives in its own headless core**, and the engine is only a front end. No engine provides multi-scale simulation LOD, cross-level conservation, or a command journal and event system; each of these would have to be built anyway, and building them inside an engine's object model would tie them to its 32-bit transforms, its frame loop, and its editor. A separate core is also the only way to run the simulation's full test suite in Linux CI and in the AI collaborators' containers.

The remaining choice is the language of the core and the front end.

## 2.3 Simulation core language

| Option | Verdict |
|---|---|
| **Rust** | **Chosen.** Memory safety without garbage collection, which suits a long-lived, performance-critical, largely AI-written core; excellent build and test tooling (Cargo); builds as a static library for iOS (`aarch64-apple-ios`); useful crates (Rapier physics, `rusqlite`, `glam`, `proptest`, `criterion`, ECS libraries such as `bevy_ecs` standalone or `hecs`). |
| C++ | Viable (Jolt Physics, cesium-native, a large ecosystem). Rejected on balance for memory-safety risk in a largely AI-written codebase and weaker tooling. |
| C# inside Unity (DOTS/Burst) | Fast, but ties the simulation to the Unity runtime; headless testing in Linux containers is clumsy; float behaviour and the API surface are engine-controlled. |
| Swift | Good iOS integration, but a thinner Linux ecosystem for this kind of work, and ARC overhead in hot loops. |

## 2.4 Front-end options (each consuming the Rust core)

### Option A: Unity 6 (URP) + Cesium for Unity (chosen)

Advantages:
- The most mature iOS deployment path of any general-purpose engine, and a very large body of examples (which also helps AI collaborators).
- **Cesium for Unity** (Apache 2.0, actively maintained, with iOS builds supported) provides WGS84/ECEF georeferencing with origin shifting, and 3D Tiles streaming: the backbone for open-data geometry and the optional Google backdrop.
- URP provides PBR lighting, post-processing, and upscaling suitable for realistic rendering on mobile.
- Editor, profiler, animation, UI, and audio out of the box.
- Free under Unity Personal below its revenue threshold.

Disadvantages:
- Unity's transforms are 32-bit. That is acceptable because the core supplies camera-relative data and Cesium handles georeferencing, but the front end must never be the source of truth for positions.
- Larger app binary and runtime overhead than a custom renderer.
- Vendor licensing history; the core's engine independence limits the exposure.

### Option B: Unreal Engine 5 + Cesium for Unreal

Advantages: the best realism on desktop and console; native 64-bit large-world coordinates; Mass Entity for crowds; Cesium support.

Disadvantages: iPhone is a weak target. Its signature rendering features are not the mobile path, and its memory footprint, app size, and thermal load are heavy for a sustained simulation game on a phone. It also has very long build times, is impossible to build in the AI collaborators' Linux containers for iOS, and charges a royalty above its revenue threshold.

### Option C: Custom Swift + Metal front end

Advantages: the best achievable efficiency on iPhone, direct Metal features, and a small app.

Disadvantages: we would have to write a 3D Tiles client (or integrate cesium-native), a realistic lighting pipeline, animation, tools, and an asset pipeline ourselves. That was reasonable for a stylized look but not for the realistic, real-Earth target (ADRs 0001 and 0002). It remains a long-term option, because the core is engine-agnostic.

### Considered and not recommended

| Option | Why not |
|---|---|
| Godot 4 (double-precision build) | Weaker mobile renderer and geospatial ecosystem |
| All-Rust (Bevy) | iOS is not a first-class target; pre-1.0 churn; no mature 3D Tiles client |

## 2.5 Comparison

| Criterion | A: Unity + Cesium | B: Unreal + Cesium | C: Swift/Metal |
|---|---|---|---|
| iPhone sustained performance | Good | Weak to moderate | Best |
| Realistic rendering effort | Low to moderate | Low | High |
| Geospatial / 3D Tiles | Built in (Cesium) | Built in (Cesium) | Must integrate cesium-native |
| App size | Medium | Large | Small |
| AI-collaborator workflow | Core in Linux; front end on a Mac | Core in Linux; front end heavy | Core in Linux; front end on a Mac |
| Cost | Free (Unity Personal) | Free until royalty threshold | Free |

## 3. Recommendation (accepted)

**Rust simulation core → Unity 6 / URP front end → Cesium for Unity where useful → iPhone.**

Commitments:
1. **The core is headless and engine-agnostic.** It exposes `submit(commands)`, `set_view(...)`, `advance(budget)`, `fill_render_snapshot(buffers)`, `query(request)`, and `save()/load()` ([04](04-simulation-architecture.md)). Unity never mutates simulation state except through commands.
2. **The boundary is a coarse-grained C ABI** (generated C header plus C# P/Invoke bindings). There is one call per frame to fill render buffers and no per-entity calls across the boundary. On iOS the core is linked statically (Unity iOS native plugins must be static libraries).
3. **Cesium is used where useful**: for georeferencing and 3D Tiles (the open-data provider's tiles and the optional Google backdrop). Simulation geometry near the camera is drawn by our own Unity code from core snapshots, not by Cesium ([03](03-rendering-architecture.md)).
4. **Visual providers are swappable** and never affect simulation state ([23](23-visual-providers.md)).
5. **Desktop tooling in Rust** (headless runner and debug inspector) is part of the Technical Prototype, not a later extra.

### iPhone deployment specifics

- Build the core with Cargo for `aarch64-apple-ios` as a static library placed in Unity's `Plugins/iOS`; Unity generates the Xcode project, which links it.
- CI: Linux runners for all core tests; a macOS job builds the static library and runs core tests on arm64; a Unity iOS build job (Unity in CI needs a licence activation for Personal, which is set up in the Technical Prototype) or local builds on Noah's Mac if CI activation proves impractical.
- On-device testing from the first prototype: free 7-day provisioning with a personal Apple ID, and TestFlight once the Apple Developer Program membership exists (the one known cost, ADR 0007).
- The minimum device is the iPhone 16 Pro (ADR 0037, superseding ADR 0004).

### Browser deployment specifics (Proposed ADR 0035)

- Target: Zero-friction friend distribution via static GitHub Pages web link ([ADR 0035](../adr/0035-browser-delivery-webgl-wasm.md)).
- Build: Rust core compiled to WebAssembly (`wasm32-unknown-emscripten`) linked with Unity 6000.3 WebGL output.
- Concurrency: Strictly single-threaded main loop to operate under static hosting without `SharedArrayBuffer` / COOP / COEP requirements.
- Persistence: SQLite format 4 saves backed by browser IndexedDB (`IDBFS`) with file download/upload controls.
- Fault Isolation: Fail-stop client quarantine on fatal WebAssembly trap, retaining offline capability and zero operational costs.
