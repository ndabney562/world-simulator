# 21. First Playable MVP

> **Revision 3 (2026-10-14):** The MVP area is **Bay Ridge, Brooklyn**, rendered from free NYC open data by default, with Google tiles optional ([22 §R.11](22-real-earth-and-realism.md#r11-revision-3-2026-10-14-bay-ridge-stack-accepted-zero-cost)).
>
> **Revision 2 (2026-10-14):** The MVP now uses **one real neighbourhood chosen by Noah**, with a Google 3D Tiles backdrop, near-field buildings built from OSM data, and a synthetic population. Everything else below stands. See [22 §R.8](22-real-earth-and-realism.md#r8-revised-mvp).

Covers point 21. This is the deliverable of Phase 2 in [19](19-roadmap.md).

## 21.1 Principle

The MVP is a **vertical slice of the architecture, not of the content**. It is small in scope but uses the real coordinate system, the real LOD and materialization pipeline, the real command journal, and the real persistence format, so that everything built afterwards extends it rather than replaces it. It deliberately does not attempt the zoom-to-orbit spectacle.

## 21.2 Working title: "One Living Neighbourhood"

### In scope

| Area | Scope |
|---|---|
| World | One procedurally generated neighbourhood of roughly 6–12 city blocks (~0.5–1 km²) on a single sector of a (not yet visible) planet, using the real cell-ID and frame system |
| Content | Houses and apartment buildings, a few shops, a workplace or two, a school, a park, streets with sidewalks; stylized, procedurally assembled from a small modular kit |
| Population | ~300–800 residents in households, each with deterministic identity, name, age, occupation, traits, home, and workplace or school |
| Agent behaviour | Daily and weekly schedules (L2), needs and utility decisions with pathfinding near the camera (L1), simple social interactions (greetings, friendships forming) |
| Simulation LOD | At least two individual levels (L1 ⇄ L2) plus building aggregates (L3), with full materialization/dematerialization and the observation cache; blocks far from the camera fall back to L2/L3 |
| Camera | Free camera from street level (~2 m) to about 1–2 km altitude, touch controls (pan, pinch, rotate), with rendering LOD for buildings and people across that range |
| Time | Day/night cycle; controls for pause, 1×, 10×, 100×; time scale caps simulation LOD as designed |
| Weather | Clear / rain, as an environmental field that changes behaviour (people stay indoors, carry umbrellas) |
| God interventions (3) | **Make it rain** (environment field, area + duration); **Spawn or remove a person** (existence, with household integration); **Bless or curse a person** (property modification: wealth, health or mood, with visible downstream consequences) |
| Inspector | Tap a person: identity, needs, current activity, schedule, relationships, recent history. Tap a building: occupants, use, history. Neighbourhood summary panel with the same aggregates the simulation uses |
| Persistence | Autosave and manual save/load with the real versioned format; notable entities persisted; reloading reproduces identical state |
| Performance | Sustained target frame rate for 20+ minutes on the minimum device, within the memory budget |

### Explicitly out of scope

Planet, terrain beyond a gentle heightfield, other cities, vehicles beyond parked props (or none), rigid-body physics (no physics bubble yet), fire or disasters, economy beyond household income and spending, interiors, combat, audio beyond basics, LLM features, iCloud, undo as a player feature (supported internally for testing), tutorials and onboarding beyond the essentials.

## 21.3 The player loop

1. Watch a neighbourhood wake up, commute, work, shop, meet, and sleep.
2. Zoom in on someone, learn who they are, and follow them.
3. Intervene (rain, bless/curse, add/remove someone) and watch the consequences ripple through schedules, relationships, and neighbourhood statistics.
4. Zoom out and back in and find the world exactly as consistent as you left it.
5. Save, quit, return later, and find your notable people still living their lives.

This is enough to test whether the core fantasy (a living world you can observe and influence) is engaging before scale is added.

## 21.4 Acceptance criteria

1. Zooming from street level to the top of the camera range and back, repeatedly, never loses or duplicates a person, and every person inspected earlier is where their schedule says they should be (or exactly where they were, within the observation-cache window).
2. Neighbourhood statistics shown in the UI equal the aggregation of individual state at all times (automated check).
3. A 30-day headless run in `ws-cli` finishes with all invariants holding and produces the same state hash on Linux and on macOS arm64 for the same build (or a documented, understood divergence).
4. Save/load round-trips produce identical state hashes.
5. Each intervention has a scenario test covering its effect on materialized and non-materialized targets.
6. Sustained frame rate and memory within budget for 20 minutes on the minimum device.
7. A tester who has never seen the project can identify, without help, at least one consequence of an intervention they made.
