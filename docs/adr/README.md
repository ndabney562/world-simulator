# Architecture Decision Records

One file per decision: `NNNN-short-title.md`, with Status (Proposed / Accepted / Superseded), Context, Decision, and Consequences. Changes to an accepted decision get a new ADR that supersedes the old one. Start from [0000-template.md](0000-template.md); the process is in [docs/process/architecture-change-process.md](../process/architecture-change-process.md).

| ADR | Decision | Status |
|---|---|---|
| [0001](0001-real-earth.md) | The world is real Earth | Accepted |
| [0002](0002-realistic-visuals.md) | Realistic visuals preferred | Accepted |
| [0003](0003-single-player.md) | Single-player only | Accepted |
| [0004](0004-minimum-device-iphone-15-pro.md) | Minimum device is the iPhone 15 Pro | Accepted |
| [0005](0005-stack-rust-core-unity-cesium.md) | Stack: Rust core + Unity 6 (URP) + Cesium for Unity | Accepted |
| [0006](0006-mvp-area-bay-ridge.md) | MVP area: Bay Ridge, Brooklyn | Accepted |
| [0007](0007-zero-cost-policy.md) | Zero-cost policy: open data by default, Google tiles optional and capped | Accepted |
| [0008](0008-visual-backdrop-providers.md) | Visual backdrops are optional, swappable providers | Accepted |
| [0009](0009-stage-progression.md) | Stage progression: Technical Prototype → Vertical Slice → Neighbourhood MVP → City → Planet → Solar System | Accepted |
| [0010](0010-tiered-determinism.md) | Tiered reproducibility instead of strict cross-platform determinism | Accepted |
| [0011](0011-world-event-history-system.md) | First-class world event and history system | Accepted |
| [0012](0012-causal-provenance.md) | Practical causal provenance | Accepted |
| [0013](0013-technical-prototype-core-path-first.md) | Technical Prototype proves the core path before visual providers | Accepted |
| [0014](0014-time-representation.md) | Simulation time representation: uniform i64 microseconds from 2000-01-01T12:00 UTC, leap-second table for UTC | Accepted |
| [0015](0015-coordinate-representation.md) | Coordinate representation (D-4) and default sector level | Accepted |
| [0016](0016-entity-identity-storage-and-randomness.md) | Entity identity, component storage, and counter-based randomness (D-7, S-1) | Accepted |
| [0017](0017-simulation-lod-l1-l2-round-trip.md) | Simulation LOD: the L1 ↔ L2 round trip (D-10, TP-007) | Accepted |
| [0018](0018-rust-unity-bridge.md) | TP-003 Rust–Unity bridge contract (S-3) | Accepted |
| [0019](0019-command-journal-and-world-events.md) | TP-008 command journal and World Event contract (D-8) | Accepted |
| [0020](0020-save-format-and-versioning.md) | TP-009 save container, serialization format (S-2) and version header (D-14) | Accepted |
| [0021](0021-god-intervention-contract.md) | TP-010 first structured god intervention: command, effect vocabulary v1, Illness state, events, format 2 | Accepted |
