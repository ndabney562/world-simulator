# World Simulator

A continuous-scale simulation of real Earth, from a single Bay Ridge street to the solar system, targeting iPhone.

**Status (2026-10-26):** Technical Prototype complete; Vertical Slice in progress ([tracker #77](https://github.com/ndabney562/world-simulator/issues/77)). Real buildings, synthetic residents, schedules, L1 behavior, camera LOD, calendar/sun, rendering, inspection and finite illness are merged. Rendering and inspection still need iPhone acceptance (#72/#73). Baseline/delta persistence (#53), whole-slice saves (#75), and the final 20-minute device run/report (#76) remain open. Merged code is not a device or stage-completion claim.

**Next:** #53 → #75 → #76. [Current workstream](docs/process/current-workstream.md) records ownership, existing delegated authorization and remaining gates. Friends must be able to play without building source; see the [distribution brief](docs/process/friend-distribution.md) for the pending target decision and cost constraints.

- Architecture proposal: [docs/architecture/README.md](docs/architecture/README.md)
- Architecture decision records: [docs/adr/](docs/adr/README.md)
- Engineering process (workflow, Definition of Done, testing, review): [docs/process/](docs/process/README.md)
- Technical Prototype backlog: [docs/process/technical-prototype-backlog.md](docs/process/technical-prototype-backlog.md)
- Conventions for AI collaborators: [AGENTS.md](AGENTS.md)

This repository is the canonical source of truth for the project and is shared between human and AI collaborators (Codex, Claude, and ChatGPT).

Start here for the [shared daily development workflow](docs/process/shared-development.md).
