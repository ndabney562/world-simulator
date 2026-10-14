# ADR 0013: Technical Prototype proves the core path before visual providers

- Status: Accepted
- Date: 2026-10-14
- Decided by: Noah (from ChatGPT's review of Architecture V1)
- Issue: recorded at the Architecture V1 freeze; backlog item TP-012
- Supersedes: none (narrows the sequencing in [19-roadmap.md](../architecture/19-roadmap.md) and [20-mvp.md](../architecture/20-mvp.md) §21.2)

## Context
The Technical Prototype's purpose is to prove the foundational architecture: a Rust core, behind a coarse FFI boundary, running inside Unity on an iPhone 15 Pro. The roadmap also lists Cesium georeferencing, open-data geometry on device, and a quota-capped Google provider as part of the same stage. Those depend on third-party packages, accounts, and data, and could delay proof of the core path for reasons unrelated to it.

## Decision
- Geospatial and visual-provider work (Cesium for Unity, open-data tiles, Google Photorealistic 3D Tiles, the provider manager) is a **separate, later Technical Prototype spike (TP-012)**.
- No other Technical Prototype issue depends on TP-012. The Rust → FFI → Unity → iPhone path (TP-001 to TP-011) is proved with simple Unity primitives drawn from core snapshots, placed using the core's own coordinate types.
- The provider requirements themselves are unchanged: providers remain optional, swappable, and never simulation state (ADR 0008), and the "switching providers leaves the core state hash unchanged" test is still a Technical Prototype exit criterion, delivered by TP-012.

## Consequences
- The core path can be proved, measured on device, and reviewed without waiting on Cesium, Google, or data licensing.
- TP-012 can start once the Unity skeleton (TP-002) and coordinate foundation (TP-005) exist, in parallel with the rest.
- The Technical Prototype is not complete until TP-012 is done, because ADR 0008's test and the Cesium-on-device question are still exit criteria.
