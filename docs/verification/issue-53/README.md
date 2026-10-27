# Issue #53 Verification: Baseline-Plus-Deltas Saves (Save Format 3, ADR 0025)

## Summary of Changes

Implements accepted [ADR 0025](../../adr/0025-baseline-plus-deltas-saves.md) for [#53](https://github.com/ndabney562/world-simulator/issues/53):

1. **Dataset Canonical Bytes and Content SHA-256 (`ws-world`):**
   - Added `TOY_DATASET_ID = 1`, `BUILDING_GENERATOR_VERSION = 1`, `RESIDENT_GENERATOR_VERSION = 1`, and `IDENTITY_POLICY_FAIL_ON_COLLISION = 1`.
   - Added `Dataset::canonical_bytes()` and `Dataset::content_sha256()` backed by a zero-dependency FIPS 180-4 `sha256` implementation tested against NIST FIPS 180-4 vectors.
2. **Baseline Structural Digest and Canonical Deltas (`ws-world`, `ws-events`, `ws-sim`):**
   - Added `PURPOSE_STRUCTURAL_DIGEST` (`"ws-world/structural-digest"`) and `ToyWorld::structural_digest()`.
   - Added `BaselineAddress`, `Tombstone`, `notables: BTreeSet<EntityId>`, `tombstones: Column<Tombstone>`, and `next_spawned_id: u64` to `ToyWorld` and `ToyParts`.
   - Extended `ToyWorld::state_hash` with optional domain tags 8 (`notables`), 9 (`tombstones`), and 10 (`next_spawned_id`) only when non-empty/non-zero, preserving exact state hashes for all existing worlds and format-1/2 fixtures.
   - Added structural delta operations on `ToyWorld` and `World`: `override_building`, `override_resident`, `promote_notable`, `tombstone_resident`, `tombstone_building`, `spawn_building`, and `spawn_resident`.
   - Updated `World::from_parts` to pass tombstoned entity IDs to `EventStore::from_records` so historical events referencing tombstoned actors remain valid while unknown/forged actor IDs and live references to tombstoned entities are rejected.
3. **Save Format 3, Baseline Registry, and Atomic Migration (`ws-persist`):**
   - Advanced `FORMAT_VERSION` to `3` while keeping Format 1 (`tests/saves/toy-v1.wssave`) and Format 2 (`tests/saves/toy-v2.wssave`) readers intact.
   - Added `BaselineRecord` (with `PURPOSE_BASELINE_DIGEST` `"ws-persist/baseline-digest"`), `BaselineRegistry`, `LoadError::MissingBaseline`, and `LoadError::UnsupportedBaseline`.
   - Replaced raw `buildings` and `residents` baseline persistence with `baseline`, `structural_overrides`, `tombstones`, `notables`, `spawned`, and `node_lod`, and extended `clock` to schema 2 (`next_spawned_id`, `next_event_id`).
   - Added `migrate_to_v3` and `migrate_to_v3_with_registry` for atomic migration from format 1/2/3 to format 3 without modifying the source file.
   - Added committed format-3 fixture `tests/saves/toy-v3.wssave` (`tests/saves/toy-v3.script`).

## Reproduce the Measurements

From the repository root:

```sh
cargo test --manifest-path core/Cargo.toml --release -p ws-persist --test baseline_deltas measure_v3_save_and_load_and_row_counts -- --ignored --nocapture
```

## Measured Results (macOS arm64, release profile)

### Fixture Comparison (`seed = 2026`, `tick = 600`)

| Fixture | Format | File Size | `buildings` + `residents` rows | `baseline` rows | `structural_overrides` rows | `node_lod` rows |
|---|---|---|---|---|---|---|
| `tests/saves/toy-v2.wssave` | 2 | 49,152 B (48 KB) | 44 (`5` + `39`) | — | — | — |
| `tests/saves/toy-v3.wssave` | 3 | 65,536 B (64 KB) | 0 | 1 | 0 | 5 |

*(At 39 residents, SQLite's 4 KB page allocation per created table dominates total file size because Format 3 defines 15 tables vs 11 in Format 2; at 5,000 residents below, eliminating the 5,000 resident identity rows reduces the file size by ~18%.)*

### 50 and 5,000 Agent Medians (7 runs after warm-up, 3,600 ticks, mixed L1/L2)

| Agents | Format | `baseline` rows | `structural_overrides` rows | `node_lod` rows | `l1_state` rows | `l2_state` rows | Events | Save Median | Load Median | File Size |
|---|---|---|---|---|---|---|---|---|---|---|
| 50 | 3 | 1 | 0 | 5 | 36 | 14 | 24 | 6.06 ms | 1.26 ms | 65,536 B (64 KB) |
| 5,000 | 3 | 1 | 0 | 5 | 3,808 | 1,192 | 1,972 | 37.68 ms | 30.11 ms | 823,296 B (804 KB) |

## Verification Checks Run Locally

All checks from [`docs/process/ci.md`](../../process/ci.md) were executed locally and passed:

- `python3 scripts/check_docs.py`
- `python3 scripts/check_unity_project.py`
- `python3 -m unittest discover -s scripts -p 'test_*.py'`
- `cargo fmt --manifest-path core/Cargo.toml --all --check`
- `cargo clippy --manifest-path core/Cargo.toml --workspace --all-targets --locked -- -D warnings`
- `cargo build --manifest-path core/Cargo.toml --workspace --all-targets --locked`
- `cargo test --manifest-path core/Cargo.toml --workspace --locked`
- `python3 scripts/check_core_deps.py`
