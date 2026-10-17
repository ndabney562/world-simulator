# ws-events

Responsibility: versioned factual history and read-only queries under [ADR 0019](../../../docs/adr/0019-command-journal-and-world-events.md). It has no dependency on simulation orchestration or domain systems.

Owned state: an append-only inline `Event` arena, sorted occurrence-time and actor/ID indices, a registered toy actor set, event/commit counters, and a rolling digest. Each record has schema, ID, tick/commit/system/local order, integer occurrence time, origin, bounded roles/location/path/causes/related links, quantized importance and aggregate flag. `Payload::name()` provides the hierarchical taxonomy. The initial producers emit `sim.world_created`, `sim.command_applied`, `sim.lod_changed`, and `agent.arrived`. TP-010 ([ADR 0021](../../../docs/adr/0021-god-intervention-contract.md)) adds three payloads:

| Name | Tag | System | LOD | Links |
|---|---|---|---|---|
| `intervention.applied` (sequence, effect, illness, severity, newly affected) | 4 | 10 | 0 | none: the root, `Origin::Command(sequence)` |
| `life.illness` (resident, illness, severity), one per newly ill resident | 5 | 12 | 0 | `CausedBy` the root, confidence direct |
| `agent.returned_home` (resident, home), an ill resident's arrival at its home | 6 | 50 | 1 or 2 | `ContributedTo` that resident's `life.illness`, confidence contributing |

`root_intervention(event)` walks `CausedBy` (then `ContributedTo`) links back to the root and returns it if it is an `intervention.applied`; it answers "why is this agent ill?" from any consequence.

Invariants: IDs start at one and never repeat. Transactions are canonically ordered; causal targets must already exist or precede the source within the transaction, with occurrence time no later than the source. Local causal references resolve to committed IDs before publication. Every actor/path resolves to the registered live toy identity set; deletion/legends are later work. All bounded lists reject overflow. Preparation validates and reserves all arena/index storage before commit; dropping it changes no logical records, counters or digest. `from_records` validates all metadata and rebuilds counters/indices. No query or index feeds simulation state.

`get(id)` uses the arena; `query(Some(actor), from, to)` and `query(None, from, to)` use inclusive time bounds and return occurrence-time/ID order, deduplicating actor roles. `backward(id, depth, results)` returns deterministic visited IDs and an explicit `truncated` flag. Unknown roots return an empty traversal. Queries allocate their result buffers, never mutate records, and do not promise bounded total history storage.

`Event::visit_words` visits the canonical field sequence used for replay comparison and hashing; the [debug tape codec](../../../docs/technical-prototype/command-event-tape.md) specifies the fields. This is not a save codec. The sorted vector actor index keeps reads small but insertion can move existing entries; retention, compaction, persistence and long-session scaling are excluded from TP-008.
