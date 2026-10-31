# ws-persist

## Responsibility

Saves a `ws-sim` `World` to a versioned SQLite file and loads it back ([ADR 0020](../../../docs/adr/0020-save-format-and-versioning.md), [ADR 0025](../../../docs/adr/0025-baseline-plus-deltas-saves.md), TP-009, #53). Workspace layer 4: it builds and reads a `World`, so it sits above `ws-sim`; `ws-ffi` does not depend on it, and save/load over the FFI is out of scope. It is the only core crate that touches the file system besides the `ws-cli` host.

## Owned state

None. It reads `World::to_parts()` and rebuilds with `World::from_parts()`; the save file is the only thing it writes. The one-row `header` records everything D-14 requires from the first save: format version, build, world seed, dataset version, generator version, rules version, simulation time and tick, the simulation-affecting settings (walk speed, wait steps, population range), and three digests (state, journal, events).

## Emitted event types

None.

## File layout (format 3; formats 1 and 2 still load)

Format 3 (#53, [ADR 0025](../../../docs/adr/0025-baseline-plus-deltas-saves.md)) replaces the raw `buildings` and `residents` structural baseline tables with a version-pinned `baseline` table plus canonical structural delta tables (`structural_overrides`, `tombstones`, `notables`, `spawned`) and a separate `node_lod` table for building simulation levels, while extending `clock` to schema 2 (`next_spawned_id` and `next_event_id`). Format-1 (`tests/saves/toy-v1.wssave`) and format-2 (`tests/saves/toy-v2.wssave`) files still load directly through the legacy reader, and `migrate_to_v3` / `migrate_to_v3_with_registry` converts a format-1/2/3 file atomically to a new format-3 file without modifying the source file. Migration resolves filesystem aliases, writes in a freshly reserved sibling directory, validates the complete candidate before atomically replacing the destination, and cleans only its owned temporary files. Source paths matching the former destination `.saving` filename remain untouched. Writers always write format 3.

`PRAGMA application_id` is ASCII `WSAV` (`0x5753_4156`) and `PRAGMA user_version` is the format version (`3`), so a file can be recognised and refused before any table is read. One SQLite file per save slot. Every record is one row holding a `schema` version and a `data` blob written by the hand-written codec in `src/codec.rs` (little-endian, fixed width, floats as raw bits, one-byte enum tags the decoder checks).

| Table | Key | Content |
|---|---|---|
| `header` | single row (`id = 1`) | format/generator/rules/effect versions, build, seed, time, settings, state/journal/events digests |
| `baseline` | single row (`id = 1`) | seed, dataset ID & version, 32-byte content SHA-256, building/resident/rules/identity-policy versions, generation inputs, start time, baseline entity counts, `structural_digest`, `baseline_digest`, `collision_overrides` |
| `clock` | single row (`id = 1`, schema 2) | clock fields, next enqueue sequence, flush ordinal, `next_spawned_id`, `next_event_id` |
| `structural_overrides` | `(cell, id, component_tag)` | changed structural component (`1 = Building`, `2 = Resident`) of a non-notable, non-tombstoned procedural baseline entity |
| `tombstones` | `id` | deleted procedural baseline entity (`BaselineAddress`, `deleted_tick`, `root_event`) |
| `notables` | `id` | promoted procedural baseline entity with its complete canonical `Building` or `Resident` record |
| `spawned` | `id` | runtime-spawned entity (`Namespace::Spawned`, payload `< next_spawned_id`) with its complete canonical record |
| `node_lod` | `id` | simulation level (`Lod`) of each live building |
| `l1_state` | `(cell, id)` | position and movement of an L1 agent |
| `l2_state` | `id` | schedule of an L2 agent |
| `illness` | `id` | resident illness (`kind`, `severity`, `root_event`) |
| `scheduled` | `sequence` | scheduled interventions waiting for their firing time |
| `journal` | `sequence` | one command outcome, rejection reasons included |
| `events` | `id` | the event record without its links |
| `event_links` | `(event_id, kind, idx)` | causes and related links |

Entity IDs and cell IDs are stored with their sign bit flipped, so SQLite's signed integer order equals the `u64` order and range scans follow Hilbert order. Derived indices (the L2 event queue, the unreconciled-residency set, `by_home`, the event store's time and actor indices) are not saved; they are rebuilt on load.

## Invariants

- A save is written to `<path>.saving` in one transaction and renamed over `path` only when complete, so `path` holds the previous file or the new one, never half of either. `migrate_to_v3` uses the same staging-and-rename guarantee and never modifies the source path.
- Saving requires a boundary with no pending commands (`SaveError::PendingCommands`), so no queued input is dropped.
- Format 3 verifies `baseline_digest`, checks `building_generator_version`, `resident_generator_version`, `rules_version`, `identity_policy_version`, and `collision_overrides` (`LoadError::UnsupportedBaseline`), resolves `(dataset_id, dataset_version)` in `BaselineRegistry` and verifies the full 32-byte `Dataset::content_sha256()` (`LoadError::MissingBaseline`), checks a shared reader/writer limit of1,000,000 total baseline entities and computes exact building/resident counts before generation, regenerates the procedural baseline with `ToyWorld::generate`, and verifies `building_count`, `resident_count`, and `ToyWorld::structural_digest()`.
- Structural delta ownership is mutually exclusive: a procedural baseline entity is either untouched (0 delta rows), overridden in `structural_overrides` (only when its component differs from the regenerated baseline), promoted in `notables` (owning its full structural record, with 0 `structural_overrides` rows), or deleted in `tombstones` (disjoint from all live structural and runtime tables, while registered in `EventStore` so historical actor references remain valid). Spawned entities (`Namespace::Spawned`, `payload < next_spawned_id`) live exclusively in `spawned`. Any overlap, redundant override, or unknown/out-of-range ID fails with `LoadError::Corrupt`.
- Loading never replays the journal. It decodes the rows, rebuilds the world, runs every invariant a running world keeps (`ToyWorld::check_lod_invariants`, the event store's rebuild from its records, journal and clock consistency), then compares the state, journal and events digests with the header. Any failure is a typed error and no partial world is returned.
- A file with another `application_id` is `NotASave`. A format version this build does not know (zero, or newer) is `UnsupportedVersion` before any table is read. Missing tables or rows, undecodable records, and key columns that disagree with their record are `Corrupt`.
- Tier 2: the digests are stable within one build and platform. They are corruption checks, not a cross-build promise. The loaded state itself (positions, schedules, IDs, events) is restored from stored values, so a save written on one platform loads on another; the committed fixtures (`toy-v1.wssave`, `toy-v2.wssave`, `toy-v3.wssave`) are loaded by both CI platforms.

## Tests and fixtures

`cargo test -p ws-persist`: round trips at L1, L2 and mixed; save at tick T, load, run N more ticks against an uninterrupted run with a checkpoint compared on every tick; events, causal links, journal and rejected commands identical after load; header and baseline contents; zero structural delta rows on unchanged worlds; `structural_overrides`, `notables`, `tombstones` (including historical event actor retention and forged actor rejection), and `spawned` round trips; `MissingBaseline` on missing or 1-bit altered datasets; `UnsupportedBaseline` on unknown generator/rules/policy versions or collision overrides; duplicate/overlapping delta ownership rejections; atomic migration from formats 1 and 2 to format 3; unknown, newer and zero format versions; non-save files; missing and damaged rows; pending commands; atomic replace; codec round trips for every enum variant. `tests/saves/toy-v1.wssave`, `tests/saves/toy-v2.wssave`, and `tests/saves/toy-v3.wssave` at the repository root are the committed format-1, format-2, and format-3 fixtures.

## Performance

`cargo test --release -p ws-persist --test baseline_deltas measure_v3_save_and_load_and_row_counts -- --ignored --nocapture`, median of seven runs after a warm-up on macOS arm64, 3,600 ticks into a run with one of five buildings at L2 ([verification report](../../../docs/verification/issue-53/README.md)):

| Agents | Format | Structural rows (`buildings`+`residents` vs `baseline`+`overrides`+`node_lod`) | Save median | Load median | File size | Events |
|---|---|---|---|---|---|---|
| 50 | Format 3 | 6 (`1` baseline + `0` overrides + `5` `node_lod`, vs `55` in format 2) | 6.06 ms | 1.26 ms | 64 KB (65,536 B) | 24 |
| 5,000 | Format 3 | 6 (`1` baseline + `0` overrides + `5` `node_lod`, vs `5,005` in format 2) | 37.68 ms | 30.11 ms | 804 KB (823,296 B, vs 975 KB in format 2) | 1,972 |

ADR0034 tags 10–13 are explicitly rejected by legacy toy save writing, alongside other slice-only payloads. Existing save formats/fixtures remain unchanged; real-slice episode/routine persistence belongs to #75.

Format 5 ([ADR 0038](../../../docs/adr/0038-bless-storm-gathering-powers.md) §6, #125) is format 4 plus two single-row slice tables: `slice_storm` (the storm episode's root, start, end and kept count, and the `ws-life` storm window, each optional) and `slice_gathering` (building, root, start, end, joined count, then up to seven attendees of resident ID and phase). Format-5 `slice_behavior` rows append each resident's storm hold, and override kind 3 (gathering) becomes valid. Writers write format 5 for slices and toys (the toy layout is unchanged); the loader reads formats 4 and 5, and a format-4 file loads with no storm, hold or gathering.
