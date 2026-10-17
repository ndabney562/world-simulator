# ws-persist

## Responsibility

Saves a `ws-sim` `World` to a versioned SQLite file and loads it back ([ADR 0020](../../../docs/adr/0020-save-format-and-versioning.md), TP-009). Workspace layer 4: it builds and reads a `World`, so it sits above `ws-sim`; `ws-ffi` does not depend on it, and save/load over the FFI is out of scope. It is the only core crate that touches the file system besides the `ws-cli` host.

## Owned state

None. It reads `World::to_parts()` and rebuilds with `World::from_parts()`; the save file is the only thing it writes. The one-row `header` records everything D-14 requires from the first save: format version, build, world seed, dataset version, generator version, rules version, simulation time and tick, the simulation-affecting settings (walk speed, wait steps, population range), and three digests (state, journal, events).

## Emitted event types

None.

## File layout (format 2; format 1 still loads)

Format 2 (TP-010, [ADR 0021](../../../docs/adr/0021-god-intervention-contract.md) §7) adds the tables `illness` (resident ID → kind, severity, the `life.illness` event that explains it) and `scheduled` (journal sequence → interventions waiting for their time), journal action tag 3, event payload tags 4 to 6, rejection tags 4 to 6, and `effect_vocabulary` in the header. A format-1 file lacks the two tables and loads with none of their rows; the loader refuses an illness whose event is missing or belongs to another resident. Writers always write format 2.

The tables below are those of format 1:

`PRAGMA application_id` is ASCII `WSAV` and `PRAGMA user_version` is the format version, so a file can be recognised and refused before any table is read. One SQLite file per save slot. Every record is one row holding a `schema` version and a `data` blob written by the hand-written codec in `src/codec.rs` (little-endian, fixed width, floats as raw bits, one-byte enum tags the decoder checks).

| Table | Key | Content |
|---|---|---|
| `header` | single row | versions, seed, time, settings, digests |
| `clock` | single row | clock fields, next enqueue sequence, flush ordinal |
| `buildings` | (cell, entity ID) | building, its box and simulation level |
| `residents` | entity ID | address and home |
| `l1_state` | (cell, entity ID) | position and movement of an L1 agent |
| `l2_state` | entity ID | schedule of an L2 agent |
| `journal` | sequence | one command outcome, rejection reasons included |
| `events` | event ID | the event record without its links |
| `event_links` | (event ID, kind, index) | causes and related links |

Entity IDs and cell IDs are stored with their sign bit flipped, so SQLite's signed integer order equals the `u64` order and range scans follow Hilbert order. Derived indices (the L2 event queue, the unreconciled-residency set, the event store's time and actor indices) are not saved; they are rebuilt on load.

## Invariants

- A save is written to `<path>.saving` in one transaction and renamed over `path` only when complete, so `path` holds the previous file or the new one, never half of either.
- Saving requires a boundary with no pending commands (`SaveError::PendingCommands`), so no queued input is dropped.
- Loading never replays the journal. It decodes the rows, rebuilds the world, runs every invariant a running world keeps (`ToyWorld::check_lod_invariants`, the event store's rebuild from its records, journal and clock consistency), then compares the state, journal and events digests with the header. Any failure is a typed error and no partial world is returned.
- A file with another `application_id` is `NotASave`. A format version this build does not know (zero, or newer) is `UnsupportedVersion` before any table is read. Missing tables or rows, undecodable records, and key columns that disagree with their record are `Corrupt`.
- Tier 2: the digests are stable within one build and platform. They are corruption checks, not a cross-build promise. The loaded state itself (positions, schedules, IDs, events) is restored from stored values, so a save written on one platform loads on another; the committed fixture is loaded by both CI platforms.

## Tests and fixture

`cargo test -p ws-persist`: round trips at L1, L2 and mixed; save at tick T, load, run N more ticks against an uninterrupted run with a checkpoint compared on every tick; events, causal links, journal and rejected commands identical after load; header contents; unknown, newer and zero format versions; non-save files; missing and damaged rows; pending commands; atomic replace; codec round trips for every enum variant. `tests/saves/toy-v1.wssave` at the repository root is the committed format-1 fixture and `the_committed_fixture_loads` loads it in CI; `toy-v2.wssave` (an intervention in effect, one waiting, one rejected) is the format-2 fixture, `the_format_2_fixture_loads`. `tests/intervention.rs` covers illness, events and links after load, a waiting intervention, and a tampered illness. When the format changes, keep the old fixture and add a migration test for it.

## Performance

`cargo test --release -p ws-persist --test roundtrip measure_save_and_load -- --ignored --nocapture`, median of seven runs after a warm-up, on the Linux Xeon used for the other core measurements, 3,600 ticks into a run with one of five buildings at L2:

| Agents | Save | Load | File | Events |
|---|---|---|---|---|
| 50 | 1.6 ms | 0.47 ms | 40 KB | 24 |
| 5,000 | 17.6 ms | 16.0 ms | 975 KB | 1,972 |

A 5,000-agent save is two to three frames of the 8 ms simulation budget, so autosave on device will need the background write that 14-persistence §15.3 describes (copy canonical state at a tick boundary, write on another thread). That is out of scope here and is measured on device in TP-011. Load time is not on the frame path.
