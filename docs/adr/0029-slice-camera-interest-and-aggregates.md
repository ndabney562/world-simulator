# ADR 0029: real-slice camera interest, building aggregates and observation continuity

- Status: Proposed
- Date: 2026-10-20
- Decided by: Noah under the technical workstream delegation on #77, after independent review; proposed by Codex
- Issue: #70 (VS-006 design phase)
- Supersedes: none; details real-slice interest/materialization beyond ADRs0017 and0028

## Context

[#70](https://github.com/ndabney562/world-simulator/issues/70) requires one coarse camera-interest input, L1/L2 selection, L3 building counts, observation continuity and conservation under repeated zooming. ADR0028 retains exact needs, counters and reconstructable paths at either resident level; it deliberately leaves camera policy and production LOD facts to this issue. Architecture05/12 require deterministic transactional selection with hysteresis and bounded work, rather than rendering directly setting per-person levels.

The vertical slice has four featured buildings and10–30 synthetic residents. Its synthetic navigation plane is explicitly not ENU. There is no real-slice Unity rendering adapter yet; #72 owns that renderer/bridge integration. This decision defines a coarse typed host command and bulk Rust snapshot contract that that adapter will consume. It introduces no temporary per-entity native calls and does not redefine the existing toy ABI. #70 must demonstrate the complete core command/transition/snapshot/replay path; #72 must connect the actual Unity camera through the coarse boundary before the end-to-end slice is claimed complete.

This PR is docs only. Implementation follows the merged accepted contract separately. Existing event/save/FFI formats and runtime behavior are unchanged by this proposal.

## Options considered

1. Let the renderer select individual residents and regenerate needs on zoom-in. Rejected: violates ownership, batching and conservation.
2. Discard all resident identity/need records at L3 and introduce a new cohort model. Rejected for this issue: discards the exact retained-state contract and adds excluded larger-domain aggregation.
3. Use deterministic building policy, retain canonical resident anchors, expose L3 occupancy/activity summaries, and reconstruct fine positions from the same model. Chosen: directly testable counts and continuity without new domains or unrecorded simulation inputs.

## Decision

### Ownership and boundary

Extend the existing real-slice `ws-life` behavior and `ws-sim::slice` orchestrator. Rust owns the controller, selected building modes and all population/need state. No new crate or dependency is required. A structured `CameraInterest { center_mm, radius_mm }` operation supplies one interest region for the complete slice, without a list of residents. `center_mm` is an integer point in ADR0028's approximate navigation plane, not ENU/ECEF; the future renderer must explicitly convert its presentation camera through the navigation adapter. Accept center coordinates only in±1,000,000,000mm and radius in0..=1,000,000,000mm, using wide squared-distance arithmetic. Radius0 means overview/all buildings at L3; its center is still validated.

The operation validates and stores the latest camera input at a serialized boundary, with an explicit recorded success/rejection outcome. It does not advance simulation time, run extra utility decisions or immediately publish a partial transition. Repeated identical inputs are accepted and journalled without resetting dwell times. Invalid inputs leave state/events unchanged. Inputs, controller policy version1 and canonical per-building mode/change time enter the slice state hash; request order and outcomes enter the separate operation digest. Multiple requests before an update use the last accepted input, while the journal retains each request for replay.

The typed Rust API and bulk views are the host integration contract for this core issue. No FFI, byte encoding or Unity package change is smuggled into it. If #72 changes the native/snapshot boundary to carry these values, it must first specify and review that format in a docs-only ADR, preserving coarse commands-in/snapshots-out. No claim that the existing toy bridge already transports a real-slice camera is allowed.

### Building modes, interest and hysteresis

The four featured buildings are the only policy nodes, in ascending entity-ID order. A node mode is L1 Active, L2 Background or L3 Aggregate. Initially each is L2, camera input is absent and no controller selection runs until a camera input has been accepted. This preserves ADR0028's manually exercisable headless behavior. Once camera control is active, it owns automatic selection; an explicit manual detail request remains a diagnostic operation and is reconciled at the next physical tick. It cannot change identity, needs or the node's policy mode.

Use each building's registered navigation portal proxy as the integer interest position. Let `d²` be squared planar distance to camera center and `r` the positive requested radius. Evaluate policy at200-ms model-time boundaries aligned to `schedule_epoch`, after advancing the common behavior to that boundary. No wall-clock time, screen resolution, renderer band, frame rate or ambient RNG participates.

Without hysteresis, `d <= r` selects L1, `r < d <= 2r` selects L2 and farther selects L3. Implement thresholds by squared comparisons without square roots. For an existing L1 node, retain L1 through `d <= r+20,000mm`; for an existing L2 node, retain L2 through `d <= 2r+20,000mm` unless it qualifies for L1. L3 promotes using the unexpanded entry thresholds. A node normally remains in its mode for at least1 simulated second since its last actual mode change; unchanged requests never restart that timer. Initial mode change time is creation time. Integer differences use wide checked arithmetic, including negative/extreme epochs.

Overview radius0 selects L3 and time scales above10× cap the finest automatic mode at L2. These explicit coarsening limits apply at the next policy update even inside dwell time; promotion after the limit clears observes normal dwell. This prevents retaining fine state indefinitely after overview/warp while making the exception replayable. Scale0 pauses model time, so stored camera input waits for a subsequent physical update. Dwell/update scheduling and tests must distinguish accepted input time from actual selection time.

All four nodes may transition in one prepared transaction. At this bounded slice size, the deterministic work budget is the complete four-node population, not a wall-clock cutoff that changes replay results. Measure the worst all-node transition below the issue's1ms target on the local host, report actual workload/intervals, and retain actual device verification for #76. If the bound fails, revise through review rather than hiding it or changing acceptance criteria.

### Resident membership and L3 summaries

Resident identity remains stored once in ADR0028's canonical ordered records. Its sole resident detail is L1 or L2; a building's L3 mode is an aggregate representation over retained coarse records, not a third duplicate resident store. This is the bounded real-slice refinement of architecture05's L3 representation: no fine components are exposed at L3, but known identities/need anchors are retained for exact conservation and later inspection. No household budgets, economy, building condition or cohort disaggregation is added.

The governing building is the current local logical destination: `AtBuilding(b)` uses b, and a local-bound commute uses its destination building. For an off-map-bound commute or off-map stationary resident, use the resident's immutable home building for policy accounting. Automatic L1 requires both an L1 governing node and a current local fine projection. An outbound resident becomes L2 upon leaving the local segment; a returning resident can become L1 when its final local segment begins. Off-map people always retain identity and needs. Reconcile this resident membership at every physical tick against the latest node modes, including ticks between controller updates and schedule/gateway crossings. No extra utility draw occurs.

Expose one bulk immutable view containing all four nodes/modes, resident views and block totals. Every building row contains actual current logical occupancy and six activity counts in the existing Activity tag order. `AtBuilding(b)` contributes once to that building and exactly one activity bin; commuters and off-map occupants contribute to separate block totals, never simultaneously to a building. Include population count, local stationary count, in-transit count and off-map stationary count so their sum is exactly population. Home membership and governing-node membership are separate from physical occupancy and must be named accordingly. A fine point waiting at a portal during a commute is still logically in transit until the existing schedule boundary.

Aggregate queries are pure derivations from the same committed individual answers. Any acceleration cache is rebuildable and excluded from hashing. Before each tick publishes, validate sorted unique complete identities, valid one-level membership, known governing/occupied buildings, bounded needs and exact population/count partitions. Derive these checks from the planned post-tick state, not stale pre-tick snapshots. Independent tests must calculate occupancy/activity counts by a separate resident scan at every physical tick of a full week. Existing schedule facts retain ADR0027's exact contract.

### Observation window and conservation

Keep a bounded per-resident recent-observation cache for120 simulated seconds after automatic demotion from L1. Record observation time and the exact complete `FineLocation` union at that boundary, in resident-ID order, with no more than one entry per known resident. This includes off-map transit/stationary variants: an outbound gateway crossing can demote a resident exactly when its current projection first becomes off-map, so the entry must retain that actual variant and timestamp rather than invent a local point. Model time is required for same-input replay; wall-clock expiry would affect phone/host pacing differently. An entry is a hit through elapsed120 seconds and expires only when elapsed exceeds120 seconds, using wide time differences. Expired entries may be evicted lazily before the next view/transition. Cache allocation/validation occurs during preparation, never after canonical publication starts.

The cache is derived: ADR0028 already retains all needs/counters and reconstructs exact paths. A hit reconstructs the current view by advancing that same deterministic model from the observation boundary; it never restores a stale frozen position, changes a need anchor, triggers a new decision, or reads event history to drive behavior. At zero elapsed time it must equal the cached view exactly; at elapsed time it must equal continuous L1 advancement. A miss uses the current schedule/path projection with the same retained anchors. Cache hit/miss/expiry therefore cannot change canonical state, factual history, utility draws or the resulting fine view. Cache capacity/order/rebuild is excluded from canonical hashes. Tests must demonstrate hits, misses, expiry and explicit cache dropping/rebuilding all preserve these results.

Level changes conserve IDs, population, home membership, needs, actions/counters, timetable deadlines and positions exactly at a common boundary. Hashes intentionally include camera policy/node mode/level membership; compare like-for-like controller state or compare the conserved domain quantities explicitly, rather than expecting a zoom command itself to leave the whole state hash unchanged. A1000-cycle randomized camera stress test must include partial/overlapping interests, overview, local/off-map journeys, dwell/update boundaries and elapsed time. It must check unique complete membership and independent aggregate counts after each operation/tick.

### Facts, atomicity and compatibility

Add a new fixed event payload, tag9/version1, `SliceLodChanged { building: EntityId, from: u8, to: u8, demoted: u32, promoted: u32 }`, named `sim.slice_lod_changed`, importance16384. Node modes use tags1/2/3. This is separate from the toy `LodChanged` tag2, whose1/2-only validation/encoding stays unchanged. Emit one fact per building in ascending ID order when its mode actually changes or its automatic resident membership changes; equal from/to is permitted only with a nonzero demotion/promotion count. Counts describe residents actually changed under that governing building in this transaction, not population totals or cache activity. Initial construction emits no false LOD change. Idempotent camera input and cache hit/expiry emit no fact.

The new fact uses producer/system70, destination LOD equal to the node's new mode, system origin70, the building as Subject, no fabricated causal links and no unverified geodetic location. An input is a recorded control request, not an illness intervention. Schedule system60 facts precede system70 facts in the same event-bearing physical tick. Event IDs/ordinals/digests remain independent of host batch size. Validate known building, mode tags, actual change/count constraints and participants before publication.

Extend the existing in-memory/tape event vocabulary and tests for tag9. Preserve all old tags, field order, hashes, schema fixtures and toy behavior. Legacy save formats1/2 must explicitly reject this slice-only fact, as they already reject tags7/8; do not silently expand old snapshot vocabularies. ADR0025's later slice save implementation must pin controller policy/camera input/node modes/change times alongside retained behavior anchors; derived aggregates/cache rebuild after load. No save format or encoder implementation is part of #70.

A physical tick prepares common needs, controller update/membership/cache, independent invariants and the complete ordered fact vector before committing any canonical state, facts or clock. Every fallible allocation/validation happens first; publication is no-fail. A failed tick leaves its preceding committed state, cache semantics and history intact; a batch reports the already committed prefix. Extend replay for camera requests, rejection outcomes, scale changes and planned controller/domain/history failures, preserving ADR0028's terminal failure and prefix-stop contracts. No component reads factual history to make domain decisions.

## Required implementation evidence

- Exact integer threshold/hysteresis/dwell/update tests, negative/extreme time and coordinate inputs, overview/warp limits, idempotent requests and rejected invalid inputs without mutation.
- At least1000 randomized zoom cycles across multiple seeds with unique complete identities, exact conserved quantities, local/off-map crossings, model-time cache hit/expiry/miss and independent count checks. Explicitly check an outbound gateway demotion caches the actual off-map variant at its real boundary, then preserves projection on a hit, at the inclusive120-second boundary and after expiry.
- Every physical tick of a simulated week compares all building occupancy/activity bins and block totals with an independent individual aggregation.
- Separate-process replay of camera/scale/host-step scripts with per-boundary state/event/operation checkpoints; single-step versus batched equivalence; atomic domain/controller/history fault injection and committed-prefix replay.
- New event validation/codec/digest fixtures, explicit old-save rejection and unchanged old fixtures/tests. Cache dropping/rebuild produces equal canonical hashes, history and views.
- All existing local/hosted checks and isolated controller-transition/bulk-view measurements, with accurate boundaries and no invented Unity/device result.

## Consequences

Camera input drives conservative simulation policy through one command, while consumers receive counts and people from the same committed model. The small complete transaction is easy to measure and replay. Retaining identities/need anchors at L3 uses more memory than a future cohort model but avoids lossy regeneration in the current slice. The synthetic coordinate plane and model-time cache must remain explicit to front-end adapters.

Revisit for a larger population/node hierarchy, true cohort/household aggregation, camera/focus/intervention interest composition, thermal budgets, schedule-independent movement or a native snapshot format. #71–#76 retain calendar, Unity rendering/inspection, illness, whole-slice persistence and real-device/stage-exit responsibilities. Acceptance does not waive any of those gates.
