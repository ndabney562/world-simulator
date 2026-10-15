# ws-ecs

## Responsibility

Entity identity and canonical component storage ([ADR 0016](../../../docs/adr/0016-entity-identity-storage-and-randomness.md) §1–2; S-1 and D-7 in [21-decisions-now-vs-later.md](../../../docs/architecture/21-decisions-now-vs-later.md)). A foundation crate with no dependencies, so `ws-world`, `ws-events`, and later crates can name entities.

| Item | Provides |
|---|---|
| `EntityId` | A 64-bit identity (`NonZeroU64`): namespace bits 63–62 are `01` procedural or `10` spawned; `00`, `11`, and `0` are never valid. `procedural(hash)` keeps the hash's top 62 bits; `spawned(counter)` takes a 62-bit counter; `from_raw` validates; `raw` is the only external representation |
| `Column<C>` | The components of one type in ascending `EntityId` order: binary-search lookup, `insert` and `remove`, `insert_batch` (sort and merge, all or nothing) and `remove_batch`, ID-ordered `iter` and `iter_mut` |
| `join`, `join_mut` | Merge-joins over two columns, in ascending ID order |

## Owned state

None of its own; callers own columns.

## Emitted event types

None.

## Invariants

- Every iteration is in ascending `EntityId` (raw `u64`) order. A column's contents, and so every iteration over it, depend only on the set of `(id, component)` pairs, never on insertion or removal history (`tests/insertion_order.rs` builds the same columns in hundreds of orders).
- Inserting an ID that is already present is an error (`DuplicateEntity`); nothing is overwritten, and a failed batch changes nothing. A procedural ID collision therefore fails loudly; resolving one is deliberately not implemented (ADR 0016 §1).
- No storage index or handle leaves the crate. No scheduler, no threads, no `unsafe`, no dependencies.
- `EntityId` is stable: changing its layout breaks saves, events, and the FFI.

## Performance

`cargo bench -p ws-sim` measures columns of 5,000 entries with 64-byte components on the Linux machine used for TP-006 (Intel Xeon, 2.1 GHz): batch insert of 5,000 in about 0.28 ms (including the batch copy), one insert into 5,000 in about 0.1 ms (including cloning the column for each run), 714 lookups in about 10 µs (14 ns each), and a full iteration in about 2.5 µs.
