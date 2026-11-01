# ws-math

## Responsibility

The coordinate and reference-frame foundation ([07-coordinates-and-time.md](../../../docs/architecture/07-coordinates-and-time.md) §8.1–8.5, ADR 0015): the Earth-surface part of the frame hierarchy.

| Module | Provides |
|---|---|
| `wgs84` | WGS84 constants; `Geodetic` (latitude and longitude in radians, ellipsoidal height in metres); geodetic ↔ ECEF |
| `frame` | Frame-tagged `Position<F>` and `Vector<F>` for the frames `Ecef` and `Enu`; adding values from different frames does not compile |
| `enu` | `EnuFrame`: a local East-North-Up frame at a geodetic origin, with ECEF ↔ ENU transforms for positions and vectors |
| `cell` | `CellId(u64)`: S2-compatible cube-sphere cells, Hilbert-ordered; from geodetic or ECEF, level, parent, children, containment, edge neighbours, centre, cell ENU frame |
| `surface` | `SurfacePos { cell, offset: [f32; 3] }` in a sector cell's ENU frame (`SECTOR_LEVEL` = 13); to and from ECEF; `CellFrame` for allocation-free bulk conversion; deterministic re-expression into the containing cell |
| `solar` | ADR0030 NOAA/Meeus geometric solar rule1 from integer civil day/label components; libm unit direction, azimuth and elevation, with no refraction |
| `reference` | `BAY_RIDGE`, the reference point for tests |
| `error` | `CoordError`, returned for every invalid input |

Conventions: SI units (metres, radians), right-handed frames, ENU local frames (x east, y north, z up), ellipsoidal heights. Cell IDs have the same `u64` value as Google S2 cell IDs for the same direction.

Out of scope (later issues): inertial and barycentric frames, precision Earth rotation (UT1/polar motion), TT/TDB ephemerides, geoid heights, camera-relative rendering and the Unity mapping, physics frames.

## Owned state

None. The crate is pure functions and value types; it holds no simulation state.

## Emitted event types

None.

## Invariants

- Every `Geodetic`, `CellId`, and `SurfacePos` value is valid: constructors reject NaN, infinities, out-of-range latitude or longitude, invalid cell bits, and non-sector cells with `CoordError`.
- Positions and vectors are tagged with their frame; conversions go only through named functions.
- **Tier 1** ([25-determinism-policy.md](../../../docs/architecture/25-determinism-policy.md)): cell IDs and cell membership are bit-identical on every platform. All trigonometry uses the pure-Rust `libm` crate, never the platform math library, and the rest uses only correctly rounded IEEE 754 operations (no fused multiply-add). Golden values in [`tests/golden/cell_ids.txt`](tests/golden/cell_ids.txt) are checked on Linux x86-64 and macOS arm64 CI. Updating them needs Product Owner approval ([testing-policy.md](../../../docs/process/testing-policy.md) §2).
- The continuous conversions are Tier 3 by policy (tolerances, not hashes), although they use the same deterministic arithmetic.
- `SurfacePos::reexpress` returns the input unchanged while the offset is inside its cell, and otherwise expresses it in the sector cell that contains the 64-bit ECEF point, preserving the position to within 0.1 mm.
- Every position produced by `SurfacePos::reexpress` or `SurfacePos::from_ecef` lies in its own cell (`occupied_cell() == cell()`), including within a fraction of a millimetre of a cell edge ([ADR 0016](../../../docs/adr/0016-entity-identity-storage-and-randomness.md) §4). Where rounding the new offset to `f32` would put it back across the edge, the nearest `f32` neighbour (3 × 3, then 5 × 5 grid steps) that lies inside the target cell is chosen. Re-expression is therefore idempotent, and a position that does not move never changes cell. Before TP-006, about 1% of `f32` positions within 200 µm of an edge re-expressed outside their cell, and some alternated between two cells forever; `tests/cell_edges.rs` shows that failure in its own samples and that the rule prevents it. Results outside that band are bit-for-bit what they were before.
- No `unsafe`, no allocation in the conversion paths, no wall clock (the benchmark alone reads it, to time itself).

## Accuracy

| Path | Tested bound |
|---|---|
| Geodetic → ECEF → Geodetic, heights −100 km to +1000 km, whole globe | < 1 mm (property test) |
| Geodetic → ECEF against PROJ 9.8.1 for benchmark points | < 2 µm |
| ECEF → `SurfacePos` → ECEF, and `SurfacePos` → ECEF → `SurfacePos`, heights −500 m to +1000 m | < 0.1 mm (property test) |
| `SurfacePos::reexpress` within ±200 µm of a cell edge (ADR 0016 rule) | < 0.1 mm (edge sweeps and property test; largest seen about 60 µm) |

32-bit offsets lose precision with height above the ellipsoid (spacing about 0.5 mm at 8 km), so objects far above the surface belong in 64-bit ECEF.

## Performance

`cargo bench -p ws-math` times 10,000 conversions (best of 50 passes). On the Linux CI-class machine used for TP-005 (Intel Xeon, 2.1 GHz):

| Path | 10,000 conversions | Each |
|---|---|---|
| `SurfacePos` → ECEF with a prebuilt `CellFrame` | 37 µs | 3.7 ns |
| `SurfacePos` → ECEF, building the frame each call | 1.1 ms | 111 ns |
| ECEF → `SurfacePos` with a prebuilt `CellFrame` | 104 µs | 10 ns |
| ECEF → `SurfacePos` with cell lookup and frame | 5.9 ms | 593 ns |

Snapshot building should build one `CellFrame` per visible cell and reuse it. iPhone measurements belong to TP-011.

## Dependencies

- `libm` 0.2 (MIT): portable transcendental functions for Tier 1 paths.
- `proptest` 1 (MIT OR Apache-2.0), dev-only, default features off.

### Cached movement re-expression (#31, 2026-10-16)

`SurfacePos::reexpress_with(&CellFrame)` reuses the source frame and keeps the full Tier 1 cell lookup and ADR 0016 edge-settling rule. A mismatched frame is an error; it never substitutes a geometric approximation. Global fixed-seed tests compare exact bits with `reexpress`, and ±200 µm edge sweeps check the same invariant and inside decisions.

On an Apple M1 Pro Mac, Rust 1.97.0 release, the conversion benchmark's original and cached paths over identical 10,000 inputs took **4,986.2 → 3,931.2 µs** (498.6 → 393.1 ns per call; best of 50 passes). Reusing the frame also removes repeated source-frame construction for movement's goal projection. The full `world_step/5000` improves from 3.3330 to 2.2909 ms; see the [complete verification report](../../../docs/verification/issue-31/README.md) for LOD comparisons and correctness evidence. These are desktop measurements, not iPhone performance claims.

### Inside-cell fast path (#48, 2026-10-17)

`CellFrame::with_inside_test(cell)` also stores the cell's four edge planes (`CellId::edge_normals`), and `CellFrame::certainly_contains(p)` uses them to prove, without trigonometry, that the full Tier 1 lookup would put `p` in that cell. `SurfacePos::reexpress_with` asks it first and returns the position unchanged, bit for bit, when it is proven; anything else takes the unchanged lookup and ADR 0016 settling. The test checks `q = (x, y, z / (1 − e²))`, which is parallel to the geodetic normal on the ellipsoid, against each plane with a margin of `|h| e'² / (A − 200 km)` radians for height (bounded from `p`'s radius) plus 1e-9 rad for rounding, about 7 mm on the ground at the surface. Points more than 100 km from the ellipsoid are never proven. Plain `CellFrame::new` frames carry no planes and never prove anything: the planes add about 30 ns and 104 bytes per frame, which measurably slowed LOD promotion when every frame carried them.

`tests/inside_fast_path.rs` sweeps rays from cell centres in eight named regions (Bay Ridge, equator, 60° and 80° N, near the south pole, a cube vertex, both sides of a face edge) and 256 random ones, at heights from −2 km to 150 km and distances from 1 µm to 100 m on both sides of the edge: every proven point is inside by the full lookup, and every point 50 m inside the edge at walking heights is proven. Removing the height term makes it fail. The ±200 µm edge sweeps in `tests/cell_edges.rs` also check that no point that close to an edge is proven, and the global property test compares fast and full re-expression bit for bit.

Same Linux Intel Xeon 2.1 GHz machine as the #31 review, Rust 1.97.0 release, `cargo bench -p ws-math --bench conversions`: `reexpress_with` on 10,000 positions inside one cell takes **428 → 32 ns per call**; `CellFrame::new` 117 ns, `CellFrame::with_inside_test` 146 ns.

## VS-007 solar rule1

`solar::position` validates UTC2000–2099 day/nominal-label bounds and finite latitude/longitude. It uses nominal UTC approximating rotational time, with integer normalization of leap-second phase before floating arithmetic; second60 and the following midnight share model phase, while ws-time retains their distinct instants. Every transcendental uses pinned libm; evaluation order/constants have new exact Tier1 bit goldens in `tests/golden/solar_v1.txt`, independent of existing coordinate goldens. Three independently executed published NOAA geometric samples at the registered block must match within0.1° (circular azimuth). No mutable sky state, RNG, wall clock, network or new dependency. See [verification](../../../docs/verification/issue-71/README.md).
