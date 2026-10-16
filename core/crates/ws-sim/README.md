# ws-sim

## Responsibility

Owns authoritative `World`, boundary command application, journal and meaningful event publication under [ADR 0019](../../../docs/adr/0019-command-journal-and-world-events.md). Rust owns domain state; hosts submit owned commands and schedule explicit physical ticks or host-elapsed advances.

## Owned state

Seed, fixed-step `Clock`, `ToyWorld`, bounded pending queue and enqueue allocator, append-only outcomes, boundary ordinal, and `ws-events::EventStore`. `WorldConfig` supplies seed, aligned start and population. The clock uses 100 ms steps and authoritative Paused/×1/×10/×100 scale. Immutable `clock()` and `toy()` views have no mutation escape.

## Commands and emitted event types

Use `commands::Command::new(IssuedBy::Player, Action::SetTimeScale(TimeScale::Paused))`, `World::submit(vec![command])`, then `flush_commands()`. `SetNodeLod(Vec<(EntityId,Lod)>)` requests the existing atomic domain transaction. `NoOp` creates a journal outcome without changing simulation state. `submit_bytes` decodes the explicit version-2 batch codec. Success means queued, never already applied. There are no direct World scale/LOD setters.

Emits `sim.world_created`, one `sim.command_applied` for every Applied outcome including idempotent commands, one `sim.lod_changed` per actual changed building (directly caused by its applied-command event), and `agent.arrived` from successful L1/L2 domain plans. Rejections have journal outcomes but no applied events. Initial materialization, conversion and scheduler wakeups do not fabricate arrivals. Event/journal read access is immutable and Rust-only.

## Invariants

- Batches/queue have 1024-command and 65536-byte caps, including one canonical 8-byte batch header. Invalid ingress is atomic and consumes no sequences. Typed diagnostic schema/LOD errors are journaled semantic rejections; wire structural/version errors fail before enqueue.
- Commands apply in enqueue order at serialized boundaries. Each command's state, outcome, events and queue removal commit together. Fixed semantic rejection changes no simulation state and continues; infrastructure failure leaves the current command/suffix pending and preserves earlier commits.
- Each nonempty flush allocates one boundary ordinal on its first committed outcome. Empty/zero-prefix flushes allocate none. Multiple paused flushes are distinct boundaries without advancing time.
- `step()` flushes once then attempts one physical tick regardless of scale. Physical domain/clock/history commit atomically; failure leaves the **post-flush** state intact. `physical_step()` is the recorder/host primitive without implicit flush. `flush_prefix` and `validate_physical_step` support bounded environmental replay fences, never arbitrary mutation.
- Host-elapsed advance flushes before budgeting with committed scale. Its existing cap is 100 ticks, excess whole steps are dropped, and the sub-step remainder survives pause/scale changes. Time overflow is checked before physical work; partial tick success counts exactly committed ticks.
- `now == start + tick * base_step`, the seed never changes, and time never wraps. Domain decisions retain canonical ordering/counter-based randomness. History/index state never feeds domain evolution.
- Simulation hash includes scale, seed, tick, time and canonical toy state; excludes history, indices, host pacing and pending inputs. Queue/journal/event digests are compared separately at every replay operation. Event IDs/ordering are Tier 1 for the same candidate stream; evolving trajectories/history are Tier 2 within one build/platform.
- All actors resolve to stable toy identities. History is append-only for the session; no saves, retention or pruning here. No unsafe code, runtime external dependencies, filesystem/environment or wall-clock reads.

Run `cargo test -p ws-sim` for ingress/FIFO/completeness, pause, no-op/rejections, actual LOD causes and arrivals, rollback/fault injection, non-feedback, and existing clock/replay properties. [ws-cli](../ws-cli/README.md) verifies full journal/events/checkpoints in another process. [Codec](../../../docs/technical-prototype/command-event-tape.md) documents the bounded diagnostic artifact.

## Performance

`cargo bench -p ws-sim` (criterion, dev-only) on the Linux machine used for TP-006 (Intel Xeon, 2.1 GHz), after 600 warm-up ticks so most agents are walking:

| Benchmark | Time | Per agent |
|---|---|---|
| `World::step`, 50 agents | 38 µs | 0.76 µs |
| `World::step`, 5,000 agents | 4.0 ms | 0.80 µs |
| `World::new`, 5,000 agents (generation) | 13 ms | 2.6 µs |

Most of the step is the per-agent `SurfacePos::reexpress` (a cell lookup, about 0.6 µs in `ws-math`'s benchmark) and building each agent's cell frame. Against [15-ios-performance-memory.md](../../../docs/architecture/15-ios-performance-memory.md) §16.3 (≤ 8 ms per frame for high-priority simulation): at the 100 ms base step and ×1, 5,000 agents cost about 4 ms once every three frames at 30 fps, so a frame that runs a tick has about 4 ms of headroom; at ×10 (about 3.3 ticks per frame) the same 5,000 agents would need about 13 ms and exceed the budget. Memory is about 96 bytes per agent (see the ws-world README), 0.5 MB for 5,000, against the 150–250 MB for materialized entities in §16.4. iPhone measurements belong to TP-011.

### Simulation LOD (TP-007)

`cargo bench -p ws-sim --bench lod` on the Linux machine used for TP-007 (Intel Xeon, 2.8 GHz, 4 vCPU; a shared cloud machine, slower than the TP-006 one: `world_step/5000` measures 5.4 ms on it, against 4.0 ms above, both on `main` before TP-007 and with it). Toy-world agents in five buildings, after 600 warm-up ticks:

| Benchmark | 30 agents | 3,000 agents | 100,000 agents |
|---|---|---|---|
| Demote all (`set_lod` → L2) | 16 µs | 1.71 ms (0.57 µs/agent) | 92 ms (0.92 µs/agent) |
| Promote all (`set_lod` → L1) | 18 µs | 1.88 ms (0.63 µs/agent) | 100 ms (1.00 µs/agent) |
| One base step, all L1 | | 3.30 ms | 106 ms |
| One base step, all L2 | | 0.91 µs | 49 µs |
| One simulated hour, all L2 | | 37 ms (44,307 events) | 2.2 s (1,470,819 events) |

- **Transitions.** A 3,000-agent transition takes 1.7 to 1.9 ms, inside the 4 ms (half of the 8 ms high-priority budget, 15 §16.3) that ADR 0017 §15 asks for. Per-agent cost rises by about 60% from 3,000 to 100,000 agents (cache misses in the larger columns), still within the ADR's 1 µs (demote) and 2 µs (promote) targets.
- **L2 against L1.** About 14.8 events per agent per simulated hour, as ADR 0017 §15 predicts. An hour costs 0.84 µs per event at 3,000 agents and 1.5 µs at 100,000, step overhead included (target ≤ 2 µs). An all-L1 hour at 3,000 agents is 36,000 steps of 3.30 ms, about 119 s, so L2 is about 3,200 times cheaper (about 1,700 times at 100,000), against the target of at least 1,000.
- A step with no event due costs a queue peek and a check of the (empty) unreconciled index: under 1 µs at 3,000 agents. Without that index the first measurement was 19 µs, because reconciliation scanned every agent.


### TP-008 history measurement (Mac, 2026-10-16)

`cargo bench --locked -p ws-sim --bench history -- --warm-up-time 1 --measurement-time 2`, Apple M1 Pro, Rust 1.97.0 release (overflow checks retained):

| Measurement | Result |
|---|---|
| 1,000 two-actor events, exact batch reservation | 808,000 bytes arena + time/actor indices (808 bytes/event; inline record 760 bytes) |
| Actor query, 10 matches among 10,000 records | 170.16–170.95 ns (mean estimate 170.54 ns) |
| 5,000 L1 agents, 500 ticks after 3,000 warm-up ticks, World with history | 1.5996–1.6227 s (3.223 ms/tick at estimate) |
| Same state/ticks, domain + clock without publication | 1.5882–1.6312 s (3.206 ms/tick at estimate) |

The estimated incremental orchestration/publication difference is about 0.5%, with overlapping intervals; this does not establish a precise overhead percentage. Both paths include domain fact collection; the comparison does not isolate that collection's cost against pre-TP-008 code. Bounded 500-tick clones keep the benchmark from silently timing different growing histories. These are desktop observations, not iPhone acceptance evidence. At ×10 a few 3.2 ms ticks per frame can exceed the 8 ms high-priority budget; use L2 and measure on device in TP-011.

The 100–150 MB world-tree/aggregate/index budget in architecture §16.4 is shared, not all available for history. At the measured minimum capacity, 100,000 such records alone would use 80.8 MB. Registry IDs, query buffers, domain state, allocator overhead, spare geometric capacity and process RSS are excluded. Long sessions exceed this budget without future retention/persistence. Sorted vector index insertion can move prior entries; short toy measurements do not prove long-history write scaling. No per-event heap payload is used; batch buffers and query results allocate separately. CLI per-operation journal hashing/serialization is debug work, outside the domain tick timing.
