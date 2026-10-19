# ws-events

Responsibility: versioned factual history and read-only queries under [ADR 0019](../../../docs/adr/0019-command-journal-and-world-events.md). It has no dependency on simulation orchestration or domain systems.

Owned state: an append-only inline `Event` arena, a sorted occurrence-time index and per-actor append-only ID lists, a registered toy actor set, event/commit counters, and a rolling digest. Each record has schema, ID, tick/commit/system/local order, integer occurrence time, origin, bounded roles/location/path/causes/related links, quantized importance and aggregate flag. `Payload::name()` provides the hierarchical taxonomy. The initial producers emit `sim.world_created`, `sim.command_applied`, `sim.lod_changed`, and `agent.arrived`. TP-010 ([ADR 0021](../../../docs/adr/0021-god-intervention-contract.md)) adds three payloads:

| Name | Tag | System | LOD | Links |
|---|---|---|---|---|
| `intervention.applied` (sequence, effect, illness, severity, newly affected) | 4 | 10 | 0 | none: the root, `Origin::Command(sequence)` |
| `life.illness` (resident, illness, severity), one per newly ill resident | 5 | 12 | 0 | `CausedBy` the root, confidence direct |
| `agent.returned_home` (resident, home), an ill resident's arrival at its home | 6 | 50 | 1 or 2 | `ContributedTo` that resident's `life.illness`, confidence contributing |

`root_intervention(event)` walks `CausedBy` (then `ContributedTo`) links back to the root and returns it if it is an `intervention.applied`; it answers "why is this agent ill?" from any consequence.

Invariants: IDs start at one and never repeat. Transactions are canonically ordered; causal targets must already exist or precede the source within the transaction, with occurrence time no later than the source. Local causal references resolve to committed IDs before publication. Every actor/path resolves to the registered live toy identity set; deletion/legends are later work. All bounded lists reject overflow. Preparation validates and reserves all arena/index storage before commit; dropping it changes no logical records, counters or digest. `from_records` validates all metadata and rebuilds counters/indices. No query or index feeds simulation state.

`get(id)` uses the arena; `query(Some(actor), from, to)` and `query(None, from, to)` use inclusive time bounds and return occurrence-time/ID order, deduplicating actor roles. `backward(id, depth, results)` returns deterministic visited IDs and an explicit `truncated` flag. Unknown roots return an empty traversal. Queries allocate their result buffers, never mutate records, and do not promise bounded total history storage.

`Event::visit_words` visits the canonical field sequence used for replay comparison and hashing; the [debug tape codec](../../../docs/technical-prototype/command-event-tape.md) specifies the fields. This is not a save codec. Issue #49 replaces the shifting actor/event-pair vector with one append-only ID list per sorted registered actor. Preparation groups only the current batch's distinct actor/event references and fallibly reserves each touched list; commit appends monotonic IDs without allocating or moving old actor references. Duplicate roles append once. Actor queries still sort by occurrence time/ID, because occurrence times need not follow commit order. `from_records` rebuilds exactly the same logical index; indices and capacity never affect event IDs, digests, saves or simulation state.

`capacity_bytes()` includes the event arena, time index, the outer actor-list allocation and every inner list's spare capacity; it excludes registered identity storage as before. The standalone `actor_index` example measures publication at growing histories, query equality/latency and capacity on the same workload before/after #49. See [measurements](../../../docs/verification/issue-49/README.md). The time index still shifts for out-of-order occurrence times. History remains unbounded; retention, compaction and durable storage are separate work.
