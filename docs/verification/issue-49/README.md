# Issue #49: actor-index scaling

The sorted `(actor, event)` vector moved old references whenever an event arrived for any actor before the last actor in sort order. It is replaced by an append-only event-ID list per registered actor. Both implementations follow accepted ADR 0019: indices are rebuildable derived state and never affect canonical event records, digests or simulation state. No schema, save, FFI, dependency or retention policy changes.

## Reproduce

Run on an otherwise idle machine with the pinned toolchain:

```sh
cd core
cargo +1.97.0 run --release --locked -p ws-events --example actor_index
```

The same example was run before and after the library change, on macOS arm64 on 2026-10-19. The before library is main `4eeb8e00f6fbc4e3ac730d8f1455befcc9b87028`. This is a diagnostic probe, not a device or statistical performance guarantee.

1,001 registered actors; each event references one cycling resident and one shared destination. Build histories in batches of 256. At each history size, time nine more batches of 256, excluding candidate construction; report median nanoseconds per event for `prepare` plus `commit`. Capacity is sampled at exactly the initial history size, including every vector's spare capacity and actor-list metadata. Queries are measured after the nine batches (2,304 additional events), 9 groups of 100 calls, median ns/call; each actor query is checked against a full arena scan. Event size is 760 bytes on this machine.

| Initial events | Before publish ns/event | After publish ns/event | Before query ns | After query ns | Before allocated bytes | After allocated bytes |
|---|---:|---:|---:|---:|---:|---:|
| 1,000 | 1,393 | 459 | 83 | 57 | 827,392 | 858,840 |
| 10,000 | 5,571 | 425 | 169 | 173 | 13,238,272 | 12,997,080 |
| 100,000 | 46,619 | 368 | 574 | 536 | 105,906,176 | 103,808,472 |

`capacity_bytes()` excludes registered entity IDs as before; add 8,008 bytes for their backing storage in this probe. Values exclude allocator bookkeeping, stack fields, temporary transaction/query buffers and the benchmark's candidate buffers. These are allocated-capacity counts, not RSS. The new actor lists cost 24 bytes per registered actor before that actor has history; at 1,000 events the outer/list minimum allocations cost more than the old shared vector. At larger histories, storing an ID rather than an `(actor, ID)` pair reduces index storage despite spare capacity. Empty actors keep empty lists without inner allocations.

At 100,000 events this run's median publication is about 127 times faster. The actor index no longer moves old references: lookup is binary search over the fixed actor registry, current-batch reservation is grouped by actor, and commit appends IDs. No wall-clock measurement enters simulation code or tests. The existing occurrence-time vector still moves entries when occurrence times arrive out of order; this probe uses monotonic times and makes no claim about eliminating that independent cost. History remains unbounded and the results do not establish an iPhone memory/frame budget.

## Correctness coverage

- Multi-actor and non-adjacent duplicate roles, negative/tied/out-of-order occurrence times, inclusive bounds, reversed ranges, unknown actors and empty query results compare against an independent arena scan sorted by `(time, ID)`.
- Dropped preparation changes no logical history, counters, query indices or digest; capacity growth alone is permitted by the transaction contract.
- Rebuilding from records gives the identical store; continued publication after rebuilding keeps records, counters and digest equal.
- Unit coverage checks every touched actor list is reserved before publication, unused actors allocate no inner storage, duplicate roles append once, commit grows no buffers, and capacity accounting includes outer/inner spare capacity.
- Existing event, command/replay, save/load and allocation-failure rollback tests remain in place without weaker assertions or changed goldens.
