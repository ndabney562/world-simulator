# Glossary

| Term | Meaning in this proposal |
|---|---|
| ADR | Architecture Decision Record: a short document in `docs/adr/` recording one decision, its context, and consequences. |
| Aggregate event | A world event summarising many individual happenings at a coarse level ("14 births in this district in March"). |
| Aggregation / disaggregation | Converting fine simulation state to coarse (summarising) and coarse to fine (generating consistent detail). |
| Attribution (causal) | The share of an aggregate outcome a coarse model can assign to a cause such as an intervention. |
| Attribution (licence) | Credit lines that data sources and providers require on screen. |
| Baseline | Content regenerated rather than stored: real open data from versioned simulation tiles plus synthetic, procedurally generated detail. |
| Canonical state | State that is the source of truth and cannot be regenerated; it is what saves contain. |
| Causal link | A typed link from an event to an earlier event (`caused_by`, `enabled_by`, `triggered_by`, `contributed_to`), with a confidence class. |
| Cell / sector cell | A node of the S2-style cube-sphere hierarchy over Earth's surface. A sector cell (~1 km) is the unit of materialization, streaming, and persistence. |
| Command / journal | A validated input to the simulation, and the ordered log of them, used for replay, undo, and debugging. |
| Confidence class | How strongly a causal claim is supported: `direct`, `contributing`, `statistical`, or `counterfactual`. |
| Core | The headless Rust simulation library. |
| Dataset version | The identifier of a processed open-data release (sources, pipeline version, hashes) that a save pins. |
| ECEF / WGS84 / ENU | Earth-centred Earth-fixed frame; the standard reference ellipsoid; a local East-North-Up tangent frame. |
| Effect vocabulary | The finite, extensible set of effect types that god interventions are composed from. |
| Exclusion zone | A polygon supplied by the core where simulation geometry replaces any visual backdrop. |
| Front end | The Unity 6 / URP app that draws snapshots and sends commands. |
| Interest manager | The part of the core that decides each node's simulation LOD level. |
| L0–L7 | Simulation LOD levels, from focused individuals (L0) to celestial mechanics (L7). |
| Materialization / dematerialization | Promoting a node to finer simulation detail / demoting it to coarser detail. |
| Notable | An entity stored persistently because the player or the simulation made it significant. |
| Observation cache | Short-lived storage of exact fine state for recently dematerialized nodes. |
| Physics bubble | The small region near the camera where rigid-body physics runs. |
| Provenance tag | A bounded, decaying label on state recording which root events (such as interventions) changed it. |
| Render snapshot | The immutable per-tick description of what the front end should draw. |
| Simulation tiles | Versioned, processed open data keyed by cell ID, produced by the offline data pipeline. |
| Synthetic population | Generated residents calibrated to real statistics; never real individuals. |
| Tier 1–4 | The reproducibility tiers in [25](25-determinism-policy.md). |
| Visual backdrop provider | A swappable, display-only source of visuals (open data, Google 3D Tiles, NASA imagery) that never affects simulation state. |
| Visual proxy | Decorative rendering data (crowd density, traffic flow) derived from aggregates where no individuals are simulated. |
| World event | A structured record of something meaningful that happened in the simulation, with time, location, actors, type, causes, effects, and importance. |
