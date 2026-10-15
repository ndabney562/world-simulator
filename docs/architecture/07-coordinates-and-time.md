# 8. Coordinate System (and Time Representation)

Covers point 8: a coordinate system that supports metre-scale interactions and astronomical distances. Time is included because it has the same multi-scale problem. The world is real Earth in the real solar system (ADR 0001).

## 8.1 The numbers that drive the design

Spacing between adjacent representable values ("ulp") at various magnitudes:

| Magnitude | Example | 32-bit float spacing | 64-bit float spacing |
|---|---|---|---|
| 2 km | Within a city cell | ~0.12 mm | negligible |
| 10 km | Across a borough | ~1 mm | negligible |
| 6.4 × 10⁶ m | Earth's radius | **0.5 m** | ~1 nm |
| 3.8 × 10⁸ m | Earth–Moon distance | ~32 m | ~60 nm |
| 1.5 × 10¹¹ m | 1 AU | **~16 km** | ~0.03 mm |
| 4.5 × 10¹² m | Neptune's orbit | ~500 km | **~1 mm** |

Conclusions:
1. 32-bit floats are only safe within a few kilometres of their origin.
2. 64-bit floats are precise enough within Earth's frame and still at millimetre level for a Sun-centred frame out to Neptune. A frame hierarchy is still needed for cheap, compact storage of millions of surface entities, for GPU rendering, for rotating frames, and for keeping physics well-conditioned.
3. The GPU only ever receives camera-relative 32-bit values.

## 8.2 Units and conventions

- SI units internally: metres, seconds, kilograms, radians. Display units (feet, miles) are a UI concern.
- Right-handed coordinate systems. Local tangent frames are East-North-Up (ENU). (Unity is left-handed and Y-up; the conversion happens once, in the front end's snapshot decoding.)
- Every position and direction type is **tagged with its frame** at the type level in the core, so adding positions from different frames is a compile error. Conversions go through explicit, tested functions.

## 8.3 The frame hierarchy

```
Solar-system barycentric frame (ICRF-aligned, non-rotating; 64-bit)
 └─ Body-centred inertial frame (e.g. Earth-centred inertial; 64-bit)
     └─ Body-fixed frame (Earth: ECEF on the WGS84 ellipsoid; 64-bit)
         └─ Surface cell frame (S2-style cell; local ENU tangent frame; 32-bit offsets)
             └─ Object frame (building, vehicle, character; 32-bit)
```

**Barycentric frame.** Holds the positions of the Sun, planets, and moons, evaluated from real ephemerides (JPL data or an analytic approximation) at the simulation time. Evaluation is deterministic and closed-form, not integrated.

**Body-centred inertial frame.** Non-rotating, centred on a body. Used for moons, spacecraft, and impactors inside a body's sphere of influence (patched-conic model, [10](10-physics.md)).

**Body-fixed frame.** For Earth, this is **ECEF on the WGS84 ellipsoid**: the same frame used by Cesium, Google 3D Tiles, and GPS, which removes conversions at the provider boundary. Earth's rotation relative to the inertial frame comes from simulation time through Earth rotation angle and precession and nutation models simplified to the precision we need; polar motion is ignored. Rotation phases are computed with integer arithmetic first, so they stay exact over long durations.

**Surface cell frames.** The surface is divided by an **S2-style cube-sphere cell hierarchy**, addressed through latitude and longitude: six faces, each a quadtree, with 64-bit cell IDs ordered along a Hilbert curve (ideal for database keys, spatial indexing, and streaming). Each cell has a local ENU tangent frame computed **on the WGS84 ellipsoid** at the cell centre, with a 64-bit transform into ECEF. Approximate sizes on Earth: level 10 ≈ 10 km, level 13 ≈ 1.2 km, level 16 ≈ 150 m, level 20 ≈ 10 m. Proposed default (to be confirmed in the Technical Prototype): **sector cells at level ~13 (about 1 km)** as the unit of simulation partitioning, materialization, streaming, and persistence. Coarser levels serve as district, city, and region nodes in the world tree ([08](08-world-state.md)).

**Entity positions on the surface** are stored as `(sector cell ID, 32-bit local ENU offset in metres)`. Within a ~1 km cell, 32-bit precision is about 0.06 mm. When an entity crosses a cell boundary, it is re-expressed in the neighbouring cell's frame at a deterministic point in the tick (cell membership is a Tier 1 integer decision; [25](25-determinism-policy.md)).

The exact types, the S2-compatible cell-ID scheme, the level-13 sector default, and how cell IDs stay bit-identical across platforms are fixed in [ADR 0015](../adr/0015-coordinate-representation.md).

## 8.4 Heights, geoid, and curvature

- All heights inside the core are **ellipsoidal heights** (above WGS84), which is what Cesium and Google tiles use.
- Source data that uses other vertical references is converted by the data pipeline: Copernicus DEM uses the EGM2008 geoid, and NYC LiDAR and building data use the local vertical datum (NAVD88). The geoid–ellipsoid separation around New York is roughly −30 m and up to about ±100 m worldwide, so skipping this conversion would visibly float or sink everything. A geoid model is part of the pipeline.
- At street scale the surface is treated as flat within a cell's tangent frame. Over 1 km the ellipsoid drops about 8 cm below the tangent plane, and terrain is generated per cell from absolute heights so that neighbouring cells meet without seams.

## 8.5 Rendering: camera-relative 32-bit

Each frame:
1. The camera's position is known in 64-bit ECEF (or another frame, beyond Earth).
2. For each visible cell, the core computes the 64-bit transform from cell to camera and converts it to 32-bit. Entities use their 32-bit local offsets, so the GPU works with small numbers near the camera.
3. Cesium for Unity's georeference provides the same camera-relative treatment for backdrop tiles. The core and Cesium share the same ECEF origin choice, which is synchronised each time the origin shifts.
4. Other bodies are chained through the inertial frames in 64-bit, and only the final camera-relative result is converted.

## 8.6 Physics bubble frame

Rigid-body physics ([10](10-physics.md)) runs in a **bubble frame**, a tangent frame anchored at a sector cell near the camera. When the bubble moves beyond a threshold, it is re-anchored at a tick boundary.

## 8.7 Orbits and space

- Planets and moons follow **real ephemerides**, evaluated deterministically at any simulation time.
- Craft, debris, or impactors created by interventions use patched conics and fixed-step integration in the relevant body's inertial frame.
- 64-bit floats in a barycentric frame are ample, because nothing interacts at metre scale in interplanetary space without first being re-expressed in a local frame.

## 8.8 Alternative considered: universal 64-bit integer fixed-point

A single universal frame in 64-bit integers (for example 1/1024 m resolution, covering about a light-year) gives exact addition everywhere. It was rejected as the primary representation because rotating frames, ephemerides, and most mathematics are naturally floating-point. It remains an option for specific Tier 1 needs.

## 8.9 Time representation

- **Simulation time** is a signed 64-bit integer of **microseconds**, with the epoch at a fixed UTC instant (for example 2000-01-01T12:00 UTC, the J2000 epoch). Its range is about ±292,000 years, and it is exact and deterministic.
- **Real calendar:** UTC dates, time zones (America/New_York for Bay Ridge, including daylight saving), the real sun position, and real seasons. Conversions to astronomical time scales (TT/TDB for ephemerides) are handled in one module; leap seconds are tabulated.
- **Start date:** a new world starts at the current real date, or at a date the player chooses; it never depends on the device clock after creation.
- Durations are 64-bit microseconds. Tick indices per system derive from simulation time and the system's step.
- Floating-point time is created only at the point of use (animation phase, orbital anomaly), with integer modular arithmetic first where phases must stay exact.
- Exact epoch, range, leap-second treatment, and how host wall-clock measurements enter the core: [ADR 0014](../adr/0014-time-representation.md).
