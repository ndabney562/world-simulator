# ADR 0028: slice L1 needs, sidewalk proxy paths and L2 reconciliation

- Status: Accepted
- Date: 2026-10-20
- Decided by: Noah, standing technical delegation on #77, applied after independent review on PR #88; proposed by Codex
- Issue: #69 (VS-005 design phase)
- Supersedes: none; details the real-slice extension of ADRs 0017 and 0027

## Context

VS-004 is merged. [Issue #69](https://github.com/ndabney562/world-simulator/issues/69) requires rest/hunger/social/duty needs, utility choices, deterministic paths, on-time scheduled destinations, and conservative L1/L2 round trips. ADR 0017's executable movement rules describe the toy world; ADR 0027 supplies real-slice timetables but intentionally defers L1 reconciliation.

The pinned extract contains footprints, not surveyed sidewalk topology or entrance locations. The issue permits a simple derived graph. Existing off-map workplaces refer to two registered synthetic gateways, not simulated roads outside the block. Those distinctions must remain explicit instead of inventing street data or allowing fine simulation to miss a timetable deadline.

This PR is docs only. The accepted policy does not itself change runtime behavior, old goldens, APIs or formats. Independent review of the proposed contract approved with zero findings; acceptance is recorded under the standing delegation on #77 and PR #88. Merge still requires green final-head CI and clean final-head review. Implementation follows the merged contract in a separate PR against #69.

## Options considered

1. Let utility decisions replace mandatory appointments and repair position on demotion. Rejected: missed deadlines and visible jumps violate the issue and conservation contract.
2. Add surveyed navigation, traffic, interiors or a general planner now. Rejected: unsupported data and excluded scope.
3. Preserve the reviewed schedule, use a small derived perimeter graph for pedestrian travel, and retain common canonical need/decision state across levels. Chosen: useful fine behavior and bounded paths without changing the structural baseline or fabricating data.

## Decision

### Ownership, versions and scope

Extend `ws-life` with navigation, needs, utility selection and real-slice fine/coarse behavior. Extend `ws-sim::slice::SliceSimulation` to own the behavior state and apply typed Rust-side detail requests at serialized boundaries. No domain-to-domain dependency, new crate, third-party dependency, FFI/wire/Unity API, save format, calendar or camera-interest manager is introduced.

Pin navigation rules version 1 and behavior rules version 1 separately from unchanged schedule rules version 1, structural/population versions and dataset digest. Include the new versions, graph fingerprint and canonical behavior state in slice hashing. Existing toy hashes, old payload tags/fields and save fixtures are unaffected. Slice hashes change within the same-build Tier 2 contract; no old Tier 1 golden is replaced. `SliceCreated` retains ADR 0027's exact fields: the additional behavior/navigation versions are pinned by the Rust simulation implementation/configuration and its state hash, not silently appended to an old payload or save record.

### A simple derived sidewalk proxy graph

The graph is a **synthetic navigation proxy**, not a survey, an OSM road graph or a claim that each proxy is a real door. Use the already pinned `BLOCK_BUILDINGS` geometry (all 58 footprints) and the population's two existing exits; fetch no new data and change no registered source bytes.

Define an integer local navigation plane anchored at the minimum source longitude/latitude. For a point in source picodegrees, compute east millimetres by Euclidean division of `(lon - origin_lon) * 84_500_000 / 10^12`, and north millimetres by `(lat - origin_lat) * 111_200_000 / 10^12`, with i128 intermediates. These fixed approximate local scale factors are navigation policy, not a replacement for ADR 0015 geodesy, terrain or vertical datums. Rendering later converts explicitly through a documented adapter; local navigation coordinates must never be passed as ENU/ECEF or surveyed elevations.

Take the bounding rectangle of all projected footprint points and pad it outward by 5,000 mm on each side. The four rectangle sides form the only walking edges. Map each featured building centroid and each registered gateway to its nearest rectangle side; ties use west, east, south, north order. The resulting perimeter point is that building's **portal proxy** or gateway proxy. A local stationary person is represented at its portal proxy; no interior traversal or centroid-to-side shortcut is simulated. Multiple semantic places may share one graph point without sharing their entity identities.

Deduplicate corner/portal/gateway coordinates, then assign graph-local node indices in lexicographic `(east_mm, north_mm)` order. These indices are versioned navigation indices, not new `EntityId`s. Connect consecutive points on each side bidirectionally; no diagonal or through-footprint shortcut exists. Weights are positive integer Manhattan distances, equal to length on these axis-aligned edges. Co-located points are merged, never connected by a zero-cost edge. All nodes lie outside the projected footprint envelope.

Shortest paths minimize integer length, with the lexicographically smallest complete node-index sequence winning ties. Return a typed failure for unknown/disconnected endpoints or invalid geometry. No hash-map iteration, ambient randomness or floating point selects topology, a path or its cost. Exhaustive all-pairs paths for the at-most-ten nodes may be cached as derived state. New independent graph/path goldens freeze coordinates, edges, semantic mappings, costs and tie decisions on both CI platforms.

An offline feasibility probe of the current extract gives a padded rectangle approximately 230,925 by 131,510 mm: no shortest perimeter path exceeds its half-perimeter, 362,435 mm. This is a design probe, not the implementation golden or performance evidence. Recompute and test the exact bound in implementation.

### Scheduled journeys and deadlines

ADR 0027's `AtBuilding`, `OffMap` and `InTransit` logical locations and half-open activity intervals remain authoritative. Walking follows the selected graph path at 1,400 mm per simulated second. At each 100-ms boundary, evaluate integer arc progress from the leg's fixed departure and route rather than accumulating floating position error. Interpolate an edge using wide integer arithmetic and a documented rounding direction. Exact endpoints, route node sequence, distance and progress are testable independently of rendering.

For an on-block journey, begin walking at scheduled departure. Arrive at the destination portal when the path is exhausted and wait there for the remaining commute interval; logical activity becomes the next scheduled activity exactly at the existing arrival boundary. Waiting at a portal during a commute is one state/location, not simultaneous presence at both endpoints. Same-place journeys have zero route length and retain the same timetable interval without fabricated movement facts.

For outbound off-map travel, walk home portal to the registered exit proxy first; after reaching the exit, represent the remaining journey as off-map transit with the original workplace/exit identity. At the scheduled arrival, logical location is the registered off-map workplace. For the return journey, the unmodelled external segment occurs first: remain off-map until the fixed last local walking segment can start, then walk exit proxy to the home portal and reach it at scheduled arrival. Never interpolate a fake road or remove the resident while outside the map. Fine location is a closed typed union of a local point (including portal waiting), off-map transit, or off-map stationary residence; coarse logical location remains the separate unique ADR 0027 answer.

Validate every possible scheduled route against its allotted interval at construction. The shortest current commute is ten minutes (840,000 mm at the chosen speed), longer than the perimeter bound above. Reject infeasible configurations with a typed error; do not silently speed up, teleport at a level change, skip an appointment, or change old schedule goldens. Exact deadline tests must include outbound, return, same-place, midnight/week wrap and negative-epoch journeys for every resident over a full week.

### Need quantities and utility decisions

Needs are deficits: rest, hunger, social and duty, each an integer in `0..=100_000`. Initial deficits use a new counter stream purpose `ws-life/needs/initial`, addressed by resident ID, draws 0 through 3, each reduced into 100,001 values with the existing `below` policy. These initial values and the choice policy are new behavior-rule-v1 goldens; they do not reuse a structural/population/schedule stream.

Every 100-ms physical tick adds the base deficit rates `(1, 2, 1, 2)`. The held action adds these effects before clamping each result to the need range:

| Action tag | Action | Rest | Hunger | Social | Duty |
|---|---|---:|---:|---:|---:|
| 0 | Idle | 0 | 0 | 0 | 0 |
| 1 | Rest | -4 | 0 | 0 | 0 |
| 2 | Eat | 0 | -12 | 0 | 0 |
| 3 | Socialize | 0 | 0 | -8 | 0 |
| 4 | Tend duties | 0 | 0 | 0 | -8 |
| 5 | Travel | 0 | 0 | 0 | 0 |
| 6 | Sleep | -8 | 0 | 0 | 0 |

At creation and at each model-minute boundary (`schedule_epoch + k * 60 seconds`), evaluate current needs after the preceding interval, then choose the action for the following interval. All schedule transitions lie on these minute boundaries and are applied before the new action is selected. A partial initial minute is allowed; need integration uses the actual whole 100-ms ticks from creation. A level change never triggers an extra decision or random draw.

Sleep, commute, work/school and errands constrain the action respectively to Sleep, Travel, Tend duties and Tend duties. At Home, choose among Idle, Rest, Eat, Socialize and Tend duties, with scores `0`, `4*rest`, `4*hunger`, `3*social`, `2*duty`. Activities never leave the current building or override the next scheduled departure. Socialize is a synthetic local activity, not a new relationship/crowd simulation; Eat creates no food/economic inventory. Tests must demonstrate distinct choices under different need states, not merely repeat a fixed schedule action.

For equal highest scores, compare counter-stream draws from purpose `ws-life/utility/tie`, addressed by `(resident ID, action tag)`, at the resident's canonical decision counter; highest draw wins, then lowest action tag. Increment the counter exactly once per actual decision, including creation, at both levels. Reject counter/time overflow without partial mutation. No shared RNG, per-render-frame noise or level-dependent draw count is allowed.

### Fine/coarse state and conservative catch-up

The common canonical resident record contains ID, detail level L1/L2, need anchor values/time, held action, decision counter and decision-time progress. Immutable population/household/role, schedules and versions remain baseline inputs. Journey endpoints/times/path come from that pinned schedule and graph; there is no independent late-arrival trajectory to repair. At L1, expose the exact local path position or explicit off-map fine location at the current boundary. At L2, retain common need/decision state and compute coarse location from the schedule. Fine positions/path caches are reconstructable/derived, not discarded canonical needs or decision progress.

Need integration over an interval with a fixed action is the exact clamped linear formula with i128 intermediates. Coarse catch-up drains the same minute decisions in the same order as L1, integrating each held-action interval exactly. It never resets needs to baseline or skips utility draws. Quiet ticks can reuse anchors until the next decision; capacities and acceleration caches are excluded from canonical hashes. No behavior reads the factual event store to decide what to do.

Manual Rust-side `SetDetail { resident, level }` operations make L1/L2 independently exercisable before #70 supplies camera-driven selection/batching. Validate known identity and construct any derived materialization before publication. Rejected requests leave canonical state/events unchanged and have explicit typed recorded outcomes. Successful detail operations enter the existing typed operation history/digest; no new persistent command transport is introduced. #70 retains camera interests, per-node policy, hysteresis, transition budgeting and production LOD event semantics.

For real-slice L1 -> L2 -> L1 at the same boundary, canonical state/hash, needs, action/counter, IDs/home counts, schedule destination/deadline and exact fine position must be unchanged. Level membership must remain a disjoint complete partition of the population; off-map people remain in that partition. With elapsed time, retained anchors and identical minute drains must give exact needs/actions/counters and route positions at a common boundary, stronger than ADR 0017's 0.1 m/one-base-step geometry tolerances. Do not loosen any existing toy tolerance or fixture; ADR 0017's toy implementation is untouched.

### Atomic orchestration, events and compatibility

Each physical tick plans behavior/clock and ADR 0027 schedule facts before committing any of them. Reserve all needed state/history storage before publication; no commit-phase allocation or fallible validation may leave behavior ahead of clock/events. On failure, retain the failing tick's canonical state and factual history, report already committed batch prefixes, and preserve the existing pacing/replay stop fences. Extend fault injection and structured replay to behavior planning and detail operations, including resource failures. Hash canonical fields explicitly in resident-ID order, with history/operation digests separate.

Keep schedule facts at exactly their existing boundaries/producer/LOD/participant contract, including when the resident is fine: they report the common schedule, not a newly invented physical arrival. Need micro-actions, cache rebuilds and level materialization create no new factual payload, synthetic provenance or false arrival. VS-010/#74 owns genuine intervention consequences, and #70 owns the later camera/LOD event contract. Unknown resident/detail requests receive documented typed outcomes; replay must verify them and the unchanged-state checkpoint rather than silently ignoring them.

Old toy commands, event tags/digest order, save-format-1/2 encoders/fixtures, CLI and FFI retain their contracts. Slice saves remain #53/#75 under ADR 0025, which must later persist the retained need/action/counter anchors and versions directly rather than replaying history. No test/CI weakening, device evidence or stage-exit waiver is part of this decision.

## Required implementation evidence

- Independent integer graph/path oracle and new goldens on Linux/macOS; positive edges, connectivity, lexicographic equal-cost ties, unknown/disconnected endpoints, route costs and path containment outside the footprint envelope.
- A full simulated week for all residents checking every scheduled destination/deadline and off-map identity, with selected exact ticks before/at/after arrivals and actual L1 path positions, not only coarse schedule answers.
- Need/rate/clamp and weighted-utility unit/property tests, all action constraints, tie stream/counter goldens and a reference minute-by-minute catch-up implementation.
- Zero-time and elapsed L1/L2 round-trip properties: exact identity/count conservation, need/action/counter/hash and fine position; mixed levels, negative/extreme epochs, same-place and off-map journeys. Existing toy tolerance tests remain unchanged.
- Typed detail/step/host-scale scripts replay in a separate process with per-boundary state/events/operation checkpoints. Failure injection covers atomic behavior/history rollback, committed prefixes, rejected detail requests and resource-stop replay.
- All existing suites/fixtures plus docs/dependency/Unity static checks, header/native C/iOS archive/CLI checks. Record all-pairs path-query and 10–30-resident active-step/catch-up measurements; host results establish no device/frame claim.

## Consequences

Fine residents acquire visible local paths and need-driven choices without missing the existing schedule. L2 retains enough canonical state to prevent zooming from resetting an individual's progress. A small approximate perimeter graph is reproducible from existing data and deliberately replaceable through a navigation-version change; it does not establish real street/door fidelity. The graph proxy, constant walking speed and common coarse need decisions limit realism, while keeping the required invariants directly testable.

Acceptance authorizes only the separate #69 implementation under the recorded workstream delegation. Revisit when real entrance/sidewalk data arrives, schedule-independent trips/interventions need explicit reconciliation, need-drain measurements exceed budget, or later rendering requires a geodetic adapter. Camera-driven LOD, real calendar, Unity rendering, illness consequences, persistence and device validation remain their existing issues.
