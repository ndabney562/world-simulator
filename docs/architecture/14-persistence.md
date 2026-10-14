# 15. Persistence and Save Architecture

> **Revision 2 (2026-10-14):** Saves now also record the **dataset version**. Photoreal visuals need a network connection, while the simulation still runs offline, and a CDN serves the processed open-data tiles. See [22 §R.7](22-real-earth-and-realism.md#r7-revised-persistence-and-connectivity).

Covers point 15.

## 15.1 What a save contains

Following the seed-plus-delta model ([08](08-world-state.md)):

| Part | Contents |
|---|---|
| Header | Save format version, game build, world seed, generator versions, rules version, simulation time, settings that affect simulation |
| Canonical aggregates | Coarse state of every node that differs from its baseline (most nodes at L5+ do, because they are simulated; most fine nodes do not) |
| Canonical entities | All notable entities and their components; ID counters |
| Deltas | Per-node changes to the procedural baseline (structures, land use, overrides) |
| History log | Significant events, compacted per retention policy |
| Journal | Commands since the last full snapshot (player actions, interventions) |
| Snapshots | Periodic full canonical snapshots, for fast load, undo, and replay |
| Observation cache | Optional; may be dropped on save to keep size down |

A save does **not** contain procedural baseline content, derived data (indices, navigation graphs), or anonymous materialized entities.

## 15.2 Storage technology

- **SQLite** as the container on device: transactional, crash-safe, well supported on iOS, queryable for inspectors, and handles partial loading by key. Accessed from Rust via `rusqlite`.
- Data layout: one table per kind of record (nodes, entities, deltas, events, journal, snapshots), keyed by IDs and by **Hilbert-ordered cell IDs** ([07](07-coordinates-and-time.md)) so spatially close data is close on disk.
- Bulk component data stored as **compressed binary blobs per node** (for example a district's entity table), using a versioned, schema-evolving binary format and fast compression (LZ4 or zstd). The exact format (custom with `serde`, FlatBuffers, Cap'n Proto, ...) is postponed to a Phase 0 spike; the requirement now is that every record carries a schema version.
- One SQLite file per save slot.

## 15.3 Saving

- **Autosave is incremental**: only dirty nodes and new journal entries are written.
- Saves are taken at tick boundaries. The core produces a **copy-on-write snapshot** of dirty canonical state (cheap, because canonical state is a small fraction of memory) and a background thread serialises and writes it, so saving never hitches the frame.
- iOS lifecycle: on entering background, flush the journal and dirty nodes immediately within the time iOS allows; the journal guarantees that at worst the last few seconds are replayed from the previous state.

## 15.4 Loading

1. Read the header; check versions (see 15.5).
2. Load macro state (system, planet, settlements at L5+), which is small.
3. Load the nodes around the saved camera position and materialize them through the normal pipeline ([12](12-materialization.md)).
4. Everything else is loaded lazily as the camera moves.

Load time therefore depends on the area around the camera, not on world size.

## 15.5 Versioning and migration

From the first saveable build:
- **Save format version** with forward migration steps (version N to N+1), tested against stored fixture saves in CI ([18](18-testing-strategy.md)).
- **Rules version** and **generator versions** recorded. If a newer build changes a generator, the save either keeps using the old generator version (if retained) or is migrated by materializing and storing the affected content first. Policy is postponed; the recording is not.
- Determinism caveat: replaying a journal requires the same simulation code. Journals are therefore replayable only within a compatible build; long-term saves rely on snapshots, not on replaying the journal from world creation.

## 15.6 Size management

- Notability budgets and retirement ([09](09-entities-and-agents.md)) bound entity growth.
- History log compaction: old insignificant events are rolled into statistics.
- Journal truncation: once a full snapshot exists, older journal entries are kept only as long as undo/replay needs them.
- Target (hypothesis, to measure): a typical save stays in the tens of megabytes after many hours of play.

## 15.7 Later

- **iCloud sync** of save slots (simple file-level sync is enough for single-player).
- **Shareable worlds**: because a world is a seed plus deltas, sharing a world or a scenario is small.
- **Replays** as a feature (watch the history of a city) built on snapshots plus journal.
