# World Simulator

**[Play the desktop preview](https://ndabney562.github.io/world-simulator/)** — open the link in a desktop browser; no build required. This preview simulates one Bay Ridge block with 30 residents. Save in the browser or download a backup before leaving.

A continuous-scale simulation of real Earth, from a single Bay Ridge street to the solar system, targeting iPhone.

**Status (2026-10-28):** Technical Prototype complete; Vertical Slice engineering includes real buildings, 30 synthetic residents, schedules, L1 behavior, camera LOD, calendar/sun, rendering, inspection, finite illness, baseline/delta persistence (#53) and whole-slice format-4 saves (#75). Rendering/inspection iPhone acceptance (#72/#73) and the final 20-minute device report (#76) remain open. See [tracker #77](https://github.com/ndabney562/world-simulator/issues/77).

**Desktop preview published (2026-10-28):** the pinned Unity/Rust player includes durable browser saves, import/export, fail-stop recovery and release notices. The local Chromium/Firefox/WebKit checks, manual Safari smoke checks and measurements are documented in [release evidence](docs/verification/issue-112/release-checks.md). Phone browsers and other desktop hardware/OS combinations remain unverified; native iPhone/device acceptance remains separate. See [publication record](docs/verification/issue-112/publication-proposal.md), [current workstream](docs/process/current-workstream.md) and [distribution brief](docs/process/friend-distribution.md).

- Architecture proposal: [docs/architecture/README.md](docs/architecture/README.md)
- Architecture decision records: [docs/adr/](docs/adr/README.md)
- Engineering process (workflow, Definition of Done, testing, review): [docs/process/](docs/process/README.md)
- Technical Prototype backlog: [docs/process/technical-prototype-backlog.md](docs/process/technical-prototype-backlog.md)
- Conventions for AI collaborators: [AGENTS.md](AGENTS.md)

This repository is the canonical source of truth for the project and is shared between human and AI collaborators (Codex, Claude, and ChatGPT).

Start here for the [shared daily development workflow](docs/process/shared-development.md).
