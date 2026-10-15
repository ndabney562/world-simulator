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
| `reference` | `BAY_RIDGE`, the reference point for tests and the toy world |
| `error` | `CoordError`, returned for every invalid input |

Conventions: SI units (metres, radians), right-handed frames, ENU local frames (x east, y north, z up), ellipsoidal heights. Cell IDs have the same `u64` value as Google S2 cell IDs for the same direction.

Out of scope (later issues): inertial and barycentric frames, Earth rotation, ephemerides, geoid heights, camera-relative rendering and the Unity mapping, physics frames.

## Owned state

None. The crate is pure functions and value types; it holds no simulation state.

## Emitted event types

None.

## Invariants

- Every `Geodetic`, `CellId`, and `SurfacePos` value is valid: constructors reject NaN, infinities, out-of-range latitude or longitude, invalid cell bits, and non-sector cells with `CoordError`.
- Positions and vectors are tagged with their frame; conversions go only through named functions.
- **Tier 1** ([25-determinism-policy.md](../../../docs/architecture/25-determinism-policy.md)): cell IDs and cell membership are bit-identical on every platform. All trigonometry uses the pure-Rust `libm` crate, never the platform math library, and the rest uses only correctly rounded IEEE 754 operations (no fused multiply-add). Golden values in [`tests/golden/cell_ids.txt`](tests/golden/cell_ids.txt) are checked on Linux x86-64 and macOS arm64 CI. Updating them needs Product Owner approval ([testing-policy.md](../../../docs/process/testing-policy.md) §2).
- The continuous conversions are Tier 3 by policy (tolerances, not hashes), although they use the same deterministic arithmetic.
- `SurfacePos::reexpress` returns the input unchanged while the offset is inside its cell, and otherwise moves it into the sector cell that contains it, preserving the position to within 0.1 mm.
- No `unsafe`, no allocation in the conversion paths, no wall clock (the benchmark alone reads it, to time itself).

## Accuracy

| Path | Tested bound |
|---|---|
| Geodetic → ECEF → Geodetic, heights −100 km to +1000 km, whole globe | < 1 mm (property test) |
| Geodetic → ECEF against PROJ 9.8.1 for benchmark points | < 2 µm |
| ECEF → `SurfacePos` → ECEF, and `SurfacePos` → ECEF → `SurfacePos`, heights −500 m to +1000 m | < 0.1 mm (property test) |

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
