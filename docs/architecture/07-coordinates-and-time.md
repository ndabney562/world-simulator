# 8. Coordinate System (and Time Representation)

Covers point 8: a coordinate system that supports meter-scale interactions and astronomical distances. Time is included because it has the same multi-scale problem.

## 8.1 The numbers that drive the design

Spacing between adjacent representable values ("ulp") at various magnitudes:

| Magnitude | Example | 32-bit float spacing | 64-bit float spacing |
|---|---|---|---|
| 2 km | Within a city cell | ~0.12 mm | negligible |
| 10 km | Across a city | ~1 mm | negligible |
| 6.4 × 10⁶ m | Earth-sized planet radius | **0.5 m** | ~1 nm |
| 3.8 × 10⁸ m | Earth–Moon distance | ~32 m | ~60 nm |
| 1.5 × 10¹¹ m | 1 AU | **~16 km** | ~0.03 mm |
| 4.5 × 10¹² m | Neptune's orbit | ~500 km | **~1 mm** |

Conclusions:
1. 32-bit floats are only safe within a few kilometres of their origin.
2. 64-bit floats are comfortably precise within any single planet's frame, and still at millimetre level for a Sun-centred frame out to Neptune. A full hierarchy is therefore not needed for raw precision at the solar-system level. It is needed for **cheap, cache-friendly storage of millions of surface entities, for GPU rendering, for rotating frames, and for keeping physics numerically well-conditioned.**
3. The GPU must only ever receive camera-relative 32-bit values.

## 8.2 Units and conventions

- SI units internally everywhere: metres, seconds, kilograms, radians. Display units are a UI concern.
- Right-handed coordinate systems. Local tangent frames are East-North-Up (ENU).
- Every position and direction type is **tagged with its frame** at the type level in the core, so adding a position in frame A to one in frame B is a compile error, not a bug. Conversions go through explicit, tested transform functions.

## 8.3 The frame hierarchy

```
System inertial frame  (Sun-centred, non-rotating; 64-bit floats)
 └─ Body inertial frame  (centred on a planet/moon, axes parallel to system frame; 64-bit)
     └─ Body-fixed frame  (rotates with the body; 64-bit)
         └─ Surface cell frame  (cube-sphere cell; local ENU tangent frame; 32-bit offsets)
             └─ Object frame  (building, vehicle, character; 32-bit)
```

**System inertial frame.** Origin at the star (a fictional system can ignore barycentre motion; a real-ephemeris system would use the barycentre). Holds orbital elements for all bodies. 64-bit floats.

**Body inertial frame.** Origin at a body's centre, non-rotating. Used for moons, spacecraft, and anything orbiting within the body's sphere of influence (patched-conic model, see [10](10-physics.md)).

**Body-fixed frame.** Rotates with the body. Rotation is computed analytically from simulation time: the rotation **phase** is computed with integer arithmetic (`t mod rotation_period`) before converting to an angle, so the phase stays exact even after centuries of simulated time. Terrain, cities, and all surface simulation live here.

**Surface cell frames.** The body's surface is divided by a **cube-sphere quadtree**: six cube faces, each recursively split into four children, projected onto the sphere with an area-equalising projection (the approach used by Google's S2 geometry library). Each cell has:
- A **64-bit cell ID** encoding face, level and position along a Hilbert curve. Hilbert ordering keeps spatially close cells numerically close, which is ideal for database keys, spatial indexing and streaming.
- A local ENU tangent frame at the cell centre, with a known 64-bit transform into the body-fixed frame.

For an Earth-sized planet, approximate cell sizes: level 10 ≈ 10 km, level 13 ≈ 1.2 km, level 16 ≈ 150 m, level 20 ≈ 10 m. Proposed defaults (tunable via ADR):
- **Sector cells (level ~13, about 1 km):** the unit of simulation partitioning, materialization, streaming, and persistence for street-level content.
- Coarser levels serve as district, city, and region nodes in the world-state tree ([08](08-world-state.md)), so the spatial hierarchy for simulation and the terrain hierarchy for rendering are the same tree.

**Entity positions on the surface** are stored as `(sector cell ID, 32-bit local offset in metres)`. Within a ~1 km cell, 32-bit precision is about 0.06 mm, far better than needed, and each position costs 20 bytes instead of 24 for three 64-bit floats, with much better locality. When an entity crosses a cell boundary, it is re-expressed in the neighbouring cell's frame (a cheap, exact-enough operation done at a deterministic point in the tick).

**Object frames.** Building, vehicle, and character local frames for meshes, interiors and attachments.

## 8.4 Curvature and the "flat locally" approximation

At street scale the surface is treated as flat within a cell's tangent frame. The error is the sagitta `d² / 2R`: over 1 km on an Earth-sized planet the surface drops about 8 cm below the tangent plane, and over 3 km about 70 cm. Terrain heights are always measured from the reference sphere and converted per cell, so neighbouring cells meet without seams. Physics and pathfinding inside the physics bubble use one tangent frame for the whole bubble (a few hundred metres), where curvature is negligible. Planets are modelled as **spheres, not ellipsoids** (a deliberate simplification; oblateness can be added later behind the same interfaces).

## 8.5 Rendering: camera-relative 32-bit

Each frame:
1. The camera's position is known in 64-bit in some frame (usually the body-fixed frame of the nearest body).
2. For each visible cell, compute the 64-bit transform from cell frame to camera frame, then convert to 32-bit. Entities inside the cell use their 32-bit local offsets, so the GPU computes `cell_to_camera × local_offset` with small numbers near the camera. Distant cells have large translations, but at that distance the precision loss is far below a pixel.
3. For other bodies, chain the transforms through the inertial frames in 64-bit and convert only the final camera-relative result.

Depth precision is handled by depth-range partitioning ([03](03-rendering-architecture.md)), not by the coordinate system.

## 8.6 Physics bubble frame

The local physics engine (rigid bodies, character controllers) runs in a **bubble frame**: a tangent frame anchored at a sector cell near the camera. When the bubble needs to move more than a threshold distance, it is re-anchored (floating origin): all bodies inside are re-expressed in the new frame at a tick boundary, deterministically.

## 8.7 Orbits and space

- Bodies follow **analytic Keplerian orbits** (on rails) defined by orbital elements in the parent's inertial frame. Position at time t is computed by solving Kepler's equation with a fixed iteration count (for determinism), in 64-bit.
- Craft or debris (if the design adds them) use **patched conics**: inside a body's sphere of influence, their state is held in that body's inertial frame; crossing the boundary re-expresses state in the parent frame.
- 64-bit floats in a Sun-centred frame give ~1 mm precision at Neptune's orbit, which is ample because nothing interacts at metre scale in interplanetary space without being re-expressed in a local frame first.

## 8.8 Alternative considered: universal 64-bit integer fixed-point

A single universal frame using 64-bit integer coordinates (for example 1/1024 m resolution, covering about one light-year) gives exact, perfectly deterministic addition everywhere. It was rejected as the primary representation because rotating frames, orbits, and most math are naturally floating-point, and conversions would be everywhere. It remains an option for specific uses where exactness matters more than convenience (for example network-free lockstep or exact accumulation of long-running positions).

## 8.9 Time representation

- **Simulation time** is a signed 64-bit integer of **microseconds since the world epoch**. Range: about ±292,000 years. Exact, deterministic, comparable, and never accumulates rounding error.
- Durations are also 64-bit integer microseconds. Rates (for example "per day") are converted at the point of use.
- **Calendar** (day length, year length, seasons, local solar time) is derived from the body's rotation and orbit, never stored independently. A fictional planet may have, for example, a 26-hour day; the calendar module handles it.
- **Tick indices** per system are derived from simulation time and the system's step, so systems running at different rates stay aligned.
- Anything that needs floating-point time (an animation phase, an orbit's mean anomaly) converts from integer time at the point of use, using integer modular arithmetic first where phases must stay exact over long durations.
