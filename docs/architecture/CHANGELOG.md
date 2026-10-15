# Architecture Proposal Changelog

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
