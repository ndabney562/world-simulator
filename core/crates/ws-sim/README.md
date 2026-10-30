# ws-sim

## Responsibility

Owns authoritative `World`, boundary command application, journal and meaningful event publication under [ADR 0019](../../../docs/adr/0019-command-journal-and-world-events.md). Rust owns domain state; hosts submit owned commands and schedule explicit physical ticks or host-elapsed advances.

## Owned state

Seed, fixed-step `Clock`, `ToyWorld`, bounded pending queue and enqueue allocator, append-only outcomes, boundary ordinal, and `ws-events::EventStore`. `WorldConfig` supplies seed, aligned start and population. The clock uses 100 ms steps and authoritative Paused/×1/×10/×100 scale. Immutable `clock()` and `toy()` views have no mutation escape.

## Commands and emitted event types

Use `commands::Command::new(IssuedBy::Player, Action::SetTimeScale(TimeScale::Paused))`, `World::submit(vec![command])`, then `flush_commands()`. `SetNodeLod(Vec<(EntityId,Lod)>)` requests the existing atomic domain transaction. `NoOp` creates a journal outcome without changing simulation state. `submit_bytes` decodes the explicit version-2 batch codec. Success means queued, never already applied. There are no direct World scale/LOD setters.

`Intervene(Intervention { target, effect, timing })` (opcode 3, [ADR 0021](../../../docs/adr/0021-god-intervention-contract.md)) is the first god intervention: a closed effect vocabulary (v1: `afflict(Fever, severity 1..=10)`), targets one resident or all residents of a building, resolved in ascending ID order when applied. `Timing::At(T)` waits in the world (`scheduled`, saved) and takes effect at the end of the first tick whose time is at least T; a time already reached applies now. Rejections (`UnknownTarget`, `UnsupportedLevel`, `SeverityOutOfRange`) change nothing. `World::why_ill(resident)` returns the `life.illness` event, the root `intervention.applied` event and the journal entry. Applying costs O(targets + current illnesses): about 2.4 ms for 1,000 residents in a 5,000-agent world (release, Linux Xeon), no world scan.

Emits `sim.world_created`, one `sim.command_applied` for every Applied outcome including idempotent commands, one `sim.lod_changed` per actual changed building (directly caused by its applied-command event), and `agent.arrived` from successful L1/L2 domain plans, plus `intervention.applied`, `life.illness` and `agent.returned_home` (see the `ws-events` README). Rejections have journal outcomes but no applied events. Initial materialization, conversion and scheduler wakeups do not fabricate arrivals. Event/journal read access is immutable and Rust-only.

## Invariants

- Batches/queue have 1024-command and 65536-byte caps, including one canonical 8-byte batch header. Invalid ingress is atomic and consumes no sequences. Typed diagnostic schema/LOD errors are journaled semantic rejections; wire structural/version errors fail before enqueue.
- Commands apply in enqueue order at serialized boundaries. Each command's state, outcome, events and queue removal commit together. Fixed semantic rejection changes no simulation state and continues; infrastructure failure leaves the current command/suffix pending and preserves earlier commits.
- Each nonempty flush allocates one boundary ordinal on its first committed outcome. Empty/zero-prefix flushes allocate none. Multiple paused flushes are distinct boundaries without advancing time.
- `step()` flushes once then attempts one physical tick regardless of scale. Physical domain/clock/history commit atomically; failure leaves the **post-flush** state intact. `physical_step()` is the recorder/host primitive without implicit flush. `flush_prefix` and `validate_physical_step` support bounded environmental replay fences, never arbitrary mutation.
- Host-elapsed advance flushes before budgeting with committed scale. Its existing cap is 100 ticks, excess whole steps are dropped, and the sub-step remainder survives pause/scale changes. Time overflow is checked before physical work; partial tick success counts exactly committed ticks.
- `now == start + tick * base_step`, the seed never changes, and time never wraps. Domain decisions retain canonical ordering/counter-based randomness. History/index state never feeds domain evolution.
- Simulation hash includes scale, seed, tick, time and canonical toy state; excludes history, indices, host pacing and pending inputs. Queue/journal/event digests are compared separately at every replay operation. Event IDs/ordering are Tier 1 for the same candidate stream; evolving trajectories/history are Tier 2 within one build/platform.
- All actors resolve to stable toy identities. History is append-only for the session; no retention or pruning here. Saving and loading live in [ws-persist](../ws-persist/README.md): `World::to_parts` / `World::from_parts` expose the canonical state and rebuild a world from it, refusing parts that break an invariant (`tests/restore.rs`). No unsafe code, runtime external dependencies, filesystem/environment or wall-clock reads.

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

Per-command journal outcomes store application metadata and effects, not before/after full-world hashes. The debug host still verifies simulation, queue, journal and event checkpoints at operation boundaries. The `history` Criterion benchmark includes maximum-batch NoOp flushes at toy and 5,000-agent populations; setup/submission/destruction are outside the timed flush. Actor-index write scaling is tracked separately in GitHub issue #49; no retention or index redesign is included here.

### Command flush review measurement (2026-10-17)

Same Apple M1 Pro, Rust 1.97.0 release, after all local suites finished; Criterion 10 samples, 1 s warm-up and 2 s requested measurement (extended for slow baseline). Same new benchmark source was compiled against baseline `041d891` and correction `2fa55ba`; isolated binaries ran sequentially.

| 1,024 NoOps | Baseline 95% interval / estimate | Corrected 95% interval / estimate |
|---|---|---|
| Toy population, seed 1 | 6.8172–6.8832 ms / 6.8411 ms | 262.35–263.01 µs / 262.64 µs |
| 5,000 agents | 794.67–795.48 ms / 795.06 ms | 269.90–272.25 µs / 271.27 µs |

The maximum-batch flush is about 0.271 ms at 5,000 agents after removing the two full-world hashes per outcome. Setup, submission and destruction are outside timing; outcome/event publication remains inside. Per-operation checkpoint hashing and tape serialization remain separate debug-host work. This meets the requested few-ms desktop flush check, not a total frame or iPhone budget claim. Existing tick-overhead measurements above remain historical evidence; no physical-tick algorithm changed here.

Reproduce from `core/`: `cargo bench --locked -p ws-sim --bench history -- flush_1024 --warm-up-time 1 --measurement-time 2`. For before/after comparison, copy this benchmark source into an isolated baseline checkout and preserve separate executables/target directories; do not run other local suites during timing. An earlier concurrent measurement is excluded; the table uses only the isolated runs.

### Movement frame reuse (#31, 2026-10-16)

On an Apple M1 Pro (arm64), Rust 1.97.0 release `world_step/5000` improved from 3.3330 to 2.2909 ms (about 31%). This meets the approximate 2.4 ms target on that Mac. Claude's independent Linux Xeon review measured 2.62–2.65 ms, so the target remains unmet on that reference machine; its average ×10 tick cost is about 8.8 ms per 30 fps frame before other work. Neither result establishes iPhone performance.

See [ws-world's implementation notes](../ws-world/README.md#movement-frame-reuse-31-2026-10-16) and the [verification report](../../../docs/verification/issue-31/README.md) for the cache's exactness, historical measurements and replay evidence. Older numbers above describe their original machines/revisions.

The merged TP-003 bridge now follows ADR 0019 §8 as ABI v2: commands submitted through `ws_submit` use this same queue/journal, zero-step advance flushes, and a 16-byte committed-control-state read supplies host pacing. The native and C# adapters do not expose a direct mutation escape or multiply scale twice. See the [bridge contract](../ws-ffi/README.md).

### Inside-cell fast path (#48, 2026-10-17)

The movement step now proves that most agents are still inside their cell without the full cell lookup (see the ws-math and ws-world READMEs), with bit-identical results. Same Linux Intel Xeon 2.1 GHz machine as the #31 review, Rust 1.97.0 release, `main` (`cb42e62`, after #31) and this change measured alternately, 30 samples each:

| Benchmark | `main` | #48 |
|---|---|---|
| `world_step/50` | 27.0–27.3 µs | 4.8–5.5 µs |
| `world_step/5000` | 2.65–2.79 ms | 0.35–0.38 ms |
| `lod_step/l1/3000` | 1.61–1.69 ms | 0.22–0.23 ms |
| `lod_step/l1/100000` | 54.9–57.6 ms | 7.6–8.4 ms |

That meets #31's 2.4 ms target on this machine with room to spare: at ×10 (about 3.3 ticks per 30 fps frame) 5,000 agents cost about 1.3 ms of the 8 ms budget (15 §16.3). LOD transitions, the L2 step and the L2 hour stayed within run-to-run noise over two alternating runs (for example `lod_transition/promote/3000` 1.20–1.24 ms on `main`, 1.21–1.27 ms with #48). These are desktop numbers; iPhone measurements belong to TP-011.

## Real-slice schedules (VS-004)

`slice::SliceSimulation::new(SliceConfig { seed, start, schedule_epoch })` owns the immutable `PopulatedSlice`, [ws-life](../ws-life/README.md) schedules, existing fixed-step clock and registered event store. Start and explicit model-Monday epoch must lie on the 100-ms grid. This separate path preserves every existing toy/CLI/bridge contract. VS-005 adds retained L1/L2 need/utility state and proxy path views below; camera LOD, real-calendar alignment, rendering and saves remain separate work.

`apply(Operation::Steps(n) | SetScale(scale) | HostElapsed(us))` returns a typed committed-prefix report. Log reservation fails before work starts; the attempted operation is then unrecorded and must not be treated as accepted. A physical tick validates clock advance, plans/reserves facts and publishes clock/history together. On event/clock failure the failing tick is unchanged, while prior ticks remain committed. Host budgeting preflights full clock overflow; a failed tick preserves the prior pending remainder. Successful host calls retain the existing cap of 100 and drop excess whole steps. Paused host calls take no ticks; explicit steps still do.

One `sim.slice_created` fact records seed/source/generator/rules/epoch at construction. Each event-bearing physical tick publishes sorted schedule transitions separately, so batch size does not change IDs, ordinals or digests. A cached next-schedule-boundary instant avoids schedule scans on quiet ticks; it is reconstructed from immutable schedules and excluded from hashes. Behavior planning additionally checks retained anchors and drains minute decisions when due. The domain never reads history.

State hashing covers pinned configuration, versions, committed time/tick/scale and canonical domain tables/answers in ID order. Pacing remainder, histories and capacities are separate. The operation digest writes explicit primitive integer fields, never Debug strings; successful explicit step groups normalize identically and their log records coalesce. Failed operations retain a state/event/pacing checkpoint; environmental replay runs only the committed prefix, verifies that checkpoint and stops rather than retrying the failed suffix. This is a typed Rust session API; it introduces no persistent diagnostic format. New events are deliberately rejected by legacy save encoders/decoders with typed errors; the saver checks before touching the destination or staging file. #53/#75 own new slice formats.

Tests cover a full week of actual event publication, index reconstruction/query non-feedback, batch/single-step equality, pause/scales/cap/remainder, injected failing-tick prefixes/rollback/retry, extreme time/grid errors and separate-process typed-script replay. Independent timetable/reference/golden tests live in ws-life. New payload tags 7/8 preserve all old tags/digest field order and all old golden/save fixtures.

`SliceSimulation::replay(config, records)` checks recorded outcomes and typed stop checkpoints. Replay input ends at its first failed operation: suffixes after that stop are rejected. A caller may inspect/verify the returned stopped world and explicitly resume with new operations; a failure is not silently retried during replay. Tests compare normalized operation digests for successful and environmentally stopped prefixes and reject corrupt checkpoints or suffixes.

### VS-004 desktop measurements (2026-10-19)

[Verification and limitations](../../../docs/verification/issue-68/README.md). Criterion on the same Mac/build measured quiet slice ticks at about 7.3 ns amortized over 1,000-tick runs, the first event-bearing tick at 2.4–2.8 µs for 18–24 residents, and the current toy 5,000-resident reference at 247 µs/tick over 100-tick runs. These compare different simulation workloads; they establish desktop slice headroom, not iPhone performance or L1/frame cost. Canonical state/hash query costs are outside the step benchmark.

## Real-slice active behavior (VS-005)

Accepted [ADR 0028](../../../docs/adr/0028-slice-l1-needs-and-navigation.md) adds `behavior()` and `resident(id)` read-only views and `Operation::SetDetail { resident, level }` for manual Rust-side L1/L2 requests. The typed enum accepts only implemented levels; an unknown identity records a typed rejected outcome with unchanged state/events. Queries and successful manual level changes publish no false arrivals or extra schedule facts and consume no utility counter. VS-006 below adds camera ownership and production LOD facts.

Each physical tick holds a borrowed prepared behavior transaction while schedule facts/history are planned/reserved; only then do history, behavior and clock publish without another fallible step. Failure discards tentative need/action/counter updates and leaves the failing tick unchanged. Existing committed-prefix, pause/scale/pacing and replay-stop rules remain intact. Domain-resource stops are verified from the recorded committed prefix, just like history-resource stops. As before, a replay input ends at its first failed operation; resuming is explicit, and an unknown-resident rejection is not silently ignored.

Slice hashes additionally cover behavior/navigation rules, graph fingerprint, level partition and retained canonical anchors/action/counters. Actual fine path positions are reconstructable and tested in separate-process replay; old toy/event/save/FFI fields remain unchanged. All people remain present at L1 or L2 while off-map. Retaining common minute decisions enables exact mixed-level catch-up and zero-time round-trip hashes; no schedule is repaired by teleporting on a level change.

`cargo bench --locked -p ws-sim --bench active` measures cold all-pairs Dijkstra paths,100 physical ticks including every fine resident view, one minute-decision tick, and one hour of standalone coarse-domain catch-up. Fresh clones/destruction are outside timed step/advance workloads; state hashing is separate diagnostic work. The VS-004 measurements above are historical, before this added domain. Current measurements and explicit limitations live in [issue-69 verification](../../../docs/verification/issue-69/README.md). No host benchmark establishes a device/frame result.

## Real-slice camera policy (VS-006)

`Operation::CameraInterest` stores one validated integer region under [ADR0029](../../../docs/adr/0029-slice-camera-interest-and-aggregates.md); its input/outcome enters the typed log. It changes no time, utility counter or fact immediately. The latest accepted input owns membership permanently; later `SetDetail` requests record `LodError::CameraOwned` without changing state/events, even before the first policy tick. Initial pre-camera manual views at L2 nodes remain visible until the next physical membership plan; L3 never exposes fine residents.

`lod()` is read-only; `view()` returns all residents and exact building/block counts from one committed boundary. Queries do not step or journal. `discard_observation_cache()` affects only derived acceleration; hashes/history/views are unchanged. Node modes, input and change times are canonical; recent-observation and logical-answer caches are not.

Each tick prepares common needs, controller modes/every-tick membership/observations, complete count invariants and one combined fact vector. System60 schedule facts precede system70 slice LOD facts in a single event-store commit. Only after all reservations succeed do events, behavior, controller and clock publish without a fallible step. Controller-resource failures use the same explicit committed-prefix/terminal-stop replay contract as behavior/history failures. A different valid camera script is a different replay, distinguished by expected operation/history digests; failed-record fences are explicitly checked.

New tests cover typed camera/manual rejections, first-input views, dwell idempotence, batch equivalence, simultaneous producer ordering/index rebuild, common/controller/cache/history rollback, physical and host-elapsed resource prefixes, and per-boundary state/event/operation/view comparisons in a separate process. `cargo bench --locked -p ws-sim --bench camera` measures complete four-node promote/demote ticks (including observation/history planning) and bulk fine views for18/20/24/30 residents. See [verification](../../../docs/verification/issue-70/README.md). The current native/Unity bridge still renders the toy path; #72 owns the coarse real-slice camera/snapshot adapter.

## VS-007 committed calendar/sky

`World::calendar_sky()` returns one immutable boundary: tick, SimTime, canonical state hash, committed scale, exact UTC and optional core-derived New York/sun output in the paired render basis. It does not flush pending commands, tick, consume RNG or retain derived state. `SliceConfig::calendar_aligned_v1` constructs an explicit stored model-week epoch using NY policy1; `config()` queries it without recomputing alignment. Raw configuration remains compatible and all old schedule/behavior rules/goldens remain unchanged; elapsed model weeks stay uniform across later DST/leap boundaries. New tests compare calendar/state/checkpoint output at every script boundary in separate processes with pause/1/10/100 scales, and query-independence tests preserve pending commands. [ADR0030](../../../docs/adr/0030-calendar-sun-and-sky-query.md); [verification](../../../docs/verification/issue-71/README.md).


## VS-008 coarse real-slice views

`slice::render` composes immutable complete source geometry, stable-ID resident rows, exact four-node summaries and calendar/sky from one committed boundary. Only actual local fine residents have positions; coarse/off-map identities remain represented with explicit status and zero geometry. `camera_operation` converts one finite bounded render-frame target/radius into the existing integer camera operation before application/replay. Queries retain no state, do not tick/flush/materialize or change history/hash/RNG. New tests preserve pending policy boundaries, count partitions, real source positions and integer replay. The native adapter owns its separate slice handle/cache under ADR0031; device evidence is still required by #72.

VS-009 / ADR0032 adds `slice::inspect` for pure retained-resident/featured-building records: exact synthetic/source identity, current common needs/action/logical location, model timetables, building counts and bounded factual actor events. Context/household/workplace IDs are not inspection targets. Queries never step, materialize, consume randomness, append history or affect replay.

ADR0033 adds a retained vitals column initialized once from the existing cohort. Inspection borrows it and LOD/camera/time do not regenerate it. The new canonical slice-state/v2 envelope includes the previous state hash, vitals policy1 and its digest. The old calculation remains private with regression values captured from the previous native library. Real-slice persistent saves remain #75; toy save formats and old lower-level goldens are unchanged.

## VS-010 finite illness (ADR0034)

`Operation::Afflict` targets a retained living resident, severity1..10, grid-aligned duration100000..86400000000µs. Semantic rejection is journaled without simulation/history effects and may precede later replay records. A successful episode stores its provenance/timers, lowers health to100−9×severity and restores100 at its half-open end. Duplicate active/pending-return requests record an affected=0 root. Continuous home routines, exact duty interruptions and eligible L1 visits are planned before controller/history publication; resource failures preserve the boundary. New episode policy1 enters the conditional slice-state/v3 envelope; healthy paths retain previous hashes.

`consequences(root)` binary-searches a derived index then performs at most16 direct EventId lookups. Rows contain only actual producer-linked facts with explicit confidence. Each group has fixed slots; index capacity grows with intervention history and is excluded from canonical hashing. `rebuild_consequence_index` reconstructs it from typed roots/explicit links. Episodes and routine overrides are canonical; saves remain #75.

## GP-002 slice powers (ADR 0038)

Operations: `Operation::Bless { resident }`, `Storm { duration_us }` and `Gather { building, duration_us }` (FFI tags 6, 7, 8 in #125 Task 8; operation-digest words 8, 9, 10). `illness::Rejection` appends `Building` and `Busy` after `Deadline`, so existing codes keep their numbers. Until #125 Tasks 5 and 6 land, `Storm` and `Gather` are journalled `Busy` rejections with no state or history effect.

Bless (`slice/powers.rs`) targets a living resident; an unknown target is `Target`, a dead one `Dead`, both journalled with no effect. It applies at the current committed boundary and records root `intervention.bless_applied` (tag 14, system 10, command origin), then:

- an active illness episode is cured: `episode.end = now`, `recovered = true`, health 100, and one `life.recovered` (system 84) linked `CausedBy/Direct` to the bless root and `ContributedTo/Direct` to the episode's `life.illness`. The natural recovery at the original end never fires. The ADR 0034 home-hold routine, any pending `agent.returned_home` and `illness_state(r).own_root` (the illness root) are unchanged;
- otherwise the four need deficits are zeroed through `Prepared::relieve` (the checked `Needs` path) and one `life.needs_relieved` (system 84) is recorded `CausedBy/Direct` the root;
- a healthy resident already at zero deficits records only the root with `affected = 0`.

Bless adds no slice state, so the state hash is unchanged by a no-op and existing goldens are unchanged when no power is used. `bless_preflight` is shared with environmental stop-fence replay.

Consequence groups are per root: `Group { root, subject: Option<EntityId>, tag, severity, applied, end, .. }`. `Group::for_root` derives the header from the root payload alone (illness: resident, severity, end or None for a duplicate; bless: resident, severity 0, end = applied; storm: no subject, end = applied + duration; gathering: building, end = applied + duration), and both the live producers and `rebuild_consequence_index` use it. A fact belongs to the root that `EventStore::root_intervention` reaches from it (first `CausedBy`, else first `ContributedTo`), in the live tick index and in the rebuild alike, so a bless cure indexes under the bless root only and the illness group has no recovery row. `INDEX_EXCLUDED` (tag 17, `life.stayed_inside`) is never indexed by either path. Invariants: at most 16 IDs per group (producers fail before commit), ascending IDs, and a rebuilt index equal to the live one (`tests/slice_powers.rs`).
