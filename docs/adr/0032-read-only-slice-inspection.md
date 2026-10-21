# ADR 0032: read-only real-slice entity inspection

- Status: Accepted
- Date: 2026-10-21
- Decided by: Noah under the named technical delegation on #77, after independent review; proposed by Codex
- Issue: #96 (design), #73 (runtime)
- Supersedes: none; extends ADRs0018/0019/0031

## Context

[VS-009](https://github.com/ndabney562/world-simulator/issues/73) requires tapping a resident or building to inspect identity, schedule, activity, needs and recent factual events. ADR0031 exposes a separate real-slice handle, immutable geometry and coherent dynamic rendering, but no inspection query. Its runtime merged through PR#95 at main c8a87d752ac570657d423fb6af93a08961f15d6d after clean independent review and all five green checks; device acceptance stays open. This decision changes only documentation; runtime implementation follows its separate merged design.

The core already retains synthetic identity, immutable weekday/weekend timetables, logical location, integer need deficits and exact building counts. EventStore indexes actors, but its existing query collects all matching events before returning. Calling it and truncating afterwards would produce unbounded temporary storage in a supposedly bounded inspector. Source-context footprints have BINs and zero EntityIds; they are not simulated buildings. A query must preserve these distinctions and must not promote a resident merely to inspect them.

## Options considered

1. Reconstruct identity, schedules and events in Unity. Duplicates authority and cannot establish agreement with a core dump.
2. Fetch every entity every frame. Unnecessary native traffic and a second snapshot surface; violates the tap-driven scope.
3. Query one selected entity on demand, returning a bounded complete record at one committed boundary. Chosen. Presentation picking resolves existing IDs; Rust supplies all domain information.

## Decision

### Scope and authority

Add a pure `SliceSimulation::inspect(EntityId)` composition in ws-sim, using immutable ws-world identity/source, ws-life schedule/behavior and ws-events actor-index data. Domain crates do not gain dependencies on presentation, FFI, or each other. No new crate, dependency, data source, event type, mutable component, save format, command, intervention or provider is introduced. Rust owns the values; Unity formats the result.

Support exactly the retained residents and four featured simulated building EntityIds. Unknown, zero, household, workplace and context BIN inputs return an explicit invalid-target status, without a fabricated record. A featured building's BIN is metadata, never its query key. Display context as visual source context when tapped, without a native entity query or invented occupants.

A resident record contains ID, synthetic given/family names, baseline age/role, household/home and optional workplace ID; detail level, logical activity/location and held micro-action; four integer need deficits in existing core order with maximum100000; model schedule epoch, household jitter and both existing weekday/weekend tables. Use existing values, with no regenerated identity or RNG draws. Show deficits as deficits rather than relabeling them as satisfaction. Current activity and held action are different quantities and receive distinct labels.

A building record contains ID, source BIN/BBL/address/class/featured role, above-ground height, residential-unit count and exact existing node mode/occupancy/governed/activity counts. No fictional building needs or personal timetable. Identity fields refer to synthetic residents and open-data buildings, not actual occupants. Current occupants and governed residents must not be confused.

Logical location uses existing AtBuilding/OffMap/InTransit semantics and endpoint IDs/exit indices. This query does not expose a new reconstructed fine position: the renderer already carries actual local fine geometry. The inspector must label L2/off-map honestly and must never create a fine actor or move the camera as a side effect. The model timetable retains elapsed model-week semantics and household jitter under ADR0030; it is not advertised as a civil DST-adjusted appointment calendar. Raw minute rows avoid narrowing the schedule engine's i128 absolute transit endpoints to i64.

### Recent factual events

Add a read-only bounded actor query in ws-events, preserving the existing query API. Traverse the existing actor index backwards; return at most16 matching records with time at or before the inspected committed time. Deduplicate any multi-role actor occurrence; order newest first by `(SimTime, EventId)` descending, with an explicit `more_events` flag established by observing a seventeenth distinct match. No query-time scan/collection of the whole event log is permitted. Tests must verify that the actor index's order agrees with time/EventId ordering, or add a rebuildable time-ordered actor index if needed. Index construction and query results must not enter canonical state or drive rules.

Events are associated only through explicit actor membership. Do not infer events from occupancy, household membership, proximity or a matching timestamp, and do not invent a creation event for each person. An empty list means no recorded matching facts. A building may have few actor events even while occupied. Keep the full existing log and all its records; the16-row display bound does not impose retention or discard facts.

Initial slice actor-event tags are7 (`agent.schedule_changed`) and9 (`sim.slice_lod_changed`). Export their actual typed payloads: tag7 resident/destination/previous/next; tag9 building/from/to/demoted/promoted. Export EventId, committed event tick, SimTime, ordinal and schema as well. Unknown future slice payloads fail this versioned query explicitly before publication, rather than disappearing or being relabeled. #74 must extend the reviewed query/event vocabulary for illness and confidence-class consequences; VS-009 does not claim a causal traversal UI or inferred causes.

### ABI and bounded wire format

Use global ABI6 only if still unused when runtime implementation starts; otherwise reconcile the version before code. ADR0026's optional provider reservation remains conditional. Migrate existing success ABI version fields/preflight/fixture selection together while retaining all prior layouts, toy command envelope2, slice operations, fixtures, consumer sources and behavioral tests. Old clients refuse the new library before new symbols. This proposal authorizes no unrelated test, golden or tolerance change.

Add `ws_slice_inspect(handle, entity_id, buffer, capacity, written)` to the existing exclusively serialized slice handle. Follow ADR0031's checked probe/fill, panic catching, poison and diagnostic conventions. Unknown target uses the existing invalid-input status; diagnostics explain it. Invalid arguments/version/target, allocation failures and caught panics publish no partial bytes. A valid short-buffer probe reports the exact required length without writing payload; other errors set written to zero. Caller pointers remain borrowed for the call only. All input/output alias, alignment and lifetime requirements follow the existing fill exports.

The output is explicit little-endian bytes, not native struct layout. Header96:

| Offset | Type | Meaning |
|---:|---|---|
|0/4|u32/u32|global ABI / header size96|
|8/16/24/32|u64/i64/u64/u64|tick / SimTime / canonical state hash / dataset version|
|40|u64|selected EntityId|
|48/52|u32/u32|inspection schema1 / entity kind1 resident or2 building|
|56/60|u32/u32|total exact byte length / flags bit0 more_events|
|64/68/72/76|4×u32|identity section bytes / state section bytes / schedule row count / event row count|
|80/84/88/92|4×u32|schedule stride48 / event stride80 / committed scale0,1,10,100 / zero|

Follow with identity section, state section, schedule rows, event rows in that order. Maximum complete output is8192 bytes. All arithmetic is checked; no padding, trailing bytes, unknown bits, zero ID, invalid tags, excessive count or invalid UTF-8 is accepted. Strings are length-prefixed UTF-8, maximum128 bytes each, with no embedded NUL. Reject oversized source/name text before publishing; never silently truncate identity. Maximum identity section1024 bytes; exactly one kind-specific state section; at most32 schedule rows combined across the two tables and16 events. Verify these caps against every current source row and cohort profile in tests.

Identity section begins with u32 schema1 and u32 section size. Resident then has home/household/workplace-or-zero as3×u64, age/role as2×u32 and two bounded strings (given then family name). Building then has BIN/BBL as2×u64, above-ground height_mm/residential_units as2×u32 and three bounded strings (address, building class, featured role). Each string is u32 byte count followed immediately by its bytes. There is no implicit alignment padding.

Resident state is64 bytes: detail/activity/action/location_tag as4×u32; four deficits as4×u32; location from/to IDs as2×u64; from/to exit indices as2×u32; zero u64. Location tags0 building,1 off-map stationary,2 transit. Stationary uses from=to; building endpoint exit is u32::MAX, off-map endpoint exit is0/1. Transit stores its actual endpoint IDs/exit tags. Entity kind and core endpoint classification resolve building versus workplace. Schedule rows convey duration; no absolute transit endpoints are narrowed. Building state is48 bytes and exactly reuses ADR0031's summary row: ID, mode, occupancy, governed, six activity counts, zero. Its schedule count is zero.

Resident schedule data begins with an additional16-byte prefix included in `state section bytes` (resident state section total80): epoch i64, household jitter i32, schedule rules version u32. Each48-byte schedule row begins with8×u32: table0 weekday/1 weekend, start minute, activity, destination kind0 building/1 off-map, origin kind0 none/1 building/2 off-map, arrival minute or u32::MAX, destination exit or u32::MAX, origin exit or u32::MAX. Destination/origin u64 IDs follow at32/40. Canonical order is weekday rows then weekend rows, ascending minute per table. Origin ID is zero only for none. Validate half-open table order, tags, endpoints and commute arrival consistency against existing schedule invariants.

Each80-byte event row has EventId u64 at0, time i64 at8, tick u64 at16, ordinal u64 at24; payload tag u32 at32, schema u32 at36; five payload words u64 at40–79. Tag7 words are resident, destination, previous activity, next activity, zero. Tag9 words are building, previous mode, next mode, demoted, promoted. All unused words are zero; selected ID must be the appropriate actor of each row. This transport is an inspection projection, not a persistent event codec; it does not change EventId or the underlying event record/schema.

Exact fixtures and C/Rust/C# layout tests guard every offset and cap. Neither a future event tag nor larger schedule/source data may silently exceed these limits; either fail explicitly or extend the contract through review.

### Picking, gate and publication

Unity picks cached immutable footprint triangles for buildings and actual local fine resident capsules using the latest successfully rendered frame's geometry/instance transforms. Include all context meshes as occluders, even though their ID is zero. Use the nearest positive ray hit; deterministic equal-depth ties prefer resident over featured building over context, then ascending stable ID/BIN. Hidden, coarse, off-map or nonexistent capsules cannot be selected. Use a positive finite camera ray and bounded intersection arithmetic. Picking never alters LOD or a core position.

Track a tap separately from a drag: one pointer starts outside the HUD; release within0.35 host seconds, with maximum displacement from its start never exceeding8 logical screen points (scaled by the existing HUD scale), and no intervening second touch, cancel or UI capture. Crossing the displacement limit permanently cancels the tap, even if the pointer returns near its start before release. Any second touch cancels that gesture as a tap even if it later becomes one touch. Orbit/pan/pinch remain available; a drag ending on a building must not open inspection. HUD input blocks camera and picking consistently. Test mouse and touch transitions through a testable gesture state machine, including an outward drag that returns to its start within the time limit.

Execute one query only for an accepted tap on a valid simulated identity. Existing handle lock serializes the complete probe/fill and validation. Core values all belong to the returned tick/time/hash/dataset/scale; no two boundary reads are combined. Reuse persistent8192-byte scratch and bounded managed records; decode/validate into staging, then replace the displayed record atomically. A failed query preserves the prior record and shows a concise error. Replacing/disposing the world clears selection and text; no stale identity/geometry survives recreation. Selecting context clears the prior entity panel and labels context; empty-space taps clear selection.

The panel shows its sampled time/tick and stays a snapshot while simulation advances. Re-tapping refreshes it with one new query. Do not poll the selected resident every frame or silently pause the world. UI formatting/selection may allocate on taps; warmed rendering retains ADR0031's zero-managed-allocation requirement. No text formatting or event query is added to the per-frame render path.

### Verification, acceptance and sequencing

Before runtime: separate architecture-change issue, this docs-only proposal, independent actual-diff review, explicit Accepted metadata under the recorded delegation, then final-head clean review/green CI and merge. Rebase or normally merge #72 engineering integration before the separate #73 runtime begins. Do not mark #72 device acceptance complete as a dependency shortcut.

Required runtime checks:

- Core dumps independently compared with resident/building query fields for every cohort ID and featured building, including weekday/weekend, pre-epoch times, L1/L2, off-map transit, stationary and schedule/need boundaries. Existing rules/goldens remain unchanged.
- Query leaves state hash, events/operations digests, pending host time, clock, membership, cache observations and future replay trajectory unchanged across repeated/interleaved success/probe/error calls. No query creates an inspected-event fact.
- Bounded event query agrees with the existing full query oracle on small fixtures, same-time events, duplicate actor roles, empty/exact16/17+ results, unknown actors and index reconstruction. Measure query temporary bytes and latency on long histories; do not collect all matches before truncation.
- ABI6 refusal/migration, exact complete resident/building fixtures, C consumer and generated-header freshness; malformed/truncated/oversized/text/tag/row/capacity/poison/no-output-on-error tests. Preserve existing consumers and fixture checks.
- Actual pinned Unity Editor tests with the native library for complete decode, malformed-output preservation, gesture cancellation, ray occlusion/ties, featured identity mapping, tap-only query count, paused operation, world replacement and panel/render separation. Test screen-to-core mapping for visible resident and building.
- Actual iPhone15Pro taps of a resident and building, screenshot and matching core-side dump at sampled tick/hash, remain #73 acceptance. Desktop evidence is separate. Signing/device/account actions retain their own human gate. If unavailable, retain unchecked device criteria and a reproducible handoff; never close #73 merely because engineering can merge.

## Consequences

Inspection exposes core truth through one bounded tap-driven query. The panel intentionally holds a labeled sample until re-tapped. Index traversal, wire validation and occlusion-aware picking require dedicated tests; these protect bounded memory, honest associations and identity correctness. #74 extends illness/consequences separately, #75 owns slice persistence, and #76 owns the final device run and stage report. No stage acceptance is established here.

## Product Owner presentation amendment — 2026-10-21

The Product Owner requested that resident inspection show only age, sex, employment, health and alive/dead. This supersedes the resident-facing detail/sample-time display above; it does not change the bounded read-only transport, core query, building panel or snapshot refresh semantics. Keep full native validation and existing data/replay tests. Age and employment are sourced from the existing baseline identity/role. Sex, health and life status are absent from the real-slice core and explicitly display `Not tracked`; do not infer them from names, need deficits or retained-entity presence. The panel hides resident names, IDs, LOD, needs, schedules, events and technical sample metadata. #74 remains the separate illness model; this presentation change does not implement demographic generation, health or mortality. Authorization is recorded on #73 and the PR.
