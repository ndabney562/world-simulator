# 18. Proposed GitHub Repository Structure

Covers point 18. This is a proposal: only `docs/` exists today. Directories are created when the Technical Prototype starts.

## 18.1 Layout

```
world-simulator/
├── README.md                     Project overview and pointers
├── CLAUDE.md / AGENTS.md         Conventions for AI collaborators (Technical Prototype)
├── docs/
│   ├── vision/                   Project instructions / vision (source of truth for intent)
│   ├── architecture/             This proposal, kept current; CHANGELOG.md per revision
│   ├── adr/                      Architecture Decision Records: NNNN-title.md
│   ├── design/                   Gameplay and content design notes
│   ├── data/                     Data-source register: licences, attribution, versions
│   └── reviews/                  Independent reviews (e.g. ChatGPT's review of proposals)
├── core/                         Rust Cargo workspace (headless simulation)
│   ├── Cargo.toml
│   └── crates/
│       ├── ws-math/              Frames (ECEF/WGS84, ENU), typed positions, S2-style cell IDs
│       ├── ws-time/              Simulation time, UTC calendar, time zones, astronomical time
│       ├── ws-rng/               Counter-based hashing and RNG streams (Tier 1 safe)
│       ├── ws-ecs/               Thin wrapper over the chosen ECS library
│       ├── ws-world/             World tree, nodes, aggregates, deltas, environmental fields
│       ├── ws-events/            World event model, taxonomy, event store, causal links,
│       │                         provenance tags, attribution, history and causal queries
│       ├── ws-data/              Simulation-tile reader (dataset versions, schemas)
│       ├── ws-procgen/           Runtime generation: population synthesis, schedules,
│       │                         social graphs, facades, interiors, gap filling
│       ├── ws-agents/            Agent components, schedules, utility AI, social, memory
│       ├── ws-physics/           Physics bubble (Rapier), ephemerides, parametric models
│       ├── ws-lod/               Interest manager, LOD levels, materialization pipeline
│       ├── ws-interventions/     Effect vocabulary, intervention application, provenance
│       ├── ws-sim/               Scheduler, command queue, journal, system registry
│       ├── ws-persist/           SQLite storage, serialization, migrations
│       ├── ws-snapshot/          Render, inspector, and query snapshot builders
│       ├── ws-ffi/               C ABI; the only crate Unity links
│       └── ws-testkit/           Scenario DSL, invariant checkers, determinism tiers, fixtures
├── tools/
│   ├── ws-cli/                   Headless runner: create worlds, run N years, replay, benchmark
│   ├── ws-inspector/             Desktop debug viewer (maps, graphs, entity and event browser)
│   └── ws-datapipe/              Offline data pipeline: sources → simulation and visual tiles
├── apps/
│   └── unity/                    Unity 6 project (URP)
│       ├── Assets/WorldSim/
│       │   ├── Bridge/           C# P/Invoke bindings over ws-ffi; snapshot decoding
│       │   ├── Rendering/        Simulation-geometry layer, agents, overlays, LOD
│       │   ├── Providers/        Visual backdrop providers (open data, Google, NASA) and
│       │   │                     provider manager, quota, attribution
│       │   ├── UI/               Inspector, history and causality views, time controls
│       │   └── Tests/            Unity edit-mode and play-mode tests
│       ├── Assets/Plugins/iOS/   Rust static library (build output, not committed)
│       ├── Packages/             Cesium for Unity and other packages (manifest)
│       └── ProjectSettings/
├── content/
│   ├── rules/                    Versioned rule data (needs, jobs, building uses, effects,
│   │                             event types and importance weights)
│   ├── generators/               Grammars, regional facade styles, name distributions
│   └── art/                      Characters, modular facade parts, materials
├── data/
│   └── manifests/                Dataset version manifests (sources, hashes). Processed tiles
│                                 are build artifacts or release assets, not committed to git
├── tests/
│   ├── scenarios/                Headless scenario files
│   ├── golden/                   Golden hashes (Tier 1 shared; Tier 2 per platform)
│   └── saves/                    Fixture saves for migration tests
├── benchmarks/                   Benchmark definitions and recorded baselines
├── scripts/                      Build helpers (iOS static library, CI scripts)
└── .github/
    ├── workflows/                CI: core (Linux), core (macOS arm64), Unity iOS build
    └── pull_request_template.md
```

## 18.2 Rules for the structure

1. **Dependency direction is one-way:** `ws-math`, `ws-time`, `ws-rng` → `ws-world` and `ws-events` → domain crates → `ws-sim` → `ws-ffi`. Domain crates depend on each other only through `ws-world` and `ws-events` types.
2. **Nothing in `core/` depends on Unity, rendering, platform APIs, or visual providers.** It builds and tests on Linux.
3. **`ws-ffi` is the only boundary** Unity sees; its surface is reviewed carefully because it is expensive to change.
4. **Google or other provider code lives only in `apps/unity/Assets/WorldSim/Providers/`**, never in `core/` or `tools/ws-datapipe/` ([23](23-visual-providers.md)).
5. **Content and data manifests are versioned and validated** by tests in CI. Large processed data is not committed; manifests and hashes are.
6. **Each crate has a README** stating its responsibility, owned state, emitted event types, and invariants, for AI collaborators working with partial context.
7. **Every data source is registered** in `docs/data/` with licence, attribution text, and version.

## 18.3 Branching and review workflow

- `main` is always green once it exists. (The repository currently has only the architecture branch; creating `main` from the accepted proposal is the first step of the Technical Prototype.)
- Work happens on short-lived branches with pull requests; each references the ADR or roadmap item it implements.
- Architecture changes need an ADR PR first. ADRs and PRs that touch `ws-math`, `ws-events`, `ws-sim`, `ws-lod`, `ws-persist`, or `ws-ffi` get independent review by the other AI collaborator (Claude and ChatGPT), with Noah as final approver.
- Reviews of proposals go in `docs/reviews/`.
