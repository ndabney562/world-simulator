# ws-persist

## Responsibility

Saves a `ws-sim` `SliceSimulation` to a versioned SQLite file and loads it back ([ADR 0020](../../../docs/adr/0020-save-format-and-versioning.md), [ADR 0036](../../../docs/adr/0036-slice-persistence.md), #75). Workspace layer 4: it builds and reads a `SliceSimulation`, so it sits above `ws-sim`; `ws-ffi` calls it for the slice save/load bridge. It is the only core crate that touches the file system besides the `ws-cli` host.

Current save format: **6** (`FORMAT_VERSION` in `src/header.rs`). It is the only format this build reads or writes: older formats, including every toy-world format, are refused with `LoadError::UnsupportedVersion` ([ADR 0040](../../../docs/adr/0040-delete-toy-world-and-old-formats.md), #139).

## Owned state

None. It reads `SliceSimulation::to_parts()` and rebuilds with `SliceSimulation::from_parts()`; the save file is the only thing it writes. The one-row `header` records the format version, build, seed, dataset, generator and rules versions, simulation time and tick, the population size, three digests (state, operation journal, events), the effect vocabulary (`EFFECT_VOCABULARY`, 1) and the model kind (`MODEL_KIND_SLICE`, 2; kind 1 was the deleted toy). The `walk_speed_bits` and `walk_wait_steps` columns are written as 0.

## Emitted event types

None.

## File layout

`PRAGMA application_id` is ASCII `WSAV` (`0x5753_4156`) and `PRAGMA user_version` is the format version, so a file can be recognised and refused before any table is read. One SQLite file per save slot. Most records are one row holding a `schema` version and a `data` blob written by the hand-written codec in `src/codec.rs` (little-endian, fixed width, one-byte enum tags the decoder checks).

| Table | Key | Content |
|---|---|---|
| `header` | single row (`id = 1`) | versions, build, seed, time, tick, population, digests, effect vocabulary, model kind |
| `slice_config` | single row | seed, start and model-week epoch |
| `clock` | single row (schema 3) | clock fields and the next event ID; the toy-era command counters are written as 0 |
| `slice_vitals` | `resident_id` | sex, health and life status |
| `slice_illness` | `resident_id` | active illness episode |
| `slice_behavior` | `resident_id` | needs, action, routine and storm hold |
| `slice_lod` | single row | camera controller and node levels |
| `events` | `id` | the event record without its links |
| `event_links` | `(event_id, kind, idx)` | causes and related links |
| `slice_operations` | `idx` | the operation log |
| `slice_storm`, `slice_gathering` | optional single row | active power episodes (below) |
| `slice_faith` | `resident_id` | faith in −100..=100 |
| `slice_goals` | single row (`id = 0`) | the day's goals |

Entity IDs are stored with their sign bit flipped, so SQLite's signed integer order equals the `u64` order. Derived indices are not saved; they are rebuilt on load.

Format 5 ([ADR 0038](../../../docs/adr/0038-bless-storm-gathering-powers.md) §6, #125) is format 4 plus two single-row slice tables: `slice_storm` (the storm episode's root, start, end and kept count, and the `ws-life` storm window, each optional) and `slice_gathering` (building, root, start, end, joined count, then up to seven attendees of resident ID and phase). The ws-life window outlives the storm episode until every hold rejoins, so the two are saved separately. The gathering row keeps its `joined` counter and each attendee's phase. Format-5 `slice_behavior` rows append each resident's storm hold, its storm root and phase included (plan decision D1, as amended), and the window carries the storm root too, and override kind 3 (gathering) becomes valid.

Format 6 ([ADR 0039](../../../docs/adr/0039-faith-and-daily-goals.md) §4, #127) is format 5 plus two slice tables: `slice_faith` (one row per retained resident, `resident_id` and `faith` in −100..=100, ascending) and `slice_goals` (one row, `id = 0`, with the goal `day` and a blob: goal count, then per goal kind, target, progress and done; the distinct blessed residents for kind 2; then the gathering being counted, building and joins, if any). The loader refuses unknown kinds, duplicate kinds, an open goal at or past its target, duplicate blessed residents, faith out of range, unsorted faith rows and trailing bytes, and the restore refuses a faith roster or goal day that disagrees with the slice. Event tag 23 `GoalCompleted` is saved from format 6.

## Invariants

- A save is written to `<path>.saving` in one transaction and renamed over `path` only when complete, so `path` holds the previous file or the new one, never half of either.
- Loading never replays operations. It decodes the rows, rebuilds the simulation through `from_parts` (which checks every invariant a running simulation keeps), then compares the state hash, operation digest and events digest with the header. Any failure is a typed error and no partial simulation is returned.
- A file with another `application_id` is `NotASave`. Any format version other than 6 is `UnsupportedVersion` before any table is read. A header whose model kind is not the slice, missing tables or rows, undecodable records, and key columns that disagree with their record are `Corrupt`.
- Tier 2: the digests are stable within one build and platform. They are corruption checks, not a cross-build promise.

## Tests and fixtures

`cargo test -p ws-persist`: round trips at tick 0, after illness, LOD and camera changes, with irregular host pacing, after rejected operations, with active and finished storms and gatherings, and with partial goals and faith; save, load and step against an uninterrupted run; atomic background saves; corrupt, tampered and non-slice files; codec round trips. `tests/saves/slice-v4.wssave` at the repository root is a format-4 file kept only to prove that older formats are refused (here and in `ws-cli`); it cannot be regenerated.
