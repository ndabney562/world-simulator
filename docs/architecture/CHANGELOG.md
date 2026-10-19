# Architecture Proposal Changelog

## ADR 0025 accepted (2026-10-19)

Noah’s direct standing technical approval is recorded on #77 and PR #83. ADR 0025 and its index now say Accepted. No runtime format changes ship here; #53 remains open for code, fixture/continuation/migration coverage and measurements, with #75 retaining whole-slice integration.

## ADR 0025: baseline-plus-deltas saves (2026-10-19, Proposed)

Docs-only proposal for #53: pin full baseline identity, store complete canonical component replacements/notables/runtime state, validate and rebuild before publication, and preserve format-1/2 fixtures through explicit migration. Adds a pending-proposal pointer in persistence architecture. No format or behavior changes; accepted ADRs 0020/0021 remain active. VS-011 keeps whole-slice integration/background-write scope.

## ADR 0024 accepted (2026-10-19)

Noah’s direct “Always approve” instruction delegates technical acceptance and merges for the current backlog, recorded on #77 and PR #82. ADR 0024 and its index now say Accepted. Implementation, City-scale feasibility measurements and versioned save integration remain separate work; current generators are not silently changed.

## ADR 0024: procedural ID collisions (2026-10-19, Proposed)

Docs-only proposal for #30: allocate over a frozen complete generation roster, reserve all primary IDs before canonical retries, and persist sparse losing-address overrides. Preserves collision-free IDs and records the explicit cost/streaming limitation. Adds a pending-proposal pointer in entity architecture; accepted ADR 0016 and runtime behavior remain unchanged. City-scale feasibility and format migration require separate evidence and acceptance.

## ADR 0022: toy-world render snapshot and ABI 3 (2026-10-17, Accepted)

Design phase of TP-011 (#12): ABI 3 replaces the debug markers with building and resident records (kinds 1 and 2) in a fixed-origin ENU frame, changes the config to a residents-per-building count, keeps the fill contract, and records the single-worker-thread host rule. Docs only.

## TP-010 implementation of ADR 0021 (2026-10-17)

Implements #11: the `Intervene` command and `afflict` effect, the `illness` column and the shared L1/L2 return-home rule in `ws-world`, three event payloads and `root_intervention` in `ws-events`, `World::why_ill` and timed interventions in `ws-sim`, save format 2 in `ws-persist` (format-1 files still load; `toy-v2.wssave` fixture), and the `afflict` script verb in `ws-cli`. ADR 0021 gained an implementation-notes paragraph (wire details, how a timed intervention waits, event system numbers, measured cost); no decision changed.

## ADR 0021: first god intervention (2026-10-17, Accepted)

Design phase of TP-010 (#11): decides the `Intervene` command, effect vocabulary v1 (`afflict(illness, severity)`), validation, a single `illness` column that makes LOD and save/load persistence structural, the shared L1/L2 return-home rule, the three event types and links, the causal query, and save format 2 (v1 files still load). Docs only.

## TP-009 implementation of ADR 0020 (2026-10-17)

Implements #10: the `ws-persist` crate (SQLite save container, hand-written canonical codec, version header, load validation), restore support in `ws-world` (`ToyParts`), `ws-sim` (`WorldParts`, `Clock::from_parts`) and `ws-cli` (`save` and `load`), and the format-1 fixture under `tests/saves/`. As ADR 0020 decided, `ws-persist` moves to its own layer above `ws-sim`; the layer table in `scripts/check_core_deps.py` and `docs/process/ci.md`, and the dependency direction in [17 §18.2](17-repository-structure.md) and `AGENTS.md`, now read `ws-sim` → `ws-persist` → `ws-ffi`. Two clarifications of the ADR, both within its load-validation paragraph: the header also records journal and event digests (a lost causal link does not change the state hash), and `buildings` and `l1_state` are keyed by (cell, entity ID) while the other record tables are keyed by entity ID. No accepted decision is changed.

## ADR 0020: save format and version header (2026-10-17, Accepted)

Design phase of TP-009 (#10), accepted by Noah on 2026-10-17: decides the SQLite save container, S-2 as a hand-written versioned canonical codec (compared with `serde`/`postcard`, FlatBuffers and Cap'n Proto), the D-14 version header, load-time validation, and `ws-persist` moving above `ws-sim` in the crate layers because it must construct a `World`. Adds pointers in persistence and the decisions table. No code, dependency, CI or layer-table change; those come with the implementation PR after acceptance.

## TP-008 repository layout correction (2026-10-17)

The repository tree now places the headless host at `core/crates/ws-cli/`, as accepted ADR 0019 §1 specifies; `tools/` remains for the inspector and data pipeline. No dependency direction or architecture decision changed.

## ADR 0019 accepted (2026-10-16)

Noah explicitly approved ADR 0019 and assigned #9 implementation in Codex; approval is recorded on #9 and PR #44. Status/index now say Accepted. Implementation follows on a separate branch; no PR merge was performed.

## ADR 0019: command journal and World Events (2026-10-16, Proposed)

Design phase of #9, tracked by #43: proposes serialized command processing at boundaries (including paused flushes), journal outcomes, atomic event publication, deterministic sequential 64-bit event IDs, initial event schemas and queries, logical separate-process replay, and an explicit ABI-v2 migration after TP-003 review. Adds pointers in simulation architecture, event architecture, determinism policy and D-8. Review corrections bound a full-size replay Submit record at 65,600 bytes (including framing/metadata) and specify explicit prefix/zero-commit fences for replaying environmental flush/tick stops without assuming allocator failures recur. No code, accepted ADR, dependency, save format or CI behavior changes; implementation follows acceptance in a separate PR. ADR 0018 remains reserved by PR #36 and is not copied into this branch.

## ADR 0018 generator-licence correction (2026-10-16)

Noah explicitly approved the pinned cbindgen 0.29.2 MPL-2.0 build-tool exception and correction, recorded on PRs #36 and #42. Corrects the earlier permissive-licence description: local/CI header generation/checking only, no runtime linkage or app distribution of the tool, and no automatic exception for other tools or upgrades. The ABI, runtime dependencies and general licence policy are unchanged.

## ADR 0018 accepted; TP-003 implementation (2026-10-16)

Noah explicitly accepted the bridge design in Codex (recorded on PR #36). TP-003 implements its synchronous native API, debug snapshots, managed ownership/decoding and Bootstrap renderer; real-entity switch-over remains TP-011. Updated pending proposal pointers. Implementation remains subject to independent PR review and Noah merge.


## ADR 0018: TP-003 Rust–Unity bridge (2026-10-16, Proposed)

Proposes S-3: versioned hand-written C ABI and generated header, caller-owned snapshot bytes, serialized handle ownership, errors/panic containment, explicit debug-marker layout, and C# lifecycle. Adds pointers in the simulation API and S-3 decision table. This docs-only proposal does not accept the decision or implement #4.

## ADR 0017 accepted; implemented by TP-007 (2026-10-15)

The Product Owner accepted ADR 0017 by merging PR #26 after ChatGPT's review, and confirmed the acceptance on 2026-10-15; its status line and the ADR index now say Accepted (in the TP-007 implementation PR for issue #8). The decision is unchanged. The implementation adds the L1 ↔ L2 round trip to `ws-world` (`lod` module: `Background`, node levels, the `set_lod` transaction, the L2 event drain, the invariant checker) and the canonical tick order and start alignment to `ws-sim`. Besides the event queue, it keeps one more derived index of the kind §10 allows (the agents awaiting reconciliation), so a tick does not scan every agent; it is rebuildable, checked, and not hashed.

## ADR 0017: simulation LOD, the L1 ↔ L2 round trip (2026-10-15, Proposed)

ADR 0017 prepares TP-007 (#8). It details D-10 for the first pair of levels: L2 is a discrete-event schedule (waiting at a building until a departure time, or walking a leg with an integer arrival time and one exact keyframe for a leg interrupted by demotion); buildings are LOD nodes and an agent is governed by its destination building; identity is stored once and residency is membership of mutually exclusive L1 and L2 stores; transitions are validated, atomic transactions at tick boundaries in ascending ID order; L1 → L2 → L1 at the same boundary is bit-exact, population and identity are always exact, and timing over elapsed time is toleranced; randomness is indexed by per-entity event counters carried at both levels. It also resolves the start-alignment caveat from TP-004: `WorldConfig::start` must lie on the base-step grid. `Walk::velocity` is reclassified as derived and leaves the state hash.

- Pointers added to [05-simulation-lod.md](05-simulation-lod.md) §6.3, [09-entities-and-agents.md](09-entities-and-agents.md) §10.3, [12-materialization.md](12-materialization.md) §13.1, [21-decisions-now-vs-later.md](21-decisions-now-vs-later.md) (D-10), and [07-coordinates-and-time.md](07-coordinates-and-time.md) §8.9.

## ADR 0016 accepted; implemented by TP-006 (2026-10-15)

The Product Owner accepted ADR 0016 by merging PR #24 after ChatGPT's review; its status line and the ADR index now say Accepted (in the TP-006 implementation PR, #7). The decision is unchanged. The implementation adds `ws-rng`, `ws-ecs` (a foundation crate, now also in the `scripts/check_core_deps.py` layer table and [ci.md](../process/ci.md)), and `ws-world`, and applies the cell-edge re-expression rule in `ws-math`.

## ADR 0016: entity identity, storage, and randomness (2026-10-15, Proposed)

ADR 0016 prepares TP-006 (#7). It fixes D-7 (64-bit `EntityId` with two namespace bits; procedural IDs hashed from world seed, dataset version, and a typed generation address; a counter namespace for spawned entities), closes S-1 for the Technical Prototype (a minimal sorted-column store in `ws-ecs`, iterated in ascending ID order, with no ECS library), defines the counter-based randomness model in `ws-rng` (SipHash-2-4 keyed by parent seed and purpose; streams as a key plus an explicit counter), and settles the cell-edge re-expression rule ADR 0015 left to TP-006 (every re-expressed `SurfacePos` lies in its own cell). `ws-ecs` moves to the foundation layer.

- [17-repository-structure.md](17-repository-structure.md) §18.1–18.2: `ws-ecs` described as the entity ID and component store, and listed with the foundation crates.
- Pointers added to [09-entities-and-agents.md](09-entities-and-agents.md) §10.1–10.2, [21-decisions-now-vs-later.md](21-decisions-now-vs-later.md) (D-7, S-1), and [07-coordinates-and-time.md](07-coordinates-and-time.md) §8.3.

## ADRs 0014 and 0015 accepted (2026-10-15)

The Product Owner accepted ADR 0014 (time representation, D-5) and ADR 0015 (coordinate representation, D-4) by merging their implementation PRs, #18 (TP-004) and #19 (TP-005). Their status lines and the ADR index now say Accepted (#22). The decisions are unchanged; S-4 (sector level) stays open as ADR 0015 states.

## ADR 0015: coordinate representation (2026-10-15, Proposed)

- ADR 0015 fixes D-4 for TP-005: SI units, right-handed ENU frames, ellipsoidal heights, frame-tagged types, S2-compatible cell IDs computed with portable `libm` math for Tier 1 exactness, and level 13 as the default sector level (S-4 still to be confirmed). Pointer added to [07-coordinates-and-time.md](07-coordinates-and-time.md) §8.3.

## ADR 0014: time representation (2026-10-14, Proposed)

TP-004 (#5) details D-5: simulation time is a uniform scale of i64 microseconds from 2000-01-01T12:00:00 UTC; UTC labels come from an embedded, versioned leap-second table; host wall-clock measurements enter only as integer inputs. Pointer added to [07-coordinates-and-time.md](07-coordinates-and-time.md) §8.9.

## Architecture V1 freeze (2026-10-14)

Revision 4 (`fe2f3db`) was frozen as Architecture V1 and merged into `main` (PR #1). Later changes follow [docs/process/architecture-change-process.md](../process/architecture-change-process.md).

- ADR 0013: geospatial and visual-provider work (Cesium, open data, Google) is a later Technical Prototype spike and does not block proof of the Rust → FFI → Unity → iPhone path. Note added to [20-mvp.md](20-mvp.md) §21.2.
- [17-repository-structure.md](17-repository-structure.md) §18.3 and [21-decisions-now-vs-later.md](21-decisions-now-vs-later.md) §22.5 updated now that `main` exists.

## Revision 4 (2026-10-14): ChatGPT review

Applied Noah's instructions based on ChatGPT's independent review. The central architecture is unchanged: Rust simulation core → Unity 6 / URP → Cesium where useful → iPhone.

1. **Visual providers.** Google Photorealistic 3D Tiles are formalized as an optional visual backdrop behind a provider abstraction (open data, Google, NASA, future sources), never part of simulation state. New: [23-visual-providers.md](23-visual-providers.md); ADR 0008.
2. **Stage progression.** Technical Prototype (MVP-0) → Vertical Slice (about one Bay Ridge block, 10–30 people, 3–5 buildings, one intervention, no visual polish) → Neighbourhood MVP (6–12 blocks, preserved) → City → Planet → Solar System. Rewritten: [19-roadmap.md](19-roadmap.md), [20-mvp.md](20-mvp.md); ADR 0009.
3. **Determinism.** Strict cross-platform bit-identical determinism is now desirable, not a blocker; tiered policy. New: [25-determinism-policy.md](25-determinism-policy.md); ADR 0010.
4. **World events and history.** A first-class event system. New: [24-world-events-and-causality.md](24-world-events-and-causality.md); ADR 0011.
5. **Causal provenance.** A practical model of direct links, bounded provenance tags, aggregate attribution, and optional counterfactuals, with confidence classes. In [24](24-world-events-and-causality.md) and [13](13-god-interventions.md) §14.8; ADR 0012.
6. **Consistency pass.** Removed leftovers of the fictional, stylized, custom-Metal design and the revision banners; rewrote affected sections so that each states the current design directly.

Files rewritten in full: README, 01, 02, 03, 04, 06, 07, 08, 11, 14, 15, 17, 18, 19, 20, 21, 22, glossary.
Files edited in place: 05 (event roll-up across levels), 09 (synthetic population calibration, event-based memory, provenance), 10 (reproducibility tiers, real data, ephemerides), 12 (event refinement during materialization, provenance), 13 (causal provenance section), 16 (event system as LLM context; zero cost).

## Revision 3 (2026-10-14)

Bay Ridge chosen as the first area; the stack (Rust core + Unity + Cesium) accepted; zero-cost policy. ADRs 0005–0007.

## Revision 2 (2026-10-14)

Noah's decisions: real Earth, realistic visuals, single-player, iPhone 15 Pro minimum. ADRs 0001–0004. Stack recommendation moved from a custom Swift/Metal front end to Unity + Cesium.

## Revision 1 (2026-10-14)

Initial proposal covering the 22 points of the brief.


## TP-008 implementation (2026-10-16)

Implements accepted ADR 0019's typed bounded command queue, boundary journal, meaningful event subset and same-build/platform headless replay. Adds `ws-events` at layer 1 and `ws-cli` as a layer-5 host adapter, without runtime external dependencies. Domain plans expose actual arrivals/LOD changes one way for atomic publication. No architecture guardrail, Unity ABI, save format, multithreaded scheduler, retention or provenance propagation is changed. The conditional bridge adaptation awaits TP-003's final merged contract; see the Rust workspace README and tape codec.
