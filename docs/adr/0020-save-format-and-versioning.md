# ADR 0020: Save container, component serialization (S-2), and version header (D-14)

- Status: Accepted
- Date: 2026-10-17
- Decided by: Noah (accepted 2026-10-17 in the project thread); proposal by Claude for TP-009
- Issue: #10 (TP-009; the ADR PR lands first, per its scope)
- Supersedes: none

## Context
TP-009 saves and loads the toy world. [14-persistence](../architecture/14-persistence.md) §15.2 fixes SQLite via `rusqlite` as the container and leaves the blob format to this spike (S-2). [21 D-14](../architecture/21-decisions-now-vs-later.md) requires save-format, rules, generator and dataset versions to be recorded from the first save, because they cannot be added retroactively. Constraints from accepted ADRs:

- Loading restores canonical state directly and never replays the journal (§15.5; ADR 0010). Derived indices (`queue`, `unreconciled`, the event store's `time` and `actors` indices) are rebuilt, not saved (§15.1; ADR 0017 §10).
- Event IDs, causal links, and the journal are identical after load (ADR 0019; [24 §24.10](../architecture/24-world-events-and-causality.md) invariant 4).
- Dependencies must be free and permissively licensed (ADR 0007). Nothing under `core/` may depend on a platform or provider.
- Today no core crate uses `serde`. Core types have private fields, are mostly `Copy`, and already have hand-written canonical encodings (`ws-sim` command bytes, `ws-events` word-wise digests).

Measured facts that shape the options: the toy world is small (a handful of buildings, up to 5,000 residents), and TP-009 stores everything as canonical state (baseline-plus-deltas is out of scope).

## Options considered
1. **`serde` with `postcard`** (MIT/Apache-2.0). Compact, stable documented wire format, little code. Costs: `serde` and `serde_derive` become dependencies of the foundation and world crates; derives expose private fields and constructors to a generic framework; the format is not self-describing, so schema evolution still needs a per-record version and hand-written migrations. Field order in a derive is a silent compatibility hazard.
2. **FlatBuffers** (Apache-2.0). Real forward/backward schema evolution and zero-copy reads. Costs: a `flatc` code generator in every contributor's and CI's toolchain, generated code in the repository or build, and zero-copy buys nothing for a load that rebuilds Rust structures anyway.
3. **Cap'n Proto** (MIT). Similar evolution story; needs the `capnp` compiler and a larger runtime. Same objections as FlatBuffers.
4. **Hand-written explicit codec in `ws-persist`**: little-endian, fixed-width fields, one schema version per record kind, decode functions per version. No new codec dependency. Costs: more code and per-type round-trip tests, and every new field needs a deliberate version bump.

## Decision
**Container.** One SQLite file per save slot, written through `rusqlite` with the `bundled` feature (MIT; SQLite itself is public domain), so the SQLite version is pinned by the lockfile and identical on every platform. The writer runs in one transaction at a tick boundary; a save is complete or absent.

**Serialization (S-2): option 4, a hand-written canonical codec.** Each record kind has an explicit byte layout (little-endian, fixed-width integers, `f64` as raw IEEE-754 bits, no padding, no platform-dependent sizes) and its own `schema_version: u32` stored beside the blob. The reasons are specific to this codebase: the types are private and `Copy`, the repository already encodes things this way, no existing crate gets a `serde` dependency, and the evolution mechanism (version plus migration function) is the same for every option here because none of the compact formats is self-describing. A decode that finds a newer version than the build knows fails with a typed error; it never guesses. Revisit if the number of record kinds grows past what hand-written code can keep tested (expected around the Neighbourhood MVP, when baseline-plus-deltas arrives).

**Tables (v1).**

| Table | Key | Content |
|---|---|---|
| `header` | single row | version header, below |
| `buildings`, `residents` | cell ID (Hilbert order), then entity ID | canonical identity and level |
| `l1_state`, `l2_state` | entity ID | position/walk, or background state; exactly one per resident, as the node level says |
| `journal` | sequence | one outcome per command, as ADR 0019 defines it |
| `events` | event ID | the full event record, causal links included |
| `event_links` | (event ID, ordinal) | causes and related links, so history queries can range-scan |
| `clock` | single row | tick, time, scale, sub-step remainder, enqueue sequence and event-ID counters |
| `pending` | sequence | commands accepted but not yet flushed (empty when saved at a boundary after a flush; rejected as an error otherwise) |

Every row carries the schema version of its record kind. Derived indices are not stored and are rebuilt on load.

**Version header (D-14).** Recorded from the first save: save format version; build identifier (crate version and git commit if available, otherwise a stated placeholder); world seed; dataset version; generator version; rules version; simulation time and tick; the settings that affect simulation (population range, walk parameters). The generator and rules versions are the values `World::new` already writes into `WorldCreated`, so one source feeds both.

**Load validation.** After decoding, the loader rebuilds derived indices, re-checks the existing invariants (the ADR 0017 invariant checker and the event-store consistency checks), and compares the recomputed `state_hash` with the hash stored in the header. A mismatch is a typed `Corrupt` error, not a partially loaded world. The stored hash is Tier 2 (same build and platform) and is a corruption check only; correctness across builds rests on the decoded values, not on the hash.

**Crate placement.** `ws-persist` must construct a `ws-sim::World`, so it cannot sit in layer 2 as the current table lists it. It moves to its own layer between `ws-sim` and the host adapters, and `ws-ffi` does not depend on it (save/load over the FFI is out of scope). The implementation PR changes `scripts/check_core_deps.py` and `docs/process/ci.md` together, and adds the minimal restore constructors (validated, crate-visible or public with a documented contract) that `ws-world`, `ws-events` and `ws-sim` need. No simulation behaviour changes.

**Dependencies added by the implementation:** `rusqlite` with `bundled` (MIT) and its `libsqlite3-sys` build of SQLite (public domain). Building SQLite from C source needs a C compiler, which the Linux and macOS CI runners already have. No codec crate is added.

## Consequences
- Easier: no new serialization framework, a single auditable byte layout per record, a versioned format from the first save, and saves that stay valid when private fields are renamed.
- Harder: every new persisted field is hand-coded, versioned and tested; migrations are explicit code. The codec needs a thorough round-trip and fixture test suite (TP-009 requires the fixture).
- Affected: new crate `ws-persist` and `ws-cli` subcommands; small additions to `ws-world`, `ws-events`, `ws-sim`; `check_core_deps.py` and `ci.md` layer table; [14-persistence](../architecture/14-persistence.md) and [21](../architecture/21-decisions-now-vs-later.md) point to this ADR.
- Not decided here: baseline-plus-deltas saves, incremental or background saving, compression, undo snapshots (all excluded by TP-009), and the migration policy for dataset, generator and rules changes (chosen at the first breaking change; only the recording is required now).
- Revisit if: record kinds multiply, the toy world's save exceeds a size or time budget measured in the TP-009 PR, or a framework with schema evolution becomes clearly cheaper than maintaining the codec.
