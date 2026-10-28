# World Simulator

A continuous-scale simulation of real Earth, from a single Bay Ridge street to the solar system, targeting iPhone.

**Status (2026-10-28):** Technical Prototype complete; Vertical Slice engineering includes real buildings, 30 synthetic residents, schedules, L1 behavior, camera LOD, calendar/sun, rendering, inspection, finite illness, baseline/delta persistence (#53) and whole-slice format-4 saves (#75). Rendering/inspection iPhone acceptance (#72/#73) and the final 20-minute device report (#76) remain open. See [tracker #77](https://github.com/ndabney562/world-simulator/issues/77).

**Next:** desktop browser delivery first; phone browsers later. The browser compatibility spike (#106) is merged; a reproducible full Unity player, durable browser saves, fault recovery and release notices are tracked in [#112](https://github.com/ndabney562/world-simulator/issues/112). [ADR 0035](docs/adr/0035-browser-delivery-webgl-wasm.md) was accepted by Noah on 2026-10-28 (recorded separately in PR #113). No public game release or device/stage completion is claimed. See [current workstream](docs/process/current-workstream.md) and [distribution brief](docs/process/friend-distribution.md).

- Architecture proposal: [docs/architecture/README.md](docs/architecture/README.md)
- Architecture decision records: [docs/adr/](docs/adr/README.md)
- Engineering process (workflow, Definition of Done, testing, review): [docs/process/](docs/process/README.md)
- Technical Prototype backlog: [docs/process/technical-prototype-backlog.md](docs/process/technical-prototype-backlog.md)
- Conventions for AI collaborators: [AGENTS.md](AGENTS.md)

This repository is the canonical source of truth for the project and is shared between human and AI collaborators (Codex, Claude, and ChatGPT).

Start here for the [shared daily development workflow](docs/process/shared-development.md).
