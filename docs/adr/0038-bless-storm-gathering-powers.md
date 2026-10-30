# ADR 0038: Bless, storm and gathering as reversible slice powers (GP-002)

- Status: Accepted
- Date: 2026-10-30
- Decided by: Claude under the standing delegation (#122, PR #123, merged 2026-10-30), after a fresh-context review recorded on PR #129; proposed by Claude for issue #125; Noah can reverse
- Issue: #125 (GP-002). A feature issue rather than an architecture-change issue, as with ADR 0021 (#11) and ADR 0036 (#75)
- Supersedes: none; extends ADR 0021's effect vocabulary and ADR 0034's episode, override and consequence machinery

## Context

The slice has one god power, the ADR 0034 illness curse, driven through slice operation tag 5 and shown by the GP-001 HUD (#124, PR #128). Issue #125 asks for three more reversible powers, no death: bless, storm and gathering. Each must be a structured core command (Rust owns state, AGENTS.md guardrails), record a root intervention, and surface its consequences through the existing `ws_slice_consequences` query so the GP-001 feed shows them without a new UI mechanism.

Constraints that shape the design:

- ADR 0034 keeps one derived consequence index per root with at most 16 produced event IDs (`MAX_CONSEQUENCES`), and producers must fail before commit rather than exceed it. ADR 0036 does not save that index: `rebuild_consequence_index` re-derives every group on load by walking `root_intervention` cause links. Whatever the live producer indexes, the rebuild must index identically, or a save stops loading.
- ADR 0036 save format 4 stores the illness episode and override state. New per-resident or block-wide state that outlives a tick must be saved, hashed and replayed (Tier 2, [25-determinism-policy](../architecture/25-determinism-policy.md)).
- The 64-byte `WsSliceOperation` ABI carries a tag and eight words. The inspection record (schema 3) validates override kinds 0, 1 and 2 with kind-specific rules; a new override kind needs its own rule.
- The mobile budget: no per-entity FFI calls; the HUD reads consequences at most once a second per active root (GP-001).

## Options considered

1. **Three new slice operations, each reusing ADR 0034's episode, override and consequence machinery.** Smallest coherent change: one new `Operation` variant per power in `ws-sim`, nine new payload tags in `ws-events`, one new override kind, two block-level episodes in slice state, a save-format bump. Costs: format 5 and a schema-3 validator extension.
2. **A generic "scripted override" effect with parameters.** Fewer tags, but moves semantics into data and weakens the closed, versioned vocabulary ADR 0021 chose. Rejected.
3. **HUD-side simulation of storm and gathering (visual only).** Violates "Rust owns authoritative state" and leaves nothing to replay. Rejected.

Option 1 is chosen.

## Decision

**1. Operations.** `ws-sim::slice::Operation` gains three variants; the FFI maps slice operation tags 6, 7 and 8 to them. Unused words must be zero, as today. Malformed framing is an error; the rules below are semantic rejections recorded in the operation journal with no state or event effect.

| Tag | Operation | Words | Semantic validation |
|---|---|---|---|
| 6 | `Bless { resident }` | 0: resident | living resident at L1 or L2 |
| 7 | `Storm { duration_us }` | 0: duration | grid-aligned, one tick through one hour; rejected while a storm or a gathering is active |
| 8 | `Gather { building, duration_us }` | 0: building, 1: duration | featured residential building; grid-aligned, 10 minutes through one hour; rejected while a gathering or a storm is active |

Each applies at the next committed tick boundary under the existing serialization gate and records a root event with `Origin::Command(sequence)`, exactly as tag 5 does. The three root payloads (tags 14, 16 and 20 below) are recognised by `root_intervention` alongside tags 4 and 13.

**2. Bless.** If the resident has an active illness episode: the episode's `end` is set to now, health is restored to 100, and one `Recovered` (12) event is produced with `CausedBy/Direct` to the bless root and `ContributedTo` to the original `LifeIllness` (5). The natural recovery scheduled for the original end does not fire a second `Recovered`. A bless-produced `Recovered` indexes under the bless root only; the illness root's group does not receive it, so an illness consequence view can end without a recovery row and the Unity illness validator tolerates that. The post-recovery home-hold and rejoin follow ADR 0034 unchanged, and the pending illness-produced return-home arrival stays pending. If the resident is healthy: all four need deficits (rest, hunger, social, duty; ADR 0028) are set to zero through the checked core path and one `NeedsRelieved` (15) event is produced. A healthy resident already at zero deficits is accepted with `affected = 0` and only the root. Bless adds no slice state: it lives only in the event history and the consequence index. The inspection record's `own_root` (offset 96) keeps reporting the illness episode's root, unchanged by a bless.

**3. Storm.** Slice state gains one block-level episode `storm { root, start, end }`. The rule is one sentence: while a storm is active, a departure that the baseline timetable would start from a featured building is deferred; nothing else changes. Override legs (illness homebound, neighbour check-in, post-recovery rejoin) are unaffected, residents already in transit continue to their destination, and no new override is created. A deferred resident keeps the current activity, location and action. At the storm's end they depart if the deferred row is still due, otherwise they resume the baseline timetable at its next row. A row skipped this way is not a `MissedDuty`: the storm is an explanation, not an absence, so no missed-duty event is produced and the cap is unaffected. The deferral is applied identically at L1 and L2 (ADR 0021 §4 lock-step) and the L1 ↔ L2 round-trip test covers a storm in progress.

Events: each deferred departure records one `StayedInside` (17) in history in canonical ascending resident-ID order, linked `ContributedTo` the storm root, so building inspection and provenance show it. `StayedInside` is excluded from every consequence index by a producer-defined exclusion table in `ws-sim` that the live producer and `rebuild_consequence_index` both apply. The storm root's group holds exactly three IDs: the root, one `StormKeptInside { count }` (18) at the storm's end, where `count` is the number of `StayedInside` events recorded during the storm (a resident with two deferred rows counts twice; zero is allowed), and `StormEnded` (19).

**4. Gathering.** Slice state gains one block-level episode `gathering { building, root, start, end, attendees }`. Up to seven attendees are chosen deterministically: residents at L1 who are at Home (Activity 5) in a featured building whose integer millimetre position in the block dataset (the source `SurfacePos` is derived from; the f32 value is never used) lies within 150 000 mm (inclusive, integer arithmetic) of the target's, not ill, not under any override, in ascending ID order. Seven keeps the group at 1 + 7 + 7 + 1 = 16, so every linked gathering event indexes uniformly and the rebuilt index equals the live one. Each attendee receives a new override kind 3 `gathering { building, root, end }`: an outbound leg to the target on the existing navigation graph (Activity 1 Commute, Action 5 Travel, from home to the target), a stay at the target until `end` (Activity 5, Action 3 Socialize, location the target building), then the existing homebound leg and rejoin from ADR 0034. Zero eligible residents is accepted with `affected = 0`.

Events: `GatheringCalled` (20) root; one `JoinedGathering` (21) per attendee on arrival, `CausedBy` the root; one `ReturnedHome` (6) per attendee on return, `CausedBy` the root; `GatheringEnded` (22) at `end`. All index under the gathering root. Attendees who never arrive before `end` (a long walk) skip the stay and go home; their `ReturnedHome` still indexes. An illness cast on an attendee mid-gathering ends their attendance under ADR 0034's interruption rule: their homebound leg and `ReturnedHome` belong to the illness root, and the gathering group simply lacks that row. `GatheringEnded.attendees` counts residents who recorded `JoinedGathering`.

**5. Events.** `Payload` gains nine tags, appended after 13. Importance and system numbers follow ADR 0034's scheme. The 80-byte recent-event projection gains one row layout per tag (five words, in order):

| Tag | Payload | Projection words |
|---|---|---|
| 14 | `BlessApplied { resident, sequence, affected }` (root) | resident, sequence, affected, 0, 0 |
| 15 | `NeedsRelieved { resident }` | resident, 0, 0, 0, 0 |
| 16 | `StormApplied { sequence, duration_us }` (root) | sequence, duration, 0, 0, 0 |
| 17 | `StayedInside { resident, building, activity }` | resident, building, activity, 0, 0 |
| 18 | `StormKeptInside { count }` | count, 0, 0, 0, 0 |
| 19 | `StormEnded { sequence }` | sequence, 0, 0, 0, 0 |
| 20 | `GatheringCalled { building, sequence, duration_us, attendees }` (root) | building, sequence, duration, attendees, 0 |
| 21 | `JoinedGathering { resident, building }` | resident, building, 0, 0, 0 |
| 22 | `GatheringEnded { building, attendees }` | building, attendees, 0, 0, 0 |

Consequence header and row-shape rules become per root tag; the ADR 0034 illness rules stay exactly as shipped for root 13, and `ConsequencesDecoder` dispatches on the root's tag:

| Root | `subject` (offset 48) | `severity` (76) | End time (88) | Allowed rows after the root, in ID order |
|---|---|---|---|---|
| 13 illness | resident | 1..10 | episode end | unchanged (ADR 0034) |
| 14 bless | resident | 0 | applied time | one 12 or one 15, or none when `affected = 0` |
| 16 storm | 0 (no subject) | 0 | storm `end` | one 18, then one 19 |
| 20 gathering | building (featured) | 0 | gathering `end` | up to seven 21, up to seven 6, one 22; repeats allowed for 21 and 6 |

In the core, `Group.subject` becomes `Option<EntityId>`, written as 0 on the wire for a storm; `ws-ffi` accepts 0 only for a storm root. The global `ABI_VERSION` advances 8 → 9 so an old client is refused by version, not by a decode failure; fixtures and consumer sources for 8 are preserved as ADR 0034 did for 7. The inspection record keeps schema 3 and its 96-byte header; its validator accepts override kind 3 with the rule in §4 (kind 3 is the only override allowed away from home), and `InspectionDecoder.ValidateIllnessEvent` grows one branch per new tag. Existing tags, layouts and goldens are unchanged.

**6. Persistence and determinism.** Save format 5 extends the slice-state envelope with two tables, `storm` (root, start, end) and `gathering` (building, root, start, end, then up to seven attendee rows of resident ID and override phase), and widens the per-resident override kind range to 3; the envelope version advances with the format. The loader reads formats 4 and 5, writers write 5; a format-4 file cannot hold an active storm or gathering, so no migration data is needed. All new state enters the canonical hash. Replay tests cover each power from a fixed seed, in the same and in a separate process; goldens are added for each power and the existing goldens stay byte-identical when no new power is used. One test per power saves after the episode completes, reloads, and asserts the rebuilt consequence index equals the live one.

**7. Unity.** `NativeSlice` gains `Bless(id)`, `Storm(durationUs)` and `Gather(buildingId, durationUs)` through the existing operation path, and a root-only read `Consequences(root)` that calls `ws_slice_consequences` without a paired resident inspection, for roots whose subject is not a resident. `SliceDemo.Consequences` and the GP-001 watch carry the root's tag so the HUD polls the illness path for curses and the root-only path otherwise. The power bar gains three buttons: Bless and Gathering are select-then-tap (resident and building respectively); Storm casts immediately for one hour. `HudText.Feed` gains one line per new tag. Polling cadence and the feed are unchanged.

## Consequences

- Easier: GP-003 (#126) reactions and GP-004 (#127) goals read the same consequence index; the feed needs only text.
- Harder: the slice-state envelope and format bump touch `ws-persist` codecs and their round-trip tests; the storm's deferral rule touches the ws-life timetable step and needs golden coverage; override kind 3 and the per-root consequence rules touch the validators on both sides of the FFI, and the ABI advances to 9.
- Ordering: the Unity part depends on the GP-001 HUD, so implementation lands after PR #128 merges; the core part can start first.
- Affected: `ws-events` (tags 14 to 22, root recognition), `ws-life` (storm deferral, gathering override), `ws-sim` (operations, episodes, index exclusion table), `ws-persist` (format 5), `ws-ffi` (tags 6 to 8, validation), Unity `Bridge`, `UI`, EditMode tests, `docs/architecture/13-god-interventions.md`, `docs/architecture/CHANGELOG.md`, the ADR index.
- Revisit if seven attendees feel too few in play (an index with a `more` flag is the next step), or if players need storms longer than one hour.

## Decisions taken under the standing delegation

1. Bless on a healthy resident relieves the four need deficits, because issue #125 names both cures and lifted needs and the HUD treats both as one button.
2. Gathering takes up to seven attendees within 150 000 mm, in ascending ID order, so the consequence index stays within the 16-ID cap on both the live and the rebuilt path.
3. Save format 5 is accepted: storm and gathering episodes must survive a save, and ADR 0036's loader already reads earlier formats.
4. A storm-skipped duty row is not a missed duty (review question on PR #129).
