# Issue #75 Verification: Whole Slice Save and Load (VS-011, Save Format 4, ADR 0036)

## Summary of Changes

Implements accepted [ADR 0036](../../adr/0036-slice-persistence.md) for [#75](https://github.com/ndabney562/world-simulator/issues/75), fulfilling Vertical Slice Exit Criterion 3:

1. **Whole-Slice Runtime Persistence (`ws-persist`, `ws-sim`):**
   - Save format bumped to 4 (`FORMAT_VERSION = 4`, `PRAGMA user_version = 4`), with `model_kind` column in the header table.
   - Dynamic table discovery in `open()` distinguishes between Format 4 Toy Worlds and Format 4 Slice saves.
   - Serializes and restores all canonical slice components:
     - `slice_config`: seed, schedule epoch, and calendar start parameters.
     - `slice_vitals`: canonical resident vitals across all residents.
     - `slice_behavior`: per-resident anchor, routine progress, need deficits, and activity state.
     - `slice_lod`: active camera interest and per-resident LOD modes with transition times.
     - `slice_events`: append-only structured events log with causal links and confidence classes.
     - `slice_episodes`: active and resolved illness episodes.
     - `slice_operations`: causal interventions and user commands.
   - Backward compatibility: format 1, 2, and 3 saves and fixtures (`toy-v1.wssave`, `toy-v2.wssave`, `toy-v3.wssave`) load unchanged.
   - Implemented `save_slice`, `save_slice_parts`, `save_slice_background`, `load_slice`, and `load_slice_parts`.
   - Generated and committed golden v4 fixture in `tests/saves/slice-v4.wssave` with reproduction script `tests/saves/slice-v4.script`.

2. **Off-Thread Background Saving:**
   - Copy-on-write snapshot written in background on a dedicated thread without stalling simulation or rendering loops.
   - Thread handle tracked on `WsSliceHandle`, auto-joined on handle destruction or queried via `ws_slice_save_status`.

3. **FFI Bridge (`ws-ffi`):**
   - Added `ws_slice_save`, `ws_slice_save_background`, `ws_slice_save_status`, and `ws_slice_load`.
   - `SAVE_IN_PROGRESS = 8` status code returned when background save is in flight.
   - Updated C header checked against `cbindgen 0.29.2` and verified by `abi-v8-smoke.c`.

4. **Unity Integration (`apps/unity`):**
   - `NativeSlice.cs`: Added `Save`, `SaveBackground`, `SaveStatus`, and `Load` P/Invoke bindings. Refactored `Initialize(IntPtr pointer)` to share geometry/population initialization across creation and loading.
   - `SliceHost.cs`: Added `SliceHost(NativeSlice loadedSlice)`, static `SliceHost.Load(string path)`, and exposed `Save`, `SaveBackground`, and `SaveStatus`.
   - `SliceDemo.cs`: Added HUD "Save" and "Load" buttons, asynchronous background save polling in `Update()`, and HUD notice updates.
   - Added EditMode tests in `SliceBridgeTests.cs` verifying round-trip state hash reproduction, background save status, and `SliceHost` integration.

5. **CLI Integration (`ws-cli`):**
   - Added `--slice` option to `ws-cli save`.
   - Added auto-detection of slice save files to `ws-cli load`.
   - Added CLI save and load step to CI workflow covering both the toy world and the slice.

## Verification Checks Run Locally

All checks from [`docs/process/ci.md`](../../process/ci.md) were executed locally and passed:

- `python3 scripts/check_docs.py`
- `python3 scripts/check_unity_project.py`
- `python3 -m unittest discover -s scripts -p 'test_*.py'`
- `cargo +1.97.0 fmt --manifest-path core/Cargo.toml --all --check`
- `cargo +1.97.0 clippy --manifest-path core/Cargo.toml --workspace --all-targets --locked -- -D warnings`
- `scripts/check-ffi-header.sh`
- `cargo +1.97.0 test --manifest-path core/Cargo.toml --workspace --locked`
- `python3 scripts/check_core_deps.py`
- CLI save and load for both toy world and slice (`ws-cli save --slice ...` and `ws-cli load ...`)
- Native ABI C smoke test (`core/crates/ws-ffi/tests/abi-v8-smoke.c`)
