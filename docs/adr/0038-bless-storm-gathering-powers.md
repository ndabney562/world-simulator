# ADR 0038: Bless, storm and gathering as reversible slice powers (GP-002)

- Status: Proposed
- Date: 2026-10-30
- Decided by: Product Owner (pending); proposed by Claude for issue #125
- Issue: #125 (GP-002)
- Supersedes: none; extends ADR 0021's effect vocabulary and ADR 0034's episode, override and consequence machinery

## Context

The slice has one god power, the ADR 0034 illness curse, driven through slice operation tag 5 and shown by the GP-001 HUD (#124). Issue #125 asks for three more reversible powers, no death: bless, storm and gathering. Each must be a structured core command (Rust owns state, AGENTS.md guardrails), record a root intervention, and surface its consequences through the existing `ws_slice_consequences` query so the GP-001 feed shows them without a new UI mechanism.

Constraints that shape the design:

- ADR 0034 keeps one derived consequence index per root with at most 16 produced event IDs, and producers must fail before commit rather than exceed it. A block-wide power must therefore record a bounded number of consequence events per root.
- ADR 0036 save format 4 stores the illness episode and override state. New per-resident or block-wide state that outlives a tick must be saved, hashed and replayed (Tier 2, [25-determinism-policy](../architecture/25-determinism-policy.md)).
- The 64-byte `WsSliceOperation` ABI carries a tag and eight words; the inspection and consequence records keep their schemas.
- The mobile budget: no per-entity FFI calls; the HUD reads consequences at most once a second per active root (GP-001).

## Options considered

1. **Three new slice operations, each reusing ADR 0034's episode/override machinery and the consequence index.** Smallest coherent change: one new `Operation` variant per power in `ws-sim`, new payload tags in `ws-events`, one slice-state addition for the storm, a save-format bump. Costs: five to seven new payload tags, format 5.
2. **A generic "scripted override" effect with parameters.** Fewer tags, but moves semantics into data and weakens the closed, versioned vocabulary ADR 0021 chose. Rejected.
3. **HUD-side simulation of storm and gathering (visual only).** Violates "Rust owns authoritative state" and leaves nothing to replay. Rejected.

Option 1 is chosen.

## Decision

**1. Operations.** `ws-sim::slice::Operation` gains three variants; the FFI maps slice operation tags 6, 7 and 8 to them. Unused words must be zero, as today.

| Tag | Operation | Words | Validation (semantic rejections, recorded in the operation journal, no state effect) |
|---|---|---|---|
| 6 | `Bless { resident }` | 0: resident | living resident at L1 or L2 |
| 7 | `Storm { duration_us }` | 0: duration | duration grid-aligned, one tick through one hour; rejected while a storm is active |
| 8 | `Gather { building, duration_us }` | 0: building, 1: duration | featured residential building; duration grid-aligned, 10 minutes through one hour; rejected while a gathering is active at any building |

Each applies at the next committed tick boundary under the existing serialization gate and records a root event with `Origin::Command(sequence)`, exactly as tag 5 does.

**2. Bless.** If the resident has an active illness episode: the episode ends at this boundary, health is restored to 100, and the existing `Recovered` (12) event is produced with `CausedBy/Direct` to the bless root and `ContributedTo` to the original `LifeIllness` (5). The pending illness-produced return-home arrival stays pending, as ADR 0034 requires. If the resident is healthy: all four need deficits (rest, hunger, social, duty; ADR 0028) are set to zero and a `NeedsRelieved` event is produced. A blessed resident already at zero deficits and healthy is accepted with `affected = 0` and only the root, mirroring the no-effect curse.

**3. Storm.** Slice state gains `storm: Option<StormEpisode { root, start, end }>`. While a storm is active, a scheduled departure from any featured building is deferred: the resident keeps the current activity and location, and departs at the storm's end if the timetable row is still due, otherwise resumes the next row. Residents already in transit continue to their destination (no teleporting, arrivals unchanged). Each deferred departure records a `StayedInside { resident, building, activity }` event in history in canonical (ascending resident ID) order, so building inspection shows it. The consequence index for the storm root holds the root, one `StormKeptInside { count }` aggregate per storm, and `StormEnded`, three entries, well inside the 16-ID cap. Missed-duty rules are untouched: a deferred departure is not a missed duty.

**4. Gathering.** Up to eight residents are chosen deterministically: residents at L1 whose current location is a featured building within 150 m of the target, not ill, not already under an override, in ascending ID order. Each receives a `visit` override (ADR 0034 override kind 2) to the target building for `duration_us`, then the existing return-home path. Events: `GatheringCalled { building, duration, attendees }` root, one `JoinedGathering { resident, building }` per attendee on arrival, reuse of `ReturnedHome` (6) on return, and `GatheringEnded { building, attendees }`. Worst case is 1 + 8 + 8 + 1 = 18 entries, so the index records the root, the joins, and the end, and the return-home events link to the gathering by cause without occupying an index slot. Zero eligible residents is accepted with `affected = 0`.

**5. Events.** New `Payload` tags, appended after 13: 14 `BlessApplied { resident, sequence, affected }`, 15 `NeedsRelieved { resident }`, 16 `StormApplied { sequence, duration_us }`, 17 `StayedInside { resident, building, activity }`, 18 `StormKeptInside { count }`, 19 `StormEnded`, 20 `GatheringCalled { building, sequence, duration_us, attendees }`, 21 `JoinedGathering { resident, building }`, 22 `GatheringEnded { building, attendees }`. Importance and system numbers follow ADR 0034's scheme. Existing tags, schemas, goldens and the inspection record are unchanged; `InspectionDecoder.ValidateIllnessEvent` grows one branch per new tag.

**6. Persistence and determinism.** Save format 5 adds the storm episode, the gathering episode (building, root, end, attendee list) and the bless root to the slice-state envelope; the loader reads formats 4 and 5, writers write 5; an active storm or gathering in a format-4 file is impossible, so no migration data is needed. All new state enters the canonical hash. Replay tests cover each power from a fixed seed, in the same and in a separate process; goldens are added for each power and the existing goldens stay byte-identical when no new power is used.

**7. Unity.** `NativeSlice` gains `Bless(id)`, `Storm(durationUs)` and `Gather(buildingId, durationUs)` through the existing operation path; the consequence query is unchanged. The GP-001 power bar gains three buttons: Bless and Gathering are select-then-tap (resident and building respectively); Storm casts immediately for one hour. `HudText.Feed` gains one line per new tag. No new UI mechanism.

## Consequences

- Easier: GP-003 (#126) reactions and GP-004 (#127) goals read the same consequence index; the feed needs only text.
- Harder: the slice-state envelope and format bump touch `ws-persist` codecs and their round-trip tests; the storm's deferral rule touches the ws-life timetable step and needs careful golden coverage.
- Affected: `ws-events` (tags 14 to 22), `ws-life` (storm deferral, gathering visit), `ws-sim` (operations, episodes, index), `ws-persist` (format 5), `ws-ffi` (tags 6 to 8, validation), Unity `Bridge`, `UI`, EditMode tests, `docs/architecture/13-god-interventions.md`, this ADR's index entry.
- Revisit if the 16-ID consequence cap proves too small for block-wide powers (an index with a `more` flag would be the next step), or if players need storms longer than one hour.

## Open questions for the Product Owner

1. Bless on a healthy resident: relieve needs (as proposed), or make bless illness-only and keep the HUD simpler?
2. Gathering size: eight attendees within 150 m, or the whole building's residents?
3. Save format 5 is required for storm and gathering state. Confirm this is acceptable under ADR 0036 (format changes are a recorded decision).
