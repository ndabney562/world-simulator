# 2–3. Technology Stacks and Recommendation

> **Revision 2 (2026-10-14):** The recommendation below has been **revised**. Because Noah chose real Earth and a realistic look, the front end is now **Unity (URP) plus Cesium for Unity**, and the Rust core is unchanged. See [22 §R.5](22-real-earth-and-realism.md#r5-revised-stack-recommendation). The analysis below is kept for reference.

Covers points 2 (2–3 viable stacks with pros and cons) and 3 (recommended stack, with emphasis on iPhone performance and deployment).

## 2.1 What the stack must do well, in priority order

1. **Multi-scale coordinates and precision** under our control (see [07](07-coordinates-and-time.md)).
2. **Simulation LOD and materialization** as first-class concepts, not bolted onto a scene graph.
3. **Determinism** of the simulation core.
4. **Sustained performance and memory discipline on iPhone** (Metal, unified memory, thermal limits).
5. **Testability in Linux containers** where AI collaborators work, without Xcode.
6. Rendering quality and tooling: important, but secondary given a stylized art direction.
7. Licensing, long-term control, and app size.

## 2.2 Option A: Unity 6 with DOTS (Entities, Burst, Jobs) and URP

**Shape:** Unity editor and runtime; simulation written as DOTS systems in C#, compiled with Burst; rendering through URP and Entities Graphics; iOS export via Xcode project generation.

Advantages:
- The most mature mobile/iOS deployment path of any general-purpose engine; large talent and knowledge pool, which also means AI models have seen a great deal of Unity code.
- DOTS/Burst gives data-oriented, SIMD-friendly code capable of tens of thousands of entities.
- Editor, profiler, asset pipeline, animation, audio, UI, and Shader Graph out of the box: fastest route to good-looking content.
- Large asset store; easy to prototype visuals.

Disadvantages:
- Transforms, physics and rendering are 32-bit float throughout. Large worlds require floating-origin workarounds, and our hierarchical-frame model would sit beside Unity's scene representation rather than inside it, duplicating state.
- Cross-platform float determinism is not guaranteed by Burst today; strict determinism needs custom fixed-point or careful math in a separate layer anyway.
- DOTS has had significant API churn; mixing GameObjects and Entities adds complexity.
- The simulation would be tied to the Unity runtime, making headless Linux testing possible but clumsy (batch mode, licensing in CI) and harder for AI collaborators to run.
- Commercial licence and history of licensing changes; large base app size; the engine is a black box at the points we most need to control (frame scheduling, memory).

## 2.3 Option B: Unreal Engine 5 (C++), Mass Entity, World Partition, Large World Coordinates

**Shape:** UE5 C++ project; agents via Mass Entity (UE's ECS-style crowd framework); world streaming via World Partition; double-precision Large World Coordinates (LWC).

Advantages:
- Native 64-bit world coordinates (LWC) and a rebasing render path, which solve much of the precision problem within one frame.
- Mass Entity and the City Sample show tens of thousands of agents and vehicles with LOD.
- Best-in-class rendering and tools on desktop/console; full engine source available.

Disadvantages:
- **iPhone is a weak target for UE5**: its signature features (Nanite, Lumen) are aimed at desktop/console and are not the mobile path; the mobile renderer is a different, reduced pipeline. Memory footprint, app size, and thermal behaviour are heavy for a sustained simulation game on a phone.
- LWC has large but finite default world bounds and is still a single frame; a full solar system would still need our own frame hierarchy.
- World Partition is designed for authored, pre-cooked worlds, not planet-scale runtime procedural generation.
- Very long C++ compile times and a huge codebase; very heavy to build in AI collaborators' Linux containers (and impossible to produce iOS builds there).
- 5% royalty above the revenue threshold; Blueprint/C++ split adds cognitive load.

## 2.4 Option C: Custom — Rust simulation core plus native Swift/Metal iOS app (recommended)

**Shape:**
- `core/`: a Cargo workspace of small Rust crates implementing coordinates, time, deterministic RNG, world state, procedural generation, simulation and LOD, agents, physics wrappers, interventions and persistence. **No rendering, no platform APIs.** Builds and tests on Linux and macOS; compiled for iOS as a static library (`aarch64-apple-ios`).
- `apps/ios/`: a Swift app (SwiftUI for menus and inspectors, a Metal view for the world) that owns the run loop, input, audio, and rendering. It talks to the core through a narrow C ABI (or UniFFI-generated Swift bindings) that exchanges commands in and snapshots out, never fine-grained per-entity calls.
- `tools/`: a headless CLI runner and a desktop debug inspector (2D maps, graphs, entity inspectors) built in Rust, used on Linux/macOS for development and CI.

Advantages:
- **Full control of the three things that matter most**: coordinates, LOD/materialization, and determinism. Nothing in an engine fights our data model.
- **Linux-testable**: the entire simulation, including multi-year soak tests and determinism tests, runs in CI and in the AI collaborators' containers.
- Rust's type system, memory safety, and lack of a garbage collector are well suited to a long-lived, performance-critical core that is largely AI-written: many classes of bug are compile errors instead of crashes.
- Direct Metal access on iOS: tile-memory (memoryless) render targets, MetalFX upscaling, shared CPU/GPU buffers on unified memory (the core can write instance data straight into an `MTLBuffer` with no copies), and Instruments profiling with full visibility.
- Small app size; no royalties; no engine lock-in.
- **The core is engine-agnostic**, so if the custom renderer proves too costly, a Unity (or other) front end can consume the same core via a native plugin. This keeps a real fallback open.
- Mature Rust ecosystem pieces exist for the parts we should not write ourselves: Rapier (deterministic rigid-body physics with its `enhanced-determinism` feature), `rusqlite` for SQLite, `glam` for SIMD math, `proptest` for property tests, `criterion` for benchmarks, ECS libraries such as `bevy_ecs` (usable standalone) or `hecs`.

Disadvantages:
- **We build the renderer, tools, and asset pipeline ourselves.** This is the largest cost and risk. It is mitigated by: a stylized art direction, procedural geometry (few authored assets), keeping the renderer simple (forward+ clustered lighting), and investing in a desktop debug inspector rather than a full editor.
- iOS builds require macOS (Noah's Mac or macOS CI runners); AI collaborators can write Swift/Metal code but cannot run it.
- Two languages and an FFI boundary; requires discipline about what crosses it (coarse-grained, buffer-based).
- Smaller pool of examples for planet/city rendering in Swift/Metal than in Unity/Unreal.

## 2.5 Considered and not recommended

| Option | Why not (for this project) |
|---|---|
| Godot 4 (with double-precision build) | Open-source and supports 64-bit coordinates, but its mobile renderer and large-scale entity performance are less proven, and the simulation would still need to live outside its scene tree. Reasonable for prototypes, weaker long-term fit. |
| All-Rust engine (Bevy for rendering too) | Attractive single-language story and cross-platform, but iOS is not a first-class Bevy target, the engine is pre-1.0 with frequent breaking changes, and wgpu does not expose Apple-specific features (memoryless attachments, MetalFX) that matter for phone performance. We may still reuse `bevy_ecs` alone. |
| C++ core instead of Rust | Fully viable (Jolt Physics, a huge ecosystem). Rejected on balance because of memory-safety risk in a largely AI-written codebase, weaker build and package tooling, and slower iteration in containers. A reasonable alternative if a future contributor strongly prefers C++. |
| Swift-only (core in Swift) | Excellent iOS integration and Swift runs on Linux, but the Linux ecosystem for this kind of work is thinner, ARC and exclusivity checks add hidden costs in hot loops, and the core would be harder to reuse outside Apple platforms. |

## 2.6 Comparison

| Criterion | A: Unity DOTS | B: Unreal 5 | C: Rust core + Swift/Metal |
|---|---|---|---|
| Multi-scale coordinates | Workarounds over float32 | Good within one frame (LWC) | Designed in |
| Simulation LOD / materialization | Must be built anyway, beside engine state | Mass LOD helps for crowds only | Designed in |
| Determinism | Hard | Hard | Designed in, testable |
| iPhone sustained performance | Good | Weak-to-moderate | Best achievable (direct Metal) |
| App size | Medium | Large | Small |
| Linux/AI-collaborator testability | Awkward | Poor | Excellent |
| Rendering and tools out of the box | Excellent | Excellent | Must be built |
| Time to first good-looking demo | Fast | Fast (desktop) | Slow |
| Long-term control and cost | Vendor-dependent | Royalty, heavy | Full control |

## 3. Recommendation

**Adopt Option C: a Rust simulation core embedded in a native Swift/Metal iOS app**, with these explicit commitments:

1. **The core is headless and engine-agnostic.** It exposes: `submit(commands)`, `advance(time budget)`, `snapshot(view request) -> render snapshot`, `query(inspector request)`, `save()/load()`. The iOS app never mutates simulation state except through commands.
2. **The FFI boundary is coarse-grained and buffer-based.** One call per frame to fill render buffers; no per-entity calls across the boundary.
3. **Stylized art direction** (flat-shaded or softly lit, low-texture, readable silhouettes, strong lighting and atmosphere). This is what makes a custom renderer tractable and what keeps the frame budget on a phone.
4. **Desktop tooling in Rust** (headless runner and debug inspector) is part of Phase 0, not a later extra.
5. **Re-evaluation gate at the end of Phase 1** (see [19](19-roadmap.md)): if the custom renderer is consuming disproportionate effort, switch the front end to Unity consuming the same core via native plugin. Because of point 1, this costs the renderer, not the simulation.

### iPhone deployment specifics

- Build the core with `cargo` for `aarch64-apple-ios` (device) and `aarch64-apple-ios-sim` (simulator on Apple-silicon Macs), packaged as an XCFramework consumed by the Xcode project.
- CI: Linux runners for all core tests; a macOS runner job that builds the XCFramework and the iOS app and runs XCTest on the simulator. Device performance tests run on Noah's devices (later, optionally a device farm).
- Distribution through TestFlight from the first prototype, so real-device performance is measured continuously rather than at the end.
- Minimum target proposed in [15](15-ios-performance-memory.md): A17 Pro class (iPhone 15 Pro) and newer, latest major iOS minus one.
