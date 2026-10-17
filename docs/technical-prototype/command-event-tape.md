# TP-008 debug tape codec (WSJR version 1)

Implements [ADR 0019](../adr/0019-command-journal-and-world-events.md); not persistence or a save compatibility promise. All integers are little endian, fixed-width and unpadded. Negative simulation microseconds use the i64 two's-complement bits in u64 slots. Every parser rejects unknown versions/tags/reserved values, truncation and trailing data. Same-build/platform only; no authentication guarantee.

## Header

A u32 header-byte length precedes a 68–1024 byte bounded header body. Body offsets:

| Offset | Field |
|---|---|
| 0 | Four bytes `WSJR` |
| 4 | u32 format version 1 |
| 8 | u64 seed |
| 16 | i64 aligned start microseconds (runner currently epoch) |
| 24, 28 | u32 minimum, maximum population per building (currently 4, 12) |
| 32 | i64 base step microseconds (100000) |
| 40 | u64 dataset version |
| 48, 52 | u32 generator, rules versions (1, 1) |
| 56 onward | Three strings: build identifier, architecture, OS; each u32 byte count followed by exact UTF-8 bytes |

Build identifier combines Git HEAD (or `nogit` when Git is unavailable or the source is outside a checkout), compiler version, target, profile and a deterministic digest of workspace Rust sources/manifests/lockfile and relevant compiler flags/features. Dirty source changes are included. This is a compatibility guard, not a cryptographic authenticity claim. The runner reconstructs the expected header and requires exact equality before replay.

## Frames and operations

Every frame is 32 bytes followed by its exact body. Offset 0 u32 total size, 4 u32 record schema 1, 8 u32 kind, 12 u32 zero flags, 16 u64 operation sequence, 24 u32 body size, 28 u32 zero reserved. Require total = 32 + body, total ≤65600, body ≤65568 **before allocation/read**. Operation sequences start at 1; observations use the preceding operation's sequence. Footer uses the final sequence.

| Kind | Body |
|---|---|
| 1 Submit | Fixed metadata 32 bytes plus exact batch: offset 0 u32 batch length ≤65536; 4 u32 ingress status; 8 u32 accepted count; 12 u32 zero; 16 u64 first assigned sequence (zero if none); 24 u64 zero; 32 batch bytes. Max total 65600. |
| 2 Flush | Exactly 56 bytes: 0 u32 entry count; 4 u32 committed prefix count; 8 u64 next untouched sequence (zero if drained); 16 u32 terminal status; 20 u32 zero; 24 four u64 checkpoint values. |
| 3 Step | Exactly 40 bytes: 0 u32 terminal status; 4 u32 zero; 8 four u64 checkpoints. One physical tick, following explicit flush, no second implicit flush. |
| 4 Journal observation | 104–12396 bytes: ten u64 words (below), then exact original command encoded as a single-command batch. Compared byte-for-byte with newly generated outcome. |
| 5 Event observation | 104–2048 bytes, multiple of eight: canonical event words below; compared byte-for-byte with newly generated event. |
| 6 Checkpoint | Exactly 32 bytes: simulation hash, pending-queue digest, journal digest, event digest, all u64. |
| 7 Completion footer | Exactly 72 bytes: nine u64 words: total frames including footer, total operations, journal count, event count, completed ticks, then the four checkpoints. No trailing bytes permitted. |

Each Submit/Flush/Step must be immediately followed by **all and only** its new journal observations in order, then all new events in ID order, then one checkpoint. The initial world-created event is observed after the first operation. There is no way to omit an observation and still verify. Footer counts/checkpoints must match independently produced records. IO/capture failure leaves the footer absent: incomplete, unverifiable.

Ingress statuses: 0 accepted, 3 fixed capacity/counter limit, 4 environmental reservation stop. Tags 1 malformed and 2 unsupported version mirror ingress diagnostics, but raw structural/version failures are rejected before recording/replaying a Submit operation; they are transport tests, not logical tape inputs. Accepted empty batches assign no sequence. Flush statuses: 0 complete, 1 environmental stop, 2 counter exhausted, 3 domain infrastructure error, 4 history invariant/counter error, 5 invalid fence. Step statuses: 0 complete, 1 environmental stop, 2 time overflow, 3 domain error, 4 history error, 5 control error (not produced by the primitive physical tick).

For environmental Submit replay, decode and validate fixed logical limits first, then leave the queue/sequence allocator unchanged. Environmental Flush replay validates entry/prefix/next sequence, runs only the committed prefix through normal handlers, and stops before the next command; prefix must be shorter than entry. Zero prefix allocates no boundary ordinal. Environmental Step replay preflights deterministic clock/domain errors, drops the prepared domain plan, and leaves the post-flush world unchanged. Unexpected real replay resource failure aborts verification. Deterministic failure statuses are recomputed, not relabeled environmental.

## Command and journal fields

Per-command outcomes contain no full-world hashes. Replay still compares every outcome/event and the four checkpoint digests after each operation. This removes O(world) work per NoOp; old debug tapes are rejected by the changed source/build identity before decoding observations.

Command batch: u32 envelope version 2, u32 count ≤1024; each record u32 opcode, u32 payload length, exact payload. Payload starts u32 command schema 1, u32 issued_by (0 player, 1 script, 2 test). NoOp opcode 0 adds nothing. SetTimeScale opcode 1 adds u32 scale 0/1/10/100. SetNodeLod opcode 2 adds u32 entry count ≤1024, then u64 entity ID + u32 LOD 1/2 per entry. No padding/trailing bytes. The queue cap counts the same canonical record bytes plus one 8-byte batch header, irrespective of submission grouping.

Journal observation's ten u64 words: schema 1, assigned command sequence, application tick, application time bits, boundary ordinal, effect (scale tag or actual changed-building count), demoted, promoted, reason, reason detail. Reasons: 0 applied, 1 typed diagnostic schema, 2 too many nodes, 3 unknown node (detail raw ID), 4 duplicate node (raw ID), 5 unsupported LOD (tag), 6 domain rejection (reserved; infrastructure failures produce no outcome). CLI wire ingress accepts only schema 1 and supported LOD tags; diagnostic typed Rust commands are tested in core, not accepted as malformed wire artifacts.

## Canonical event words

All words u64, in exact `Event::visit_words` order:

1. Schema 1, ID, publication tick, commit ordinal, occurrence time bits, producer system, local sequence, payload tag, quantized importance, aggregate flag (0), LOD (0/1/2).
2. Origin tag (0 system, 1 command), then system ID or journal sequence.
3. Typed payload: tag 0 world-created = seed, dataset, generator, rules, population minimum, maximum; tag 1 command-applied = sequence, opcode, effect, demoted, promoted; tag 2 lod-changed = building raw ID, before LOD, after LOD, demoted, promoted; tag 3 arrived = resident raw ID, destination raw ID, leg counter.
4. Actor count ≤8, then pairs (raw ID, role 0 subject/1 object).
5. Location present 0/1; if present: cell raw ID, precise present 0/1, then three f32 offset-bit words when precise; path count ≤8 and raw entity IDs. Precise cell must equal the location cell, so it is not repeated.
6. Cause count ≤8, then triples (resolved event ID, relation, confidence). Relation 0 caused-by/1 enabled-by/2 triggered-by/3 contributed-to; confidence 0 direct/1 contributing/2 statistical/3 counterfactual. No local unresolved references survive publication.
7. Related count ≤8, then pairs (event ID, kind 0 part-of/1 follows-up/2 same-episode).

System orders: 0 creation, 10 applied command, 20 actual LOD changes, 30 L1 arrivals, 40 L2 arrivals. Arrival ordering inside each system is occurrence time, resident ID, leg. IDs and commit ordinals are sequential counters, never bit-packed. LOD changes link directly to their applied command event. Arrival events make no invented causal claims. Initial importance: creation 65535, applied 0, LOD 16384, arrival 4096.

## Fixtures and regression evidence

`ws-sim` test `version_two_wire_fixture_is_explicit_and_round_trips` commits an exact 76-byte mixed-command fixture with a known spawned ID. `ws-cli` unit test `frame_fixture_is_exact_little_endian_and_rejects_before_body_read` commits an exact 32-byte checkpoint-frame fixture. `maximum_batch_and_record_round_trip_and_forged_lengths_fail` constructs an accepted **65536-byte batch / 65600-byte Submit**, including semantic rejections, and rejects forged lengths/caps/reserved fields. Empty and 1024-command fixtures also round-trip. `environmental_fences_preserve_pending_retry_and_ordinals` covers zero-enqueue, zero-prefix, partial-prefix/retry and zero-commit Step fences and rejects an altered fence. Actual transaction fault injection and rollback are tested in `ws-sim::control_tests`. The separate-process CLI integration test covers mixed commands, pause/resume, repeated flushes, both LODs, full observation comparison and corruption/footer rejection. The format makes no interoperability promises beyond this schema and matching build/platform.
