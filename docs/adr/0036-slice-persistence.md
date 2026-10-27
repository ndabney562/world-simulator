# ADR 0036: Slice persistence in save format 4 and off-thread writing

- Status: Accepted
- Date: 2026-10-27
- Decided by: Noah, authorized on #75 and #77; proposed by Claude
- Issue: #75 (VS-011)
- Supersedes: none; extends ADR 0020 and ADR 0025

## Context

Issue #75 (VS-011) completes the persistence spine for the real Bay Ridge block (`SliceSimulation`) in save format 4, meeting Vertical Slice exit criterion 3: "Save, quit, and load reproduce the same state hash, including notables, deltas, and the event log."

[ADR 0020](0020-save-format-and-versioning.md) established the SQLite container, explicit little-endian codecs, and save format 1. [ADR 0021](0021-god-intervention-contract.md) added toy illness state in format 2. [ADR 0025](0025-baseline-plus-deltas-saves.md) introduced version-pinned baseline identity and canonical deltas in format 3 for the toy world.

The real slice (`SliceSimulation`) combines:
1. Pinned Tier 1 immutable baseline: 4 featured buildings + 54 context footprints (`PopulatedSlice`), 10 households, 10–30 residents, 3 workplaces, 2 gateways, initial vitals, schedules, and initial behavior.
2. Mutable authoritative runtime state:
   - Clock: steps, current SimTime, scale multiplier, host pacing remainder, next sequence, boundary, next event ID.
   - Resident vitals: sex, health (0–100), life status (Alive/Dead).
   - Finite illness episodes: resident, root event, illness event, severity, start time, end time, return_at, recovered flag, returned flag, missed count, visited flag.
   - Resident behavior: detail level (L1/L2), 4 integer need deficits (rest, hunger, social, duty), decision anchor SimTime, current Action, cumulative decision count, and temporary routine overrides (illness home hold, neighbour check-in visits, original/homeward journey endpoints, rejoin SimTime).
   - LOD camera policy: camera center (approximate plane mm) and radius, plus four featured building node modes and last-changed timestamps.
   - World event store: full structured event records with causal links and confidence classes.
   - Operation log: session operations, outcomes, and reproducible stop checkpoints.
3. Off-thread persistence:
   - Saving must not hitch the interactive render loop on device. Copy-on-write capture at a tick boundary serialises and writes to SQLite off the main thread.
4. Backward compatibility:
   - Format 1, 2, and 3 saves and fixtures (`toy-v1.wssave`, `toy-v2.wssave`, `toy-v3.wssave`) must continue loading with identical expected state hashes.

## Decision

### 1. Save Format Version 4

Reserve `user_version = 4` and `application_id = 0x57534156` (`WSAV`). Format-1, format-2, and format-3 readers remain supported and tested.

The `header` table is extended with an explicit model kind discriminator (`model_kind: u32`: `1` for Toy World, `2` for Slice Simulation). Older formats default to model kind 1.

### 2. Table Layout for Slice Saves

| Table | Primary Key | Record Schema | Content |
|---|---|---|---|
| `header` | `id` (single row) | 4 | Save format 4, git build ID, seed, dataset version (`BLOCK_6049_DATASET_VERSION = 0x8518_3751_d4f7_93b9`), generator versions, rules version, sim time, tick, state hash, journal/operation digest, events digest, model kind |
| `slice_config` | `id` (single row) | 1 | Seed, start SimTime, model week schedule epoch SimTime |
| `clock` | `id` (single row) | 2 | Steps, now SimTime, scale multiplier, host pacing remainder, next sequence, boundary, next event ID |
| `slice_vitals` | `resident_id` | 1 | Canonical resident ID, sex (0 Female, 1 Male), health (0–100), life status (0 Alive, 1 Dead) |
| `slice_illness` | `resident_id` | 1 | Finite episode: root event, illness event, severity, start, end, return_at, recovered, returned, missed, visited |
| `slice_behavior` | `resident_id` | 1 | Detail (1 L1, 2 L2), 4 need deficits, anchor SimTime, action, decision count, routine override |
| `slice_lod` | `id` (single row) | 1 | Camera interest (center east/north mm, radius mm), 4 featured building node modes and changed_at timestamps |
| `events` | `id` | 1 | Structured events with payload tags 0–13, including all slice payloads (`SliceCreated`, `SliceLodChanged`, `ScheduleChanged`, `SliceIllnessApplied`, `MissedDuty`, `NeighbourCheckIn`, `Recovered`, `ReturnedHome`) |
| `event_links` | `(event_id, ordinal)` | 1 | Causes and related links with relation and confidence classes |
| `slice_operations` | `index` | 1 | Ordered operation records (operation opcode, arguments, report outcome, stop checkpoint) |

Derived indices (consequence groups, observation cache, L2 transition queues, navigation graphs) are not saved; they are rebuilt deterministically on load.

### 3. State Restoration & Validation

Loading validates container, versions, and schemas before materializing entities:
1. Regenerates the pinned `PopulatedSlice` baseline from `config.seed`.
2. Restores `vitals`, validating living health bounds and unique roster membership.
3. Reconstructs `schedules` from the model week epoch.
4. Restores `behavior` resident states, evaluating derived schedule answers or routine overrides.
5. Restores `lod` Controller nodes and camera interest with empty observation cache.
6. Restores `clock` and `events` via `EventStore::from_records`.
7. Restores `illness` episodes and calls `rebuild_consequence_index()` to regenerate consequence groups directly from committed factual events.
8. Verifies that the restored `SliceSimulation::state_hash()` strictly equals the header's `state_hash`.

### 4. Off-Thread Background Saving

Saving captures a coherent copy-on-write snapshot (`SliceParts`) at a tick boundary. The SQLite transaction and file writing run on a background thread, writing to a `.saving` sibling file and atomically renaming it on success. The FFI exposes both synchronous (`ws_slice_save`) and non-blocking background save (`ws_slice_save_background` / `ws_slice_save_status`).

## Consequences

- The complete Vertical Slice runtime state round-trips through disk with exact state hash equality (VS exit criterion 3).
- Format 1, 2, and 3 saves continue loading without change.
- The Unity UI exposes Save and Load controls with asynchronous execution, avoiding frame hitches on device.
