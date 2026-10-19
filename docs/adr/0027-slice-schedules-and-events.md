# ADR 0027: slice L2 schedule model, factual events and orchestration

- Status: Accepted
- Date: 2026-10-19
- Decided by: Noah, direct standing approval on 2026-10-19, recorded on #77 and PR #86; proposed by Codex
- Issue: #68 (VS-004 design phase)
- Supersedes: none; extends ADRs 0017/0019 for the real slice

## Context

VS-003 is merged. [Issue #68](https://github.com/ndabney562/world-simulator/issues/68) needs role/household-derived sleep, commute, work/school, errands and home schedules, one location per resident, factual events and reproducible replay. The existing `World` runs the toy model, and its accepted event/save vocabulary does not describe these facts. A separate real-slice orchestration path must preserve existing toy behavior, codecs and fixtures rather than silently changing their meaning.

This docs-only proposal makes the necessary event-vocabulary/domain-boundary decisions first. No FFI, save format or runtime implementation changes ship here. Noah's standing approval on #77 covers technical acceptance/delegated merge only after independent review and green CI; it does not waive tests or device evidence.

## Options considered

1. Reuse toy arrivals for every activity change. Rejected: sleeping or waking is not a physical arrival, and inventing arrivals contradicts ADR 0019.
2. Add mutable schedules and rendering logic to the structural `ws-world` baseline. Rejected: conflates immutable generation with driving state/presentation.
3. A small `ws-life` domain plus `ws-sim::slice::SliceSimulation`, using the existing clock/event infrastructure and a typed factual extension. Chosen: the toy path remains compatible and later L1/LOD work extends this domain.

## Decision

### Boundaries and canonical time

Create `ws-life` only when implementation starts, with dependencies on existing foundation/world/events crates and no `ws-sim`, persistence, FFI, platform or provider dependency. It owns schedule templates, typed activity/location answers and transition planning. `ws-sim::slice::SliceSimulation` owns the populated baseline, clock, domain, event store and typed command/history boundary. Existing `World`, `ToyWorld`, CLI/toy APIs and old fixtures retain their contracts. The new crate/layer is registered in workspace/dependency checks and documented together; no third-party dependency is needed.

Slice configuration pins seed, start, **schedule epoch**, population/structural versions and schedule-rules version 1. Start and epoch lie on the existing 100-ms grid. Schedule epoch denotes Monday midnight **in the model's seven-day phase**, supplied explicitly; it is not inferred from the wall clock or falsely labelled as real New York time. Calendar/time-zone alignment belongs to VS-007 and must preserve/version this input when implemented.

All evaluation uses checked integer microseconds and Euclidean day/week division, including times before the epoch. Subtraction uses a wide intermediate so valid i64 endpoints cannot overflow. Intervals are half-open: the new activity owns an exact boundary. There is exactly one answer at every valid simulation time, including midnight, week wrap and negative times. No shared mutable RNG or host timing enters a schedule.

### Version-1 schedules and locations

Use typed activities `Sleep=0`, `Commute=1`, `Work=2`, `School=3`, `Errands=4`, `Home=5`. Templates depend on role and household; a small fixed integer jitter from a separate counter-based purpose keyed by household ID may shift related boundaries while preserving order and positive durations. Its label/range and the exact minute tables are frozen by new Tier-1 schedule goldens in implementation; changing either requires a rules-version bump and review.

Workers/students have weekday sleep, home, outbound commute, work/school, return commute, errands and home intervals. Weekends use a home/sleep/errand profile. Preschool/retired residents have no job/school interval. Every resident still has a complete daily template; no synthetic person is removed while off-map. Errands can visit the existing synthetic shop; workplace capacities from VS-003 concern job assignment, not shopping occupancy.

Location is one typed value: `AtBuilding(id)`, `OffMap(workplace_id, exit_index)`, or `InTransit(from, to, departure, arrival)`. A local workplace resolves to its building; an off-map destination resolves through the registered workplace/exit record. A commute carries both endpoints and exact timing; it never means simultaneous residence at both. No L1 pathfinding, terrain/height conversion, needs or relationships beyond the existing household are invented. Queries are pure functions of pinned schedule inputs and simulation time.

A rebuildable ordered next-boundary queue may avoid scanning every resident on every base tick. It is derived, excluded from canonical hashes and rebuilt/checked from the schedules and current time. Future L1 needs, illness and camera-interest overrides are separate issues; their rules must explicitly reconcile with this L2 contract rather than mutate templates behind an unrecorded path.

### Typed factual events

Extend the existing `ws-events` vocabulary with two payload tags; preserve tags 0–6 and every existing canonical field/digest order:

| Tag | Payload | Canonical fields |
|---:|---|---|
| 7 | `ScheduleChanged` | resident ID, destination ID, previous activity u8, next activity u8 |
| 8 | `SliceCreated` | seed u64, dataset u64, structural generator u32, population generator u32, schedule rules u32, schedule epoch i64 |

Tags 7/8 have fixed-width explicit fields and checked activity tags; no free text, native usize or floating identity. `ScheduleChanged` is a factual boundary transition, named `agent.schedule_changed`; consumers can identify left-for-work/school, shopping and home transitions from its typed fields and destination. It is not a fabricated arrival. Creation is `sim.slice_created`, once at the creation boundary; do not emit synthetic past transitions or false arrivals for the initial activity.

Register all baseline building/household/resident/workplace IDs with the existing EventStore. A schedule fact has resident Subject and destination Object, occurrence time exactly at the schedule boundary, system 60, LOD 2 and system origin 60. The destination is a known building or workplace; off-map location remains explicit in domain state. Slice creation is world-scoped, system/LOD 0. Validate payload IDs/activities, participants and producer tags before publication. No unsupported causal claim is emitted: ordinary schedule transitions have no invented intervention link; VS-010 later supplies genuine provenance.

Within each event-bearing physical tick, sort independent facts by `(occurrence time, resident ID)` before assigning per-system local sequence. Existing EventStore allocation supplies monotonic IDs/commit ordinals and all-or-nothing publication. A multi-tick host call must preserve the same event-bearing physical-tick commits as repeated single steps; host batch size never changes IDs/digests. Exact-boundary repeats, queries and initial construction must not duplicate facts.

### Orchestration, commands and failure

`SliceSimulation` uses the existing fixed-step clock and exposes structured boundary operations for stepping and setting the existing time scales. Record deterministic operation/outcome sequences and checkpoints; no direct provider/presentation mutation or arbitrary code execution. The new path is initially Rust-side; no FFI/wire/Unity migration is included in #68. Host elapsed time remains host input and pause/scale semantics follow the existing clock contract.

At each physical tick, validate planned time/state and reserve event storage before publishing time/domain/events. Infrastructure failure leaves the failing tick's canonical state/history unchanged; a host batch may retain its already committed tick prefix, explicitly reported and replayed. Canonical hashing covers configuration and generator/rules versions, time/scale and authoritative domain state in ID order, with separate history/operation digests as ADR 0019 requires. Derived query/queue capacity is not hashed. No new system reads the factual event store to drive decisions.

Replay tests exercise identical typed operation scripts in separate processes, comparing canonical state plus event/history digests, not wall-clock output. Reusing the toy debug tape format or introducing a new CLI file format is not required here; any later persistent debug transport needs an explicit versioned contract. Save/load must restore canonical values directly, not replay these operations across platforms.

### Legacy save compatibility

Payload tags 7/8 are reserved for new slice-capable record schemas/formats (format 3 or later under accepted ADR 0025). **Formats 1/2 never silently acquire new payload meanings.** Their decoders continue rejecting these tags; their writers reject unsupported slice records with a typed error before replacing a save. Any shared codec refactoring needed for exhaustive enum matches preserves old bytes, tags and fixture expectations, and has regression tests for the old-format rejection path. Never use a panic/unreachable assumption to guard an externally constructible record.

#53/#75 own versioned slice persistence and the eventual wire layouts. This ADR does not write a new save format, add background saving, or claim that a SliceSimulation can already be saved by the toy writer. New payload digest/validation support belongs to #68; actual new-format persistence follows its own accepted contract and fixtures.

## Required implementation evidence

- New golden minute tables/jitter and integer schedule evaluation across midnight/week wrap/negative times and extreme checked-time inputs; unchanged existing building/resident goldens.
- A week-long test for every resident against an independent interval reference, one location per person, correct commute endpoints and off-map identity conservation.
- Exact fact times/participants/producer/activity/destination tags, initial no-false-facts, no duplicate transitions, and actor/time query/index reconstruction equivalence.
- Single-step versus varied-batch operation scripts and separate-process replay with identical state/event/operation digests, including pause/scale changes and injected failing-tick rollback/prefix reporting.
- Old toy/command/save suites and format-1/2 fixtures remain unchanged; new slice tags rejected by old-format paths. No FFI or Unity tests are weakened.
- Record step/transition planning measurements at 10–30 residents using the existing benchmark approach, including a baseline comparison with the published 5,000-resident prototype measurement; no device/frame claim from a host benchmark.

## Consequences

The slice gets an explicit schedulable core instead of silently extending the toy model. Activity/location semantics are inspectable, integer-time based and compatible with later L1/LOD reconciliation. Two event tags and a new domain module add code/codec validation obligations, while preserving old save/wire meanings. Real calendar alignment, L1 utility/pathfinding, camera LOD, interventions, rendering and persistence stay in their existing issues.

Noah accepted this docs-only contract under his 2026-10-19 standing approval, recorded on #77 and PR #86, after independent review. No new domain/event code ships here; #68 remains open for separate implementation and its required evidence. Revisit when the real-calendar phase is integrated, L1 overrides need reconciliation, or measurement shows the chosen queue/step approach exceeds budget.
