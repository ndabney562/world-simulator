# ADR 0031: real-slice footprint transport and presentation camera

- Status: Proposed
- Date: 2026-10-20
- Decided by: pending independent review and Noah's named technical delegation on #77; proposed by Codex
- Issue: #72 (VS-008 design phase)
- Supersedes: none; extends ADRs0018/0022/0029/0030 with a separate real-slice transport

## Context

[#72](https://github.com/ndabney562/world-simulator/issues/72) requires the four simulated buildings at their VS-002 positions, real footprint extrusions, surrounding open-data massing, residents and an orbit/pan/pinch camera from street scale to a few hundred metres. The existing bridge renders a five-building toy world. The real slice already owns structural identities, schedules, fine navigation and canonical camera/LOD operations in Rust, but has no native transport. ADR0030 supplies calendar/sun output and consumes ABI4; its runtime PR must merge before this implementation integrates it.

The registered block6049 extract contains58 MultiPolygon features, each currently one polygon with one exterior ring,449 non-closing ring vertices in total and no holes. The four featured rows have actual simulation EntityIds; the other54 source rows are visual context. Navigation coordinates are approximate-plane integer millimetres, not geodetic ENU. Extending the toy box records would obscure these distinctions. This docs-only decision precedes separate implementation and does not complete device acceptance.

## Options considered

1. Construct footprints and resident positions in Unity from the GeoJSON. Rejected: duplicates authoritative data/adapters and bypasses native snapshot coherence.
2. Replace the toy handle and its command journal with an implicit backend switch. Rejected: mixes distinct operation/history models and risks old save/command behavior.
3. Add an explicit real-slice handle, bounded static mesh and dynamic bulk view, using the same native ABI preflight and ownership rules. Chosen: preserves toy behavior while exposing the existing slice honestly.

## Decision

### Ownership, source and geometry

`ws-world` owns immutable footprint extrusion and the fixed render frame. `ws-life` continues owning navigation and LOD; its existing projection gains checked explicit adapters. `ws-sim` composes immutable slice render/summary/calendar views and applies typed slice operations. `ws-ffi` marshals via its existing `ws-sim` dependency. Unity caches meshes and renders snapshots. No new dependency, source, provider, package, terrain or crate is introduced.

Use only the registered compiled extract and its existing dataset version. Extrude all58 footprints with their source above-ground `height_mm`, not centroid boxes. Preserve source coordinates, closures, identities, structural baseline and all old goldens. Keep source BIN separate from EntityId: static rows are ascending BIN, carry BIN plus the featured EntityId or zero for context, and never allocate simulation IDs/population/history for surrounding context.

Render-geometry version1 supports this pinned dataset's simple, single, hole-free polygons. Validate that limitation explicitly, including closure, nonzero area, duplicate/collinear treatment, self-intersection and all count/index limits. Unsupported changed topology fails construction rather than filling holes or silently dropping parts. A future source with holes/multiple parts needs a reviewed geometry-version extension. Triangulate concave roofs in Rust with deterministic ear clipping using exact i128 picodegree orientation/segment predicates and stable source-order tie breaking; remove only exactly collinear vertices from the temporary roof triangulation. Retain every original wall boundary segment. Normalize output winding only in derived geometry, never rewrite source rings. No bottom face is required. Emit outward unit normals and consistent roof/wall winding after the Unity axis conversion; test concave area coverage, no exterior-crossing triangles, source coverage and every index.

Pin the real-slice frame at ADR0030's complete-extract envelope midpoint, longitude−74.030160593242° and latitude40.622079256973°, WGS84 ellipsoidal height0. For each footprint vertex, convert geodetic latitude/longitude with explicitly supplied ellipsoidal height0 or source roof height through ECEF into that frame, then narrow finite values to f32 metres. The extract's NAVD88 ground elevations are retained source metadata and are never passed as ellipsoidal height. This is a flat massing approximation, not surveyed terrain. Publish the frame and full derived bounds. Unity performs only ENU→(east,up,north), including normals and triangle-handedness handling; it never subtracts absolute longitude or treats navigation millimetres as ENU.

Navigation→render uses the existing navigation origin and exact projection constants: approximate east millimetres correspond to longitude delta via84,500,000mm/degree; north via111,200,000mm/degree. Invert that documented approximation into geodetic coordinates, then use ECEF→the fixed render frame. For camera input use the reverse frame→ECEF→geodetic transform, then the existing checked integer projection and bounded `CameraInterest`; radius rounds upward to integer millimetres. Version these adapters with render version1. Retain the current navigation graph/fingerprint, path lengths and positions. Derived coordinate conversion is Tier3; integer source geometry, identity, canonical camera words, counts and topology predicates remain Tier1. Test source coordinates against VS-002, round trips within2mm for navigation points and finite/bounded street-to-overview camera cases. Calendar's dedicated direction conversion retains ADR0030's exact-bit guarantee in this same frame.

### ABI5 and explicit real-slice handle

Consume ABI5 if still unused at implementation start, following merged ABI4. ADR0026's provider reservation remains conditional and must use a later unused version before its own implementation. Preflight refuses versions1–4 before new symbols are used. Advance version fields in the existing config/control/toy-render/calendar structs together, while preserving their layouts, toy command envelope2, opcodes, save formats and semantics. Keep all old fixtures and consumers as refusal/reference coverage; add ABI5 fixtures/consumer. Technical acceptance covers only these obsolete success-version expectations, new fixture selections and C-consumer entrypoint migration, with all earlier behavioral/error/pixel/allocation checks retained.

Add opaque `WsSliceHandle`, separate from `WsHandle`. Never cast between them. Create/destroy/error behavior follows the existing ownership, TLS creation diagnostics, caught panic and poisoned-handle rules. A handle is exclusively serialized; no retained caller pointers, callbacks or per-resident calls. Existing `ws_*` toy functions remain toy-only. New functions are:

- `ws_slice_create(const WsSliceConfig*)` / `ws_slice_destroy(WsSliceHandle*)`.
- `ws_slice_apply(WsSliceHandle*, const WsSliceOperation*)`: one coarse structured operation, not a per-entity transport. Returns the existing status vocabulary. Mutating operations preserve the underlying committed-prefix/failure checkpoint semantics; queries below report the actual committed boundary after failure. Invalid wire input is refused before the domain operation begins.
- `ws_slice_fill_geometry` and `ws_slice_fill_snapshot`: `(handle, void* buffer, size_t capacity, size_t* written)` following existing probe/fill rules. Short output is untouched and reports required length; no query mutates/flushes/materializes, changes history/hash, or consumes RNG. Other failures leave output untouched. Geometry is immutable per handle; dynamic output is one complete committed view.
- `ws_slice_get_calendar_sky(WsSliceHandle*, WsCalendarSky*)`: existing160-byte layout, now ABI5, pure slice calendar/hash/tick/scale with sun rotated into the published fixed frame.
- `ws_slice_last_error` with the existing bounded diagnostic-copy convention.

`WsSliceConfig` is32 bytes aligned8: ABI u32 at0, size32 u32 at4, seed u64 at8, start_us i64 at16, reserved-zero u64 at24. Population is the existing seed-generated slice cohort, not a toy residents-per-building parameter. Creation uses ADR0030's calendar-aligned model-Monday helper and rejects unsupported start/grid inputs before publication. Store the resulting schedule epoch/configuration unchanged. No save codec ships here; #75 owns whole-slice persistence and version pinning.

`WsSliceOperation` is64 bytes aligned8: ABI u32 at0, size64 u32 at4, tag u32 at8, reserved-zero u32 at12, six64-bit words at16–63. Read words using the defined tag types, never alias unaligned bytes. Tags1/2/3 carry unsigned explicit step count, scale0/1/10/100, or elapsed host microseconds respectively in word0; remaining words zero. Explicit step count is bounded by the existing native maximum per call. Tag4 carries four finite f64 values, target east/north/up metres and interest radius metres, in words0–3; words4/5 zero. Require nonnegative radius and checked core coordinate/bounds conversion before applying. No manual-detail or intervention opcode is introduced in this issue. Unknown tags, reserved bits, malformed values, size/version errors are refused without domain mutation or partial input acceptance.

Each accepted operation delegates to existing `SliceSimulation::apply` and records the actual normalized integer `Operation`/outcome. Camera float input is converted once; replay reuses recorded integer camera words and never recomputes them from floats. Existing invalid-domain operation/outcome semantics and environmental stop checkpoints remain intact. This wire input is not a new save/replay format. Host elapsed delegates to core cap/drop/remainder logic; do not layer a second host step accumulator over it. Scale applies at a serialized boundary, including pause without a physical tick.

### Static geometry buffer

All buffers are explicit little-endian bytes with checked arithmetic, no raw struct padding. Geometry header96 bytes:

| Offset | Type | Meaning |
|---:|---|---|
|0/4 |u32/u32 |ABI5 / header96 |
|8 |u64 |dataset version |
|16/20/24/28 |4×u32 |render version1 / building count / vertex count / index count |
|32/36/40/44 |4×u32 |building stride48 / vertex stride24 / index stride4 / zero |
|48/56/64 |3×f64 |origin latitude degrees / longitude degrees / ellipsoidal height metres |
|72–92 |6×f32 |minimum east/north/up, maximum east/north/up |

Follow with building rows, vertex rows then u32 triangle indices. Building row48: BIN u64 at0, EntityId-or-zero u64 at8, first_vertex/count u32 at16/20, first_index/count u32 at24/28, source height metres f32 at32, flags u32 at36 (bit0 featured), source centroid east/north metres f32 at40/44. Vertex row24 is position east/north/up f32 then unit normal east/north/up f32. Indices address the complete vertex array; every building owns disjoint bounded vertex/index ranges, with index count divisible by3. Context/featured flags must agree with zero/nonzero identity; duplicate BINs/featured IDs are invalid.

Bounds cover every emitted vertex, not only centroids. Cap buildings at256, vertices at32,768 and indices at131,072; reject construction overflow or unsupported geometry before publishing a handle. These caps exceed the pinned source and bound persistent host allocation; decoders reject excessive counts before allocating or indexing. Require exact total byte length, versions, strides, valid nonoverlapping coverage, bounded finite geometry, unit normals and valid indices. Geometry is fetched/uploaded once after creation/recreation, never rebuilt per frame. Fixture tests cover all fields, corrupt/truncated lengths, indices, flags and overflow.

### Dynamic snapshot buffer and coherence

Header128 bytes:

| Offset | Type | Meaning |
|---:|---|---|
|0/4 |u32/u32 |ABI5 / header128 |
|8/16/24/32 |u64/i64/u64/u64 |tick / SimTime / canonical hash / dataset version |
|40/44 |u32/u32 |render version1 / committed scale |
|48/52/56 |3×u32 |population / resident row count / building summary count |
|60/64/68 |3×u32 |resident stride48 / summary stride48 / zero |
|72/76/80 |3×u32 |local stationary / in transit / off-map stationary counts |
|84/88 |u32/u32 |L1 count / L2 count |
|92–124 |9×u32 |zero |

Then all residents in ascending EntityId order, then the four building summaries in ascending EntityId order. Preserve each real resident's identity even when no fine geometry exists. Resident row48: EntityId u64 at0, governing building EntityId u64 at8; local east/north/up metres f32 at16/20/24 and heading radians f32 at28; detail u32 at32 (1/2), activity u32 at36 (existing schedule tags0–5), location u32 at40 (0 no fine position,1 local fine,2 off-map transit,3 stationary off-map), zero u32 at44. Nonlocal/no-fine position and heading fields are zero. L2 has location0; L1 uses the actual fine union. A local capsule's foot is the transformed navigation point, with presentation radius/height added by Unity. Derive local walking heading from the core route tangent in the fixed render basis, zero when stationary. Never invent an off-map street position or materialize a coarse resident to draw it.

Summary row48: building EntityId u64 at0; mode u32 at8 (existing L1/L2/L3 tag), occupancy u32 at12, governed u32 at16, six activity counts u32 at20–40, zero u32 at44. These are exact existing controller counts; enforce occupancy=sum(activities), population=local_stationary+in_transit+off_map_stationary=L1+L2, complete unique resident partition and exactly the four featured IDs. Rendering a coarse aggregate uses these counts, not duplicated resident capsules or guessed building occupants. Do not turn source context into fake L3 domain nodes. Cap population/rows at the existing maximum30 and summaries at4; reject version/size/length/finite/tag/count/identity incoherence before replacing a managed frame.

Hold the same managed slice gate across dynamic fill and calendar query, then require equal tick, time, hash, scale and dataset before publishing the reusable frame. Static geometry belongs to that handle/dataset/render version. Raw C callers must serialize the pair themselves. Failed decode/query publishes no partial new frame. Reuse persistent bounded byte/value/matrix buffers and geometry caches; warmed fill/decode/render must introduce no per-frame managed allocation. Rebuild only on explicit world replacement and dispose native/GPU resources exactly once. Any temporary Rust snapshot allocation is bounded and included in measured fill cost; do not claim allocation-free Rust queries without evidence.

### Camera, rendering and verification

Keep the existing toy demo/measurement scene available for regression. Add an explicit real-slice demo/host using the new handle and fixed configured civil start. Default overview fits the complete geometry bounds with aspect-correct perspective and margin, including all four featured buildings; viewport changes recompute fit only when in default framing, preserving an intentionally moved view. Provide mouse/touch orbit, ground-plane pan and pinch/wheel zoom, bounded finite target/elevation/distance and near/far planes. Street view must avoid near-plane clipping of people; overview supports a few hundred metres. Tests exercise narrow/wide viewport framing of every featured footprint, resize, orbit, pan, pinch/wheel clamps and repeated input, without depending on a particular screenshot crop.

Camera motion itself is presentation. Submit one coarse interest target/radius from the visible ground footprint at a bounded cadence, coalescing unchanged inputs and the latest pending move. A zero radius explicitly selects overview. Core conversion and ADR0029 continue deciding canonical node membership with200-ms evaluation,1-second dwell,20-metre hysteresis and the >10× L2 cap. Pause can accept camera input without advancing a tick; membership changes at the existing next physical policy boundary. Never set per-resident detail from Unity or change thresholds to improve a picture. Test rendered membership/aggregate conservation against native summaries and replay the recorded normalized camera/scale script.

Upload cached footprint meshes with a shared flat-colour URP material and bounded static draws; keep resident capsules and aggregate proxies instanced, with stable identity mapping for later #73 picking. Reuse source context rather than enabling a provider. Feed the directional light and date/speed HUD only from the coherent ADR0030 sky. No new imagery, terrain, facade textures, photorealism or lighting polish. Instance-level presentation culling does not alter simulation membership. At street scale use a known committed instant with genuine local L1 residents for visibility evidence; label off-map/coarse counts honestly.

Required implementation evidence:

- New exact layouts/fixtures and C/Rust/C# consumer/error/refusal coverage; fresh generated header and iOS Rust archive; old command/save/replay and render behavioral checks still pass.
- Source-versus-extrusion assertions for all58 rows, four featured identities/VS-002 positions, roof-area/concavity/winding/normals/index bounds, explicit NAVD88 exclusion, adapter round trips and bounded no-output-on-error tests.
- Immutable query/hash/history/RNG independence, coherent sky/snapshot pairing, scale/pause/host-prefix behavior, camera normalization replay and count conservation across all modes/scales. No old golden/tolerance changes.
- Actual pinned Unity6000.3.26f1 Editor with graphics: decoder/layout/play camera, all-featured-in-view pixels, street resident pixels, pause/speed/light and warmed allocation checks. Record source SHA, seed/population, backend, geometry counts and raw CSV with fill/decode/render/frame/worker times. Editor evidence is explicitly desktop evidence.
- Reuse the TP-011 device build pipeline. Actual iPhone15Pro default overview and street screenshots plus CSV must demonstrate60fps (frame≤16.7ms) and report memory against the≈220MB baseline. Do not substitute static compilation or Editor FPS for these criteria. Device signing/provisioning/runs retain their separate approval; if unavailable, leave #72 open with device criteria unchecked and a concrete reproducible handoff. No stage-exit claim.

Independent review and all hosted checks must pass on the final published design head before delegated acceptance/merge; runtime follows in a separate PR after ADR0030 runtime integration. Runtime may land as verified engineering progress with outstanding device criteria explicitly recorded, without closing #72 or marking tracker acceptance complete. This does not waive any acceptance criterion or budget.

## Consequences

The real block crosses one bounded static and one coherent dynamic native boundary, with core-owned geometry/positions and source context kept separate from simulation identities. A separate handle avoids mixing slice operations with the old toy journal/persistence and retains its regressions. The fixed frame is sufficient for this block; larger areas/floating origins, general polygons with holes, persistent slice transport, native picking/inspection (#73), interventions (#74), persistence (#75) and device soak/stage exit (#76) remain separate reviewed work.
