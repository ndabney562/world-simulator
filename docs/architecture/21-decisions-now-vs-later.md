# 22. Decisions to Make Now vs. Deliberately Postpone

Covers point 22.

The test for "decide now": **would changing this later force a rewrite of large parts of the core, or invalidate saves and tests?** If yes, decide now and record an ADR in [`docs/adr/`](../adr/). If no, postpone until real data (measurements, playtests) exists, and keep the decision behind an interface.

## 22.1 Decided (ADRs accepted)

| ID | Decision | ADR |
|---|---|---|
| D-1 | **Stack:** Rust simulation core → Unity 6 / URP front end → Cesium for Unity where useful → iPhone | 0005 |
| D-2 | **Core/front-end separation:** headless core; commands in, snapshots and query results out; a coarse-grained C ABI | 0005 |
| D-3 | **Reproducibility policy:** tiered. Exact for identity and structure everywhere; replay-exact on the same build and platform; bounded differences elsewhere ([25](25-determinism-policy.md)) | 0010 |
| D-9 | **Minimum device:** iPhone 15 Pro (8 GB), current and previous iOS | 0004 |
| D-11 | **Real Earth:** open data for simulation, synthetic people | 0001 |
| D-12 | **Realistic visuals** within iPhone limits | 0002 |
| D-13 | **Single-player** | 0003 |
| D-16 | **Zero cost:** free tools and open data; Google optional and capped; Apple's $99 per year is the known exception | 0007 |
| D-17 | **MVP area:** Bay Ridge, Brooklyn | 0006 |
| D-18 | **Visual providers are optional and swappable**, never part of simulation state ([23](23-visual-providers.md)) | 0008 |
| D-19 | **Stage progression:** Technical Prototype → Vertical Slice → Neighbourhood MVP → City → Planet → Solar System ([19](19-roadmap.md)) | 0009 |
| D-20 | **World event and history system** as a first-class, shared record ([24](24-world-events-and-causality.md)) | 0011 |
| D-21 | **Practical causal provenance** for interventions and major events (direct links, bounded tags, aggregate attribution, optional counterfactuals) | 0012 |

## 22.2 Decide now (proposed; to be written as ADRs at the start of the Technical Prototype)

| ID | Decision | Proposed answer | Why it cannot wait |
|---|---|---|---|
| D-4 | Coordinate system | Frame hierarchy; ECEF/WGS84 body-fixed frame; S2-style cell IDs; cell-local 32-bit offsets; ellipsoidal heights; camera-relative GPU data ([07](07-coordinates-and-time.md)) | Positions appear in every system, save, and test |
| D-5 | Time representation | 64-bit integer microseconds from J2000; real UTC calendar; multi-rate ticks | Same reason |
| D-6 | World-state model | Baseline (dataset version + seed + generator versions) + canonical deltas + journal + event log ([08](08-world-state.md)) | Defines saves, generation, and LOD |
| D-7 | Entity identity | Tier 1 deterministic procedural IDs, persistent promotion, a separate namespace for spawned entities ([09](09-entities-and-agents.md); detailed in [ADR 0016](../adr/0016-entity-identity-storage-and-randomness.md)) | Changing it breaks saves and zoom consistency |
| D-8 | Mutation model | Validated, journaled commands; interventions composed from an effect vocabulary ([13](13-god-interventions.md)) | Underpins saves, replay, undo, LLM integration, and testing |
| D-10 | LOD contract | Each domain defines aggregation, disaggregation, invariants, and event roll-up per level ([05](05-simulation-lod.md); L1 ↔ L2 detailed in [ADR 0017](../adr/0017-simulation-lod-l1-l2-round-trip.md)) | The defining architectural rule |
| D-14 | Versioning discipline | Save format, rules, generator, and dataset versions recorded from the first save; golden tests | Cannot be added to existing saves retroactively |
| D-15 | Process | ADRs, cross-review between AI collaborators, Noah as approver, `main` always green | Needed before multiple contributors write code |
| D-22 | Data-source register | Every source has licence, attribution, and version recorded in `docs/data/`; Google never used as simulation input | Licence mistakes are hard to unwind |

## 22.3 Decide in the Technical Prototype via short spikes (then lock)

| ID | Decision | Options |
|---|---|---|
| S-1 | ECS library | `bevy_ecs` standalone, `hecs`, or a minimal custom store behind `ws-ecs`. Technical Prototype: minimal custom store ([ADR 0016](../adr/0016-entity-identity-storage-and-randomness.md)) |
| S-2 | Binary serialization format | `serde` with a compact codec, FlatBuffers, Cap'n Proto; must support schema evolution |
| S-3 | Rust ↔ Unity bridge details | A hand-written C ABI with `cbindgen` plus C# bindings (hand-written or generated); native buffers shared without copies. [ADR 0018](../adr/0018-rust-unity-bridge.md) proposes the TP-003 contract; acceptance pending |
| S-4 | Sector cell level | ~1 km (level 13) proposed; confirm with materialization cost measurements |
| S-5 | Frame-rate target | 30 fps sustained with opportunistic 60, or 60 sustained with lower budgets |
| S-6 | Unity CI | Unity iOS builds in CI on the free plan, or local builds on Noah's Mac |
| S-7 | Event store layout | SQLite table and index design, compaction cadence, provenance-tag cap and decay constants |
| S-8 | Vertical Slice block | Which exact Bay Ridge block, chosen for data quality |

## 22.4 Deliberately postponed

| Topic | Why postponing is safe or better | Revisit |
|---|---|---|
| Economy model details | Needs the LOD contract and playtesting first; isolated in its own domain crate | Neighbourhood MVP to City |
| Weather and climate models | Coarse environment fields suffice until the Planet stage | Planet |
| Vehicle simulation depth | Not needed until the City stage | City |
| Interiors | On-demand generation fits the existing pipeline later | City and beyond |
| Advanced AI (GOAP, deep social simulation) | Utility AI and schedules cover the early stages; minds are layered | City and beyond |
| LLM provider and features | Already isolated; the landscape changes fast | After the Neighbourhood MVP |
| Generator and dataset migration policy | Versions are recorded now; the policy is chosen at the first breaking change | First breaking change |
| Counterfactual explanation UI | The event model supports it; cost and value are better judged later | City |
| Planet-wide data pipeline and hosting | Staged by region; costs reviewed against ADR 0007 when needed | City, Planet |
| N-body or non-ephemeris orbital mechanics | Ephemerides suffice; interventions use parametric models | Solar System |
| Multiplayer | Out of scope (ADR 0003) | Only if the vision changes |
| Other platforms (iPad, Mac, Android) | Unity makes them cheaper later | After the Neighbourhood MVP |
| Monetisation, modding, user-generated content | Product decisions | After the Neighbourhood MVP |
| iCloud sync, shareable worlds, replay features | Built on the persistence model without changing it | City and beyond |
| Audio architecture | Independent of the core | Neighbourhood MVP |
| Undo as a player feature | Supported internally; a design call | Neighbourhood MVP design |

## 22.5 Open questions for Noah

1. Exact Neighbourhood MVP bounds (proposed: around 3rd–5th Avenue and 75th–86th Street).
2. ~~Whether to create a `main` branch~~ Resolved: `main` was created at the Architecture V1 freeze (PR #1).
3. Committing the Project Instructions to `docs/vision/`, so that both AI reviewers work from the same text.
