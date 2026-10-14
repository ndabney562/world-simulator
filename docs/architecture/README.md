# World Simulator — Technical Architecture Proposal

Status: **Proposal, for review** (no implementation yet)
Author: Claude (Anthropic), for Noah
Date: 2026-10-14
Intended reviewers: Noah, ChatGPT (independent review)

This directory is the first architecture and technical-planning pass for World Simulator. It deliberately contains no implementation code. Its job is to make the hard decisions explicit, propose a foundation that can realistically grow from a street-level prototype to a solar-system-scale simulation on iPhone, and be honest about which parts of the vision require approximation.

## How to read this

Each file maps to one or two of the 22 points in the brief, so a reviewer can check coverage point by point.

| # | Topic | File |
|---|-------|------|
| 1 | Hardest engineering problems (and a reality check on the vision) | [01-hard-problems.md](01-hard-problems.md) |
| 2, 3 | Candidate technology stacks and the recommended stack | [02-technology-stacks.md](02-technology-stacks.md) |
| 4 | Rendering architecture | [03-rendering-architecture.md](03-rendering-architecture.md) |
| 5 | Simulation architecture | [04-simulation-architecture.md](04-simulation-architecture.md) |
| 6 | Simulation LOD | [05-simulation-lod.md](05-simulation-lod.md) |
| 7 | Rendering LOD | [06-rendering-lod.md](06-rendering-lod.md) |
| 8 | Coordinate system (and time) | [07-coordinates-and-time.md](07-coordinates-and-time.md) |
| 9 | World-state architecture | [08-world-state.md](08-world-state.md) |
| 10 | Entity / agent architecture | [09-entities-and-agents.md](09-entities-and-agents.md) |
| 11 | Physics architecture | [10-physics.md](10-physics.md) |
| 12 | Procedural generation | [11-procedural-generation.md](11-procedural-generation.md) |
| 13 | Materialization / dematerialization | [12-materialization.md](12-materialization.md) |
| 14 | God-intervention system | [13-god-interventions.md](13-god-interventions.md) |
| 15 | Persistence / saves | [14-persistence.md](14-persistence.md) |
| 16 | iPhone performance and memory | [15-ios-performance-memory.md](15-ios-performance-memory.md) |
| 17 | AI / LLM augmentation of NPCs | [16-ai-llm-npcs.md](16-ai-llm-npcs.md) |
| 18 | Repository structure | [17-repository-structure.md](17-repository-structure.md) |
| 19 | Testing strategy | [18-testing-strategy.md](18-testing-strategy.md) |
| 20 | Phased roadmap | [19-roadmap.md](19-roadmap.md) |
| 21 | First playable MVP | [20-mvp.md](20-mvp.md) |
| 22 | Decide now vs. postpone | [21-decisions-now-vs-later.md](21-decisions-now-vs-later.md) |
| — | Glossary of terms used across the documents | [glossary.md](glossary.md) |

## Important caveat: inputs this proposal was based on

The brief asked for the complete Project Instructions to be read first. **No Project Instructions or vision document were available to this session**: the repository was empty (no commits) and the project had no instructions text visible to the session that wrote this. The proposal is therefore based on the vision as described in the brief itself, plus the assumptions below. Any assumption that is wrong should be corrected before the "decide now" items in [21](21-decisions-now-vs-later.md) are locked.

**Recommendation:** commit the Project Instructions / vision document into the repository (for example `docs/vision/project-instructions.md`) so that every collaborator, human or AI, reviews against the same text.

### Working assumptions about the vision

| ID | Assumption | Why it matters |
|----|------------|----------------|
| A1 | Single-player, offline-first game/simulation. | Multiplayer would change persistence, determinism and networking requirements fundamentally. |
| A2 | Primary platform is modern iPhone; other platforms are secondary. | Drives the stack, the memory budget and the renderer. |
| A3 | The player is an observer/"god" who can move the camera continuously from a single street to the whole solar system and intervene at any scale. | Drives the LOD, materialization and intervention design. |
| A4 | The world is populated by NPCs living plausible daily lives (homes, work, relationships), not just scenery. | Drives the agent architecture and the cost model. |
| A5 | The world is fictional and procedurally generated (a procedurally generated Earth-like planet in a procedurally generated or stylized solar system), not a 1:1 replica of real Earth. | Real-Earth data is a different project; see [01](01-hard-problems.md) and decision D-11. |
| A6 | Visual fidelity is secondary to simulation depth and scale continuity. | Lets us commit to a stylized art direction, which is essential on iPhone at these scales. |
| A7 | Development is done largely with AI collaborators (Claude, ChatGPT) working in Linux containers that cannot run Xcode, plus Noah on a Mac for device builds. | Strongly favours a headless, Linux-testable simulation core. |

## Executive summary

**The core insight.** "Continuous scale from street to solar system" is not one simulation running at all scales. It is a hierarchy of *different* simulations, each valid at its own scale, plus a disciplined contract for moving information up (aggregation) and down (materialization) between them. The world is only ever fully detailed where someone is looking or where something important is happening. Everything else is statistics, schedules, and deterministic procedural generation that can re-create plausible detail on demand. Getting that contract right is the project; rendering and content are comparatively well-understood problems.

**Recommended stack (point 3).** A custom, headless, deterministic **simulation core in Rust** (a Cargo workspace that builds and runs its full test suite on Linux and macOS), embedded in a **native iOS app written in Swift with a Metal renderer**. The core knows nothing about rendering; the renderer only consumes per-frame snapshots. This gives full control over the three things off-the-shelf engines handle poorly for this project: multi-scale coordinates, simulation LOD, and determinism. It also lets the AI collaborators build and test most of the system in their Linux environments. The main cost is that we must build our own renderer, tools and asset pipeline, which is why the proposal commits to a stylized look and keeps the core engine-agnostic so a Unity front end remains a fallback.

**Foundational decisions proposed now** (detail in [21](21-decisions-now-vs-later.md)):

1. Headless, deterministic core; renderer is a pure consumer of snapshots.
2. Hierarchical reference frames with 64-bit floats per frame, cell-addressed surface positions with 32-bit local offsets, and camera-relative 32-bit rendering. 64-bit integer time.
3. World = *seed + generator versions + journal of deltas*. Procedural content is never stored unless something changed it.
4. Every state mutation, from player and interventions alike (and eventually LLM suggestions), is a command recorded in a journal, which makes saves, replay, undo and debugging the same mechanism.
5. A formal aggregation / disaggregation contract between simulation LOD levels, with conservation tests.
6. Stable, deterministic entity identity so that the same person reappears when you zoom back in.
7. A stylized art direction and a minimum device target (proposed: A17 Pro / iPhone 15 Pro class and newer, 8 GB RAM).

**Recommended MVP (point 21).** "One living neighborhood": a few procedurally generated city blocks, a few hundred NPCs with daily schedules, a camera that goes from street level to a few kilometres up, two simulation LOD levels with real materialization/dematerialization, three god interventions, time controls, an inspector, and save/load. It is small in content but exercises every architectural spine the later phases depend on.
