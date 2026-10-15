# ADR 0015: Coordinate representation (D-4) and default sector level

- Status: Proposed
- Date: 2026-10-15
- Decided by: Noah (Product Owner), on Claude's proposal for TP-005; reviewed by ChatGPT
- Issue: #6 (TP-005)
- Supersedes: none (fixes in detail D-4 of [21-decisions-now-vs-later.md](../architecture/21-decisions-now-vs-later.md), and proposes the S-4 default)

## Context
[07-coordinates-and-time.md](../architecture/07-coordinates-and-time.md) §8.1–8.5 sets the frame hierarchy: ECEF on WGS84 as the body-fixed frame, S2-style cells with local ENU frames, `(sector cell ID, 32-bit local ENU offset)` for surface entities, ellipsoidal heights, and camera-relative GPU data. TP-005 implements the Earth-surface part of it in `ws-math`, which fixes the types, units, and cell-ID scheme that every later system, save, and test will depend on. Cell IDs and cell membership are Tier 1 ([25-determinism-policy.md](../architecture/25-determinism-policy.md)): they must be bit-identical on Linux x86-64, macOS arm64, and iPhone, which floating-point code does not give for free, because platform `sin`, `cos`, and `atan2` differ in the last bit between C libraries.

## Options considered
1. **Use the `s2` crate** (Apache-2.0) for cells. Benefits: mature port of Google S2 with a large API. Costs: it computes with the platform math library, so its results are not Tier 1; it is large and brings dependencies that are hard to remove; we need only a small part of it.
2. **Our own S2-compatible implementation with portable math** (chosen). Benefits: same IDs as S2, so external tools and data can be cross-checked; full control of every floating-point operation on the Tier 1 path; one small permissive dependency. Costs: we maintain a few hundred lines and must test them against S2.
3. **H3 or another grid.** Rejected: Architecture V1 chose an S2-style hierarchy, and H3's hexagons do not nest exactly, which complicates the world tree.
4. **Universal 64-bit fixed-point positions** (07 §8.8). Kept as an option for specific Tier 1 needs; not the primary representation.

## Decision

**Units and conventions.** SI units: metres and radians. All frames are right-handed. Local frames are East-North-Up (x east, y north, z up along the ellipsoid normal). Heights are ellipsoidal heights above WGS84 (a = 6 378 137 m, 1/f = 298.257 223 563); geoid conversion belongs to the data pipeline.

**Types** (`core/crates/ws-math`):
- `Geodetic { lat, lon, h }`: radians, radians, metres; validated (finite, latitude in [−π/2, π/2], longitude in [−π, π]).
- `Position<F>` and `Vector<F>`: 64-bit components tagged with a frame marker type, `Ecef` or `Enu`. The algebra is affine (position − position = vector; position ± vector = position), so adding a position to a vector of another frame, or two positions, does not compile. Conversions go only through named functions. Which ENU frame a value belongs to is runtime data, so surface positions carry their cell.
- `EnuFrame`: a 64-bit origin and rotation at a geodetic origin.
- `CellId(u64)`: bit-for-bit the Google S2 `CellID` layout: 3 face bits, 2 bits per level along S2's Hilbert curve, a trailing marker bit; S2's face orientation and quadratic projection; `floor` when converting to leaf coordinates. A latitude and longitude is placed in a cell as a direction on the unit sphere (S2's `LatLng` convention), so membership ignores height.
- `SurfacePos { cell: CellId, offset: [f32; 3] }`: the offset is in metres in the cell's ENU frame, whose origin is the WGS84 ellipsoid point (height 0) under the cell's centre.
- Invalid input returns a `CoordError`; nothing silently produces NaN.

**Sector level.** Sector cells are **level 13** (0.6 to 1.2 km across) by default, as `SECTOR_LEVEL`. S-4 stays open until materialization costs are measured (TP-006 onward); changing the level is a one-constant change before the first save, and a save-format version change after.

**Tier 1 exactness.**
- Every transcendental function in `ws-math` (`sin`, `cos`, `atan2`) comes from the pure-Rust **`libm`** crate (MIT, a port of musl's libm), never from the platform library.
- Everything else on the path is IEEE 754 operations that are correctly rounded on every Rust target (`+ − × ÷ √`, comparisons, `floor`). Rust never contracts `a * b + c` into a fused multiply-add on its own, and the crate never calls `mul_add` or `powi`.
- Once a point has a cell ID, level, parent, children, containment, ordering, and Hilbert decoding are integer operations. Edge neighbours across a face boundary use S2's `FromFaceIJWrap` re-projection, made of the same basic operations.
- ECEF → geodetic uses Bowring's method with a fixed iteration count, so the operation sequence is identical for every input. Positions within 1000 km of the Earth's centre are rejected.
- A surface position is re-expressed by converting to ECEF, finding the containing sector cell, and, only if it differs, expressing the ECEF point in the new cell's frame. The whole path is deterministic, so the resulting cell is the same everywhere.
- Golden tests with committed cell IDs run on Linux x86-64 and macOS arm64 CI. The continuous conversions stay Tier 3 by policy (tolerances in tests), although they use the same portable arithmetic.

## Consequences
- Cell IDs can be compared directly with S2 tooling. During TP-005, 3,013 of 3,014 points matched s2sphere (a Python port of S2) exactly. The exception lies exactly on a cube vertex, where the platform `sin` and libm's `sin` differ by one ulp and the face changes. That case shows why the Tier 1 path cannot use platform math.
- `libm` is slower than platform math. That is acceptable at this scale: converting a `SurfacePos` to ECEF with a prebuilt cell frame takes about 4 ns, and a full ECEF → `SurfacePos` with cell lookup about 0.6 µs, on a Linux CI-class machine. Snapshot building should build one frame per visible cell.
- 32-bit offsets lose precision with height above the ellipsoid (spacing about 0.5 mm at 8 km). Aircraft, spacecraft, and anything far above the surface stay in 64-bit ECEF. If tall structures need better vertical precision, a per-cell reference height can be added later without changing cell IDs.
- A point within a fraction of a millimetre of a cell boundary can land on either side after `f32` rounding, so repeated re-expression could alternate between two cells. This is deterministic, not a Tier 1 violation. Hysteresis, if needed, is decided with the movement systems (TP-006).
- Inertial and barycentric frames, Earth rotation, geoid heights, camera-relative rendering, and physics frames are not covered and get their own decisions later.
- Revisit if S-4 measurements choose another sector level, if a Tier 1 path needs a function `libm` does not provide, or if `libm` changes its results between versions (`Cargo.lock` pins it, and the golden tests would catch it).
