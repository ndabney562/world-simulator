# Glossary

| Term | Meaning in this proposal |
|---|---|
| ADR | Architecture Decision Record: a short document in `docs/adr/` recording one decision, its context, options, and consequences. |
| Aggregation / disaggregation | Converting fine simulation state to coarse (summarising) and coarse to fine (generating consistent detail). |
| Canonical state | State that is the source of truth and cannot be regenerated; it is what saves contain. |
| Cell / sector cell | A node of the cube-sphere quadtree that divides a body's surface. A sector cell (~1 km) is the unit of materialization, streaming, and persistence. |
| Cell ID | A 64-bit identifier of a cell encoding face, level, and Hilbert-curve position. |
| Command | The only way to change simulation state from outside: validated, applied at a tick boundary, and recorded in the journal. |
| Core | The headless Rust simulation library. |
| Effect vocabulary | The finite, extensible set of effect types that god interventions are composed from. |
| Frame (reference frame) | A coordinate system with an origin and orientation (system inertial, body inertial, body-fixed, cell, object). |
| HLOD | Hierarchical level of detail: merging many objects into one simplified representation at distance. |
| Interest manager | The part of the core that decides each node's simulation LOD level from camera, focus, events, and budgets. |
| Journal | The ordered log of commands applied to the world, used for saves, replay, and undo. |
| L0–L7 | Simulation LOD levels from focused individuals (L0) to celestial mechanics (L7). |
| Materialization / dematerialization | Promoting a node to finer simulation detail / demoting it to coarser detail. |
| Notable | An entity that is stored persistently because the player or the simulation made it significant. |
| Observation cache | Short-lived storage of exact fine state for recently dematerialized nodes, so quick zoom-out/zoom-in shows the same scene. |
| Physics bubble | The small region near the camera where rigid-body physics runs. |
| Procedural baseline | Content generated deterministically from seeds and never stored unless changed. |
| Render snapshot | The immutable per-tick description of what the renderer should draw, produced by the core. |
| Visual proxy | Decorative rendering data (crowd density, traffic flow) derived from aggregates, used where no individuals are simulated. |
