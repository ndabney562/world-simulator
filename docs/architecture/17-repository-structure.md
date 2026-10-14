# 18. Proposed GitHub Repository Structure

Covers point 18. This is a proposal; no directories other than `docs/` are created yet.

## 18.1 Layout

```
world-simulator/
├── README.md                     Project overview and pointers
├── CLAUDE.md / AGENTS.md         Conventions for AI collaborators (added in Phase 0)
├── docs/
│   ├── vision/                   Project instructions / vision (source of truth for intent)
│   ├── architecture/             This proposal, kept current as the design evolves
│   ├── adr/                      Architecture Decision Records: NNNN-title.md
│   ├── design/                   Gameplay and content design notes
│   └── reviews/                  Independent reviews (e.g. ChatGPT's review of proposals)
├── core/                         Rust Cargo workspace (headless simulation)
│   ├── Cargo.toml                Workspace manifest
│   └── crates/
│       ├── ws-math/              Frames, typed positions, cell IDs, transforms
│       ├── ws-time/              Simulation time, calendars, tick alignment
│       ├── ws-rng/               Deterministic hashing and RNG streams
│       ├── ws-ecs/               Thin wrapper over the chosen ECS library
│       ├── ws-world/             World tree, nodes, aggregates, deltas, history log
│       ├── ws-procgen/           Generators (system, planet, terrain, city, buildings, population)
│       ├── ws-agents/            Agent components, schedules, utility AI, social layer
│       ├── ws-physics/           Physics bubble (Rapier wrapper), orbits, parametric models
│       ├── ws-lod/               Interest manager, LOD levels, materialization pipeline
│       ├── ws-interventions/     Effect vocabulary and intervention application
│       ├── ws-sim/               Scheduler, command queue, systems registration, journal
│       ├── ws-persist/           SQLite storage, serialization, migrations
│       ├── ws-snapshot/          Render and inspector snapshot builders (format shared with app)
│       ├── ws-ffi/               C ABI / Swift bindings; the only crate the app links
│       └── ws-testkit/           Scenario DSL, invariant checkers, fixtures for tests
├── tools/
│   ├── ws-cli/                   Headless runner: create worlds, run N years, replay, benchmark
│   └── ws-inspector/             Desktop debug viewer (2D maps, graphs, entity inspector)
├── apps/
│   └── ios/
│       ├── WorldSimulator.xcodeproj
│       ├── App/                  Swift app shell, SwiftUI screens, input, settings
│       ├── Renderer/             Metal renderer (Swift) and shaders (.metal)
│       ├── Bridge/               Swift wrapper over ws-ffi
│       └── Tests/                XCTest unit, UI and performance tests
├── content/
│   ├── rules/                    Versioned rule data (needs, jobs, building types, effects)
│   ├── generators/               Generator parameters, grammars, name tables
│   └── art/                      Modular meshes, small textures, materials (source + exported)
├── tests/
│   ├── scenarios/                Cross-crate scenario files (headless)
│   ├── golden/                   Golden hashes for generators and determinism
│   └── saves/                    Fixture saves for migration tests
├── benchmarks/                   Benchmark definitions and recorded baselines
├── scripts/                      Build helpers (XCFramework build, CI scripts)
└── .github/
    ├── workflows/                CI: core (Linux), core (macOS), iOS build + simulator tests
    └── pull_request_template.md
```

## 18.2 Rules for the structure

1. **Dependency direction is one-way:** `ws-math`/`ws-time`/`ws-rng` → `ws-world` → domain crates → `ws-sim` → `ws-ffi`. Domain crates do not depend on each other except through `ws-world` types and events. Enforced by Cargo (no cyclic dependencies possible) and reviewed in ADRs when a new edge is needed.
2. **Nothing in `core/` depends on rendering or platform APIs.** It builds and tests on Linux.
3. **`ws-ffi` is the only boundary** the iOS app sees. Its surface is reviewed carefully because it is expensive to change.
4. **Content is data**, versioned, and validated by tests (schema checks) in CI.
5. **Each crate has its own README** stating its responsibility, its owned state, and its invariants. This matters for AI collaborators, who often work with partial context.

## 18.3 Branching and review workflow (proposed)

- `main` is always green (CI passing).
- Work happens on short-lived branches with pull requests; each PR references the ADR or roadmap item it implements.
- Architecture changes require an ADR PR first. Independent review by the other AI collaborator (Claude ↔ ChatGPT) for ADRs and for PRs that touch `ws-math`, `ws-sim`, `ws-lod`, `ws-persist` or `ws-ffi`, with Noah as final approver.
- Reviews of proposals go in `docs/reviews/` so the reasoning is preserved in the repository.
