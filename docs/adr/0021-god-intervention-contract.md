# ADR 0021: The first structured god intervention (TP-010)

- Status: Accepted
- Date: 2026-10-17
- Decided by: Claude, under Noah's standing merge authority for ADR PRs (2026-10-17); product scope is fixed by issue #11
- Issue: #11 (TP-010; the ADR PR lands first)
- Supersedes: none

## Context
[13-god-interventions](../architecture/13-god-interventions.md) §14.2–14.8 define an intervention as data: target, effect, timing, provenance. ADR 0019 gives the command journal and World Events; ADR 0017 gives the L1 ↔ L2 round trip; ADR 0020 gives the save format. TP-010 adds the first real command on top of them: `afflict(illness, severity)`. The choices below are the ones the issue leaves open and that later effects will inherit.

## Decision

**1. Command.** `Action::Intervene(Intervention)` joins `NoOp`, `SetTimeScale`, `SetNodeLod` (wire opcode 3). `Intervention { target, effect, timing }`; provenance is the journal entry itself (sequence, submit tick, issuer), so no extra field is stored.
- `Target::Entity(EntityId)` or `Target::ResidentsOf(building EntityId)`.
- `Effect::Afflict { illness: IllnessKind, severity: u8 }`, a closed, versioned enum (`EFFECT_VOCABULARY_VERSION = 1`). `IllnessKind` has one member, `Fever` (0). Severity is 1..=10.
- `Timing::Now` or `Timing::At(SimTime)`. `At(T)` with `T` not later than the apply boundary is applied now; it is stored as a pending intervention in the journal path of ADR 0019 (the command waits in the pending queue until its tick, as `SetTimeScale` already does). No new scheduler.

**2. Validation (at apply time, at the tick boundary).** Rejected with a recorded reason, changing nothing: unknown entity or building (`UnknownTarget`), a target resident whose node is not at L1 or L2 (`UnsupportedLevel`, since there is no aggregate equivalent), severity outside 1..=10 (`SeverityOutOfRange`), unknown illness kind (`UnknownEffect`, only reachable from decoded bytes). A building with no residents is accepted and affects zero agents (the root event is still emitted). Targets resolve in canonical (ascending ID) order, then the whole intervention is all-or-nothing: if any resident fails validation, nothing is applied.

**3. State.** One `illness` column in `ToyWorld`, keyed by resident ID: `Illness { kind, severity, root: EventId }` where `root` is the `life.illness` event that explains it. It is independent of the L1 and L2 stores, so demote, promote, and save/load carry it by construction; the issue's "equivalent field at L2" is this single column read by both levels. It is included in the state hash and saved. Re-afflicting an already ill agent is a no-op for that agent (no second `life.illness`), recorded in the root event's affected count.

**4. Behaviour (same rule at both levels, so the LOD round trip stays exact).** An ill agent does not choose a new target. When its wait would end:
- away from home: it walks home (no target draw, so no `targets_drawn` increment) and emits `agent.returned_home` on arrival;
- at home: it keeps waiting (`wait_steps` is refilled / L2 `depart` is pushed by `(wait_steps+1)·Δ`).
An agent mid-walk finishes its current leg first. There is no recovery in v1 (a later effect or rule adds it). `ids::next_target` is not called for ill agents, so counter-based streams stay aligned.

**5. Events** (payload tags 4–6, `ws-events`; existing tags unchanged).
- `intervention.applied` (root, `Origin::Command(sequence)`), payload: effect, severity, affected count.
- `life.illness` per newly afflicted agent: `CausedBy` the root, target `Local(0)`-style candidate cause as for `lod.changed` in ADR 0019, actor = the agent.
- `agent.returned_home`: emitted on the arrival that completes a return-home leg of an ill agent, with `ContributedTo` the agent's `life.illness` (`root`) and `Confidence::Certain`. Its cause is the arrival itself, which already carries no event; the illness is a contribution, not the sole cause, matching 24 §24.6.
Event IDs are predicted from `EventStore::counters()` so `Illness.root` can be set before `prepare/commit`, within one tick boundary.

**6. Causal query.** `ws_events::root_cause(&EventStore, EventId) -> Option<EventId>`, following `CausedBy` (and `ContributedTo` when no `CausedBy` exists) to a root; `ws_sim::World::why_ill(EntityId) -> Option<WhyIll>` returns the root intervention event, its `Origin::Command` sequence and the journal entry. Pure reads.

**7. Save format.** Format version 2 adds table `illness` (entity ID → kind, severity, root) and journal action tag 3. The loader reads versions 1 and 2: a v1 file has no `illness` table and loads as an empty column; the v1 fixture must keep loading. Writers always write 2. The header records `EFFECT_VOCABULARY_VERSION`. Loading validates that every `root` names an existing `life.illness` event for that agent.

**8. Cost.** Apply is proportional to the targets: building residents come from a derived home → residents index, the new rows merge into the sorted `illness` column, one event per target. No full-world scan. Per-step cost for ill agents is a column lookup at wait expiry only. The PR reports the 5,000-agent measurement.

**Implementation notes (TP-010).** Recorded where the code made a detail concrete; none changes a decision above.
- Wire: opcode 3 carries 36 payload bytes; severity is a full word on the wire, so an out-of-range value is a journaled `SeverityOutOfRange`, not a framing error; an unknown illness kind is `Malformed` (so `UnknownEffect` is not a rejection). `Timing::Now` must carry a zero time.
- `Timing::At(T)` with a future `T` is journaled as applied at once (outcome `Ok`, effect 0) and held in `World::scheduled`; it takes effect at the end of the first tick whose time is at least `T`, at that boundary's time (the next tick starts from it). The waiting set is part of the queue digest and of a save (`scheduled` table).
- Event system numbers: root 10, `life.illness` 12, `agent.returned_home` 50 (after the arrivals of the same tick).
- Cost: applying is O(targets + current illnesses) (the batch merges into the sorted column); 1,000 residents in a 5,000-agent world took 2.4 ms (release, Linux Xeon).
- Hash and event IDs: `Illness.root_event` is part of the state hash, so worlds with different event histories (for example extra `lod.changed` events) hash differently even when every agent behaves the same; behaviour comparisons across such worlds compare agent state.

**Out of scope:** other effects, L3+ application, FFI/Unity exposure, recovery, UI, undo.

## Consequences
- Easier: later effects add enum variants and payload tags under the same validation, journal, and event path; LOD exactness is a column plus one shared rule, not two implementations of state.
- Harder: the L1 and L2 movement rules both branch on illness and must stay in lock-step (guarded by the round-trip test with an afflicted population).
- Affected: `ws-world` (column, rule), `ws-events` (payloads, query), `ws-sim` (command, apply, `why_ill`), `ws-persist` (format 2), `ws-cli` (script tape), docs.
- Revisit if: effects need per-level behaviour that a shared column cannot express, or recovery/duration is added.
