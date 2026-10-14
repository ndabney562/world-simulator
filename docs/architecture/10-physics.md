# 11. Physics Architecture

Covers point 11.

## 11.1 Principle: no single physics engine, and no global physics

Physics is a set of **domain-specific models, each valid at its own scale**, all subordinate to the simulation LOD system. Nothing runs "physics" over the whole world. Rigid-body simulation in particular exists only in a small bubble near the camera, because it is by far the most expensive per object and the least important for simulation outcomes.

## 11.2 Physics domains

| Domain | Where it runs | Model | Notes |
|---|---|---|---|
| Rigid bodies, collisions, ragdolls, debris | Physics bubble only (a few hundred metres around the camera when at street band) | Rapier (Rust) with its determinism feature enabled, fixed 60 Hz step | Bodies created from procedural descriptors when the bubble covers them; removed (with lasting results recorded) when it leaves. |
| Character movement | L0/L1 agents | Kinematic character controllers plus navigation and local avoidance; not dynamic rigid bodies | Characters become dynamic only for specific events (ragdoll on impact). |
| Vehicles | L1 near camera | Kinematic lane-following; optional simplified dynamic vehicle in the bubble | Full vehicle dynamics only if the design adds driving. |
| Terrain and static environment | Physics bubble | Heightfield colliders plus building and prop colliders generated from procedural descriptors | Colliders cached per sector cell. |
| Projectiles and impacts | Bubble when local; parametric when large | Ballistic integration; impact effects from parametric models | |
| Fire, smoke | L1+ buildings and cells | Cellular spread model over building and parcel graphs (fuel, wind, suppression) | Visual particles independent of the spread model. |
| Water, floods | Cell level | Heightfield flood-fill or a coarse shallow-water model on a grid | No fluid simulation. |
| Weather | Region and planet | Procedural weather systems (fronts, storms) moving over climate zones on a coarse grid; local weather derived from them | No atmospheric CFD. |
| Climate | Planet | Zonal energy-balance-style model with biome feedback, run monthly or yearly | Enough to support "make the planet colder" interventions. |
| Geology (earthquakes, volcanoes) | Region | Event-based parametric models (magnitude, radius, damage curves) | Tectonics only at world-generation time. |
| Orbital mechanics | System | Analytic Kepler orbits for bodies; patched conics and fixed-step symplectic integration for craft | See [07](07-coordinates-and-time.md). |
| Large impacts and celestial interventions | System and planet | Parametric outcome models (impact energy to crater size, ejecta, climate effect) and scripted consequences | Scientifically inspired, not simulated. |

## 11.3 The physics bubble

- Anchored to a sector-cell tangent frame near the camera; re-anchored deterministically when the camera moves far enough ([07](07-coordinates-and-time.md)).
- Active only when the camera is at street or low neighbourhood band and the time scale is low (rigid-body physics is disabled at high time warp).
- Bubble size and body count are budgeted (for example ≤ 1,000 active bodies, aggressive sleeping).
- **Entering:** static colliders and dynamic objects in the bubble are instantiated from procedural descriptors plus canonical deltas.
- **Leaving:** objects in a stable resting state are converted back to descriptors. Lasting changes (a car flipped over, a wall knocked down) are recorded as canonical deltas; transient ones (a bouncing ball) are dropped or settled to rest first.

## 11.4 Determinism

- Rapier provides cross-platform determinism when built with its determinism feature on IEEE-754-compliant platforms; we additionally fix the step size, the order of body insertion (by entity ID), and the solver iteration counts.
- All other physics models are written in the core under the same determinism rules as the rest of the simulation ([04](04-simulation-architecture.md)).
- Physics results that matter for the long-term world (something broke, someone was injured) are emitted as events and recorded; the exact trajectories are not.

## 11.5 Cross-scale consistency for physical events

Physical events must have a definition at every level they can reach. For example, a fire:
- At L3: building-level spread probability per hour; occupants become casualties or displaced by distribution.
- At L1/L0: per-room spread, visible flames and smoke, individual agents fleeing, firefighters responding.
- At L4: district-level counts of fires, damage, insurance and economic effects.

The same event state (building on fire since 14:02, 40% burned) is valid at all levels, so the camera can arrive mid-fire and see a consistent scene.
