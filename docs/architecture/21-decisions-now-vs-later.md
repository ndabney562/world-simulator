# 22. Decisions to Make Now vs. Deliberately Postpone

Covers point 22.

The test for "decide now": **would changing this later force a rewrite of large parts of the core, or invalidate saves and tests?** If yes, decide now and record an ADR in `docs/adr/`. If no, postpone until real data (measurements, playtests) exists, and keep the decision behind an interface.

## 22.1 Decide now (Phase 0 ADRs)

| ID | Decision | Proposed answer | Why it cannot wait |
|---|---|---|---|
| D-1 | Technology stack | Rust headless core + **Unity/URP + Cesium for Unity** iOS front end (revised; [22 §R.5](22-real-earth-and-realism.md)) | Everything else is written in it. |
| D-2 | Core/renderer separation | Headless core; commands in, snapshots out; coarse-grained, buffer-based FFI | Retrofitting separation is a rewrite. It is also what keeps the Unity fallback open. |
| D-3 | Determinism policy | Same-build, same-architecture determinism guaranteed and tested; counter-based RNG streams; soft transcendental math; integer time; canonical iteration order | Cannot be retrofitted; every system must follow it from line one. |
| D-4 | Coordinate system | Frame hierarchy; 64-bit floats per frame; cube-sphere cell IDs; cell-local 32-bit offsets; camera-relative GPU data; spherical bodies ([07](07-coordinates-and-time.md)) | Positions appear in every system, every save, every test. |
| D-5 | Time representation | 64-bit integer microseconds; derived calendars; multi-rate ticks | Same reason. |
| D-6 | World-state model | Seed + generator versions + canonical deltas + journal; three kinds of state ([08](08-world-state.md)) | Defines saves, procgen, and LOD. |
| D-7 | Entity identity | Deterministic procedural IDs, persistent promotion, separate namespace for spawned entities ([09](09-entities-and-agents.md)) | Changing it breaks saves and consistency across zoom levels. |
| D-8 | Mutation model | All external changes are validated, journaled commands; interventions are data composed from an effect vocabulary | Underpins saves, replay, undo, LLM integration, and testing. |
| D-9 | Minimum device | **Decided:** iPhone 15 Pro and newer, 8 GB RAM; current and previous iOS (ADR 0004) | Sets every budget. (Can be relaxed later, not tightened easily.) |
| D-10 | LOD contract | Every domain defines aggregation, disaggregation, and invariants per level before it can cross levels ([05](05-simulation-lod.md)) | The defining architectural rule; must be enforced from the first domain. |
| D-11 | Real Earth vs. fictional world | **Decided: real Earth** (ADR 0001). Open data for simulation, Google 3D Tiles for display, synthetic people | Changes the data pipeline, licensing, storage, and scope; see [22](22-real-earth-and-realism.md). |
| D-12 | Art direction | **Decided: realistic** (ADR 0002). Hybrid photoreal backdrop plus realistic near-field geometry, within iPhone limits | Determines renderer and stack. |
| D-13 | Single-player | **Decided: single-player only** (ADR 0003). The simulation runs offline; photoreal visuals need a network connection | Multiplayer would change persistence and networking design. |
| D-16 | Data sources and the Google terms boundary | Google data for display only; OSM/Overture, Copernicus DEM, GHSL/WorldPop, and ESA WorldCover for simulation; pinned dataset versions | Mixing derived Google data into the simulation would violate its terms and is hard to unwind. |
| D-14 | Versioning discipline | Save format, rules, and generator versions recorded from the first save; golden tests for generators | Cheap now, impossible to add retroactively to existing saves. |
| D-15 | Process | ADRs for architecture changes; cross-review between AI collaborators; Noah as approver; `main` always green | Needed before multiple contributors start writing code. |

## 22.2 Decide in Phase 0 via a short spike (then lock)

| ID | Decision | Options |
|---|---|---|
| S-1 | ECS library | `bevy_ecs` standalone, `hecs`, or a minimal custom store, behind `ws-ecs` |
| S-2 | Binary serialization format | `serde` + a compact binary codec, FlatBuffers, Cap'n Proto; must support schema evolution |
| S-3 | Rust ↔ Swift bridge | Hand-written C ABI with `cbindgen`, or UniFFI; must support shared Metal buffers without copies |
| S-4 | Sector cell level | ~1 km (level 13 on an Earth-sized planet) proposed; confirm with materialization cost measurements |
| S-5 | Frame-rate target | 30 fps sustained with opportunistic 60, or 60 sustained with lower budgets; decided from device measurements |

## 22.3 Deliberately postpone

| Topic | Why postponing is safe or better | Revisit |
|---|---|---|
| Economy model details | Needs the LOD contract and playtesting first; isolated behind its own domain crate | Phase 2–3 |
| Specific weather and climate models | Coarse environment fields are enough until the planet phase | Phase 4 |
| Vehicle simulation depth | Not needed until the city phase | Phase 3 |
| Interiors | On-demand generation fits the existing pipeline later | Phase 3+ |
| Advanced AI (GOAP, deep social simulation) | Utility AI and schedules cover the MVP; minds are layered so more can be added | Phase 3+ |
| LLM provider and features | The architecture already isolates them; provider landscape changes fast | After MVP |
| Generator-version migration policy (keep old generators vs. migrate) | Versioning is recorded now; the policy can be chosen when the first breaking generator change happens | First breaking change |
| N-body or non-Keplerian orbital mechanics | Analytic orbits suffice; interventions use parametric models | Phase 5 |
| ~~Ellipsoidal planets, real ephemerides~~ | Now required for real Earth (WGS84, real ephemerides); see [22 §R.6](22-real-earth-and-realism.md) | Phase 0 / Phase 5 |
| Multiplayer | Assumed out of scope; journals keep the door open | Only if the vision changes |
| Other platforms (iPad, Mac, Android) | iPad/Mac are cheap later with Swift/Metal; Android needs a new front end | After MVP |
| Monetisation, modding, user-generated content | Product decisions; modding would benefit from the data-driven rules already planned | After MVP |
| iCloud sync, shareable worlds, replays as features | Built on the persistence model without changing it | Phase 3+ |
| Audio architecture | Independent of the simulation core | Phase 2+ |
| Undo as a player feature | Supported internally; whether players get it is a design call | Phase 2 design |

## 22.4 Questions for Noah

Answered 2026-10-14: real Earth, realistic, single-player, iPhone 15 Pro minimum (ADRs 0001–0004). Then: MVP area Bay Ridge, stack accepted, zero cost (ADRs 0005–0007). Questions answered in that second round:

1. Which real neighbourhood should the MVP use?
2. Do you accept the revised stack (Rust core + Unity + Cesium)?
3. Are you OK with Google Maps Platform usage costs and a network requirement for photoreal visuals?

Original questions (answered):

1. **D-11:** Is the world fictional and procedurally generated, or must it be based on real Earth?
2. **D-12:** Is a stylized look acceptable, or is realism a core goal?
3. **D-13:** Single-player only?
4. **D-9:** Is iPhone 15 Pro and newer an acceptable minimum?
5. Can the full Project Instructions / vision text be committed to `docs/vision/` so both AI reviewers work from the same source?
