# 15. Persistence and Save Architecture

Covers point 15. Related: [08](08-world-state.md) (what is canonical), [24](24-world-events-and-causality.md) (event store), [25](25-determinism-policy.md) (reproducibility).

## 15.1 What a save contains

| Part | Contents |
|---|---|
| Header | Save format version, game build, world seed, **dataset version**, generator versions, rules version, simulation time, settings that affect simulation |
| Canonical aggregates | Coarse state of every node that differs from its baseline, including intervention attributions |
| Canonical entities | Notable entities and their components (including provenance tags); ID counters |
| Deltas | Per-node changes to the baseline (structures, land use, overrides) |
| **World event store** | Structured events with causal links and indices, compacted by the retention policy ([24 §24.8](24-world-events-and-causality.md)) |
| Command journal | Commands since the last full snapshot (player actions, interventions, adopted LLM suggestions) |
| Snapshots | Periodic full canonical snapshots, for fast loading, undo, replay, and counterfactual branches |
| Observation cache | Optional; may be dropped on save |

A save does **not** contain: the baseline (simulation tiles and procedural content), derived data (indices, navigation graphs), anonymous materialized entities, or anything from visual providers ([23](23-visual-providers.md)).

## 15.2 Storage technology

- **SQLite** as the container on device (transactional, crash-safe, queryable for inspector and history queries, partial loading by key), accessed from Rust with `rusqlite`.
- Tables per kind of record (nodes, entities, deltas, events, causal links, journal, snapshots), keyed by IDs and by **Hilbert-ordered cell IDs**, so spatially close data is close on disk and region queries are range scans.
- Bulk component data stored as compressed binary blobs per node, in a versioned, schema-evolving format (the exact format is chosen in a Technical Prototype spike; decided in [ADR 0020](../adr/0020-save-format-and-versioning.md)). Every record carries a schema version.
- One SQLite file per save slot.

## 15.3 Saving

- **Autosave is incremental**: dirty nodes, new events, and new journal entries only.
- Saves are taken at tick boundaries. The core takes a copy-on-write snapshot of dirty canonical state, and a background thread serialises and writes it, so saving never causes a frame hitch.
- iOS lifecycle: on entering the background, flush the journal and dirty state immediately within the time iOS allows.

## 15.4 Loading

1. Read the header and check versions (15.5). Make sure the referenced dataset version is available (shipped with the app for the MVP area; fetched from static hosting later).
2. Load macro state (system, Earth, regions, cities), which is small.
3. Load the nodes around the saved camera position and materialize them through the normal pipeline ([12](12-materialization.md)).
4. Load everything else lazily as the camera moves.

Load time depends on the area around the camera, not on world size.

## 15.5 Versioning, migration, and reproducibility

- **Save format version** with forward migration steps, tested against fixture saves in CI ([18](18-testing-strategy.md)).
- **Dataset, generator, and rules versions** recorded. If a newer build changes any of them, the save keeps the old versions (old tiles kept available) or is migrated by materializing and storing affected content first. The policy is chosen at the first breaking change; the recording is required from day one.
- **Saves never depend on cross-platform replay.** Loading restores canonical state directly; the baseline is regenerated with Tier 1 exactness. Journals are replayed only on the same build and platform, for undo, debugging, and counterfactuals (Tier 2, [25](25-determinism-policy.md)).

## 15.6 Size management

- Notability budgets and retirement ([09](09-entities-and-agents.md)) bound entity growth.
- Event retention by importance with deterministic compaction into aggregate events; intervention causal chains are retained preferentially ([24 §24.8](24-world-events-and-causality.md)).
- Journal truncation: once a full snapshot exists, older journal entries are kept only as long as undo and replay need them.
- Target (a hypothesis, to be measured): a typical save stays in the tens of megabytes after many hours of play.

## 15.7 Later

- **iCloud sync** of save slots (file-level sync is enough for a single-player game; ADR 0003).
- **Shareable worlds**: a world is a seed, a dataset version, and deltas, so sharing a world or scenario is small.
- **Replays and timelines** as a player feature, built on snapshots, journal, and events.
