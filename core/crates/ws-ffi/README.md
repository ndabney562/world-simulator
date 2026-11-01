# ws-ffi

The only Rust library Unity links. Current ABI: **11** (`ABI_VERSION` in `src/lib.rs`), under accepted [ADR 0040](../../../docs/adr/0040-delete-toy-world-and-old-formats.md), extending ADR0018/0030/0031/0034/0038/0039. Owns the opaque slice handle (`WsSliceHandle`) and bounded diagnostics. Owns no independent history: typed operations enter ws-sim, which publishes outcomes and events at fixed boundaries.

## Contract

See `include/ws_ffi.h` for the generated C interface and [ADR 0018](../../../docs/adr/0018-rust-unity-bridge.md) for lifetimes. Callers must serialize calls and disposal for each handle. Caller buffers and out-parameters must be valid and non-overlapping; arbitrary or dangling pointers are not recoverable errors. Creation and handle-free errors use thread-local diagnostics, read with `ws_slice_last_error(NULL, ...)`; handle errors use the handle's own diagnostic. A caught panic poisons the handle: only diagnostics and destruction remain.

Every ABI-bearing request carries the exact ABI word 11 and its exact structure size. Any other ABI word is `VERSION_MISMATCH` and leaves outputs untouched. Status codes keep their ABI1 numeric meanings (`OK` 0 through `SAVE_IN_PROGRESS` 8).

## ABI 11 (ADR 0040)

ABI 11 deletes the toy world's exports: `ws_create`, `ws_destroy`, `ws_submit`, `ws_advance`, `ws_get_control_state`, `ws_fill_render_snapshot`, `ws_get_calendar_sky` and `ws_last_error`, with their structs (`WsConfig`, `WsControlState`, `WsSnapshotHeader`, `WsInstance`) and the toy handle. Every slice export, structure and byte layout is unchanged from ABI 10; only the ABI word moves to 11. `ws_slice_last_error(NULL, ...)` replaces `ws_last_error(NULL, ...)` for thread-local errors. Older ABI fixtures and C smoke sources are deleted; tests refuse every older ABI word instead. `tests/abi-v11-smoke.c` is the C consumer CI compiles, and the [fixtures](tests/fixtures/README.md) are ABI 11 only.

## Build and verify

From repository root with the pinned Rust toolchain installed:

```sh
scripts/build-native-macos.sh
cargo install cbindgen --version 0.29.2 --locked
scripts/check-ffi-header.sh
cd core
cargo test --locked -p ws-ffi
```

Header generation (run in `core/`): `cbindgen --config crates/ws-ffi/cbindgen.toml --crate ws-ffi --output crates/ws-ffi/include/ws_ffi.h`. `cbindgen` 0.29.2 (MPL-2.0) is a build tool only, not shipped or linked into the app. Runtime dependencies are ws-sim and ws-persist. CI verifies generated-header freshness, compiles and runs `tests/abi-v11-smoke.c` against the host library, and builds `aarch64-apple-ios`; device linking and signing remain manual.

The crate allows unsafe only for the explicit C pointer boundary, with local safety explanations. Byte encoding and decoding avoid alignment casts.

## Calendar and sky

`ws_utc_to_sim` is a pure configuration conversion: a 40-byte, 4-aligned `WsUtcLabel` with exact ABI, size, reserved, civil-label and range validation, and an i64 result only on success. It preserves valid sub-grid input; slice creation separately rejects a start off the 100 ms grid. No timezone or epoch arithmetic is duplicated in C#. Errors use thread-local diagnostics.

`WsCalendarSky` is the 160-byte, 8-aligned record `ws_slice_get_calendar_sky` returns: ABI and size, committed tick, time and hash, UTC and New York label fields, offset, scale, solar/NY/leap/site versions, flags, sun direction and site angles. The caller initializes only ABI and size. Bit 0 of `flags` means supported local time and sun; otherwise the local, offset and float fields are zero while UTC and committed metadata stay exact.

## Real-slice transport (VS-008, introduced in ABI7)

[ADR0031](../../../docs/adr/0031-real-slice-render-transport-and-camera.md) adds `WsSliceHandle`. `ws_slice_create` takes the32-byte seed/start config and stores the existing calendar-aligned schedule epoch. The cohort remains the existing10–30 synthetic residents. `ws_slice_apply` validates a64-byte coarse operation before applying typed Steps(≤100), scale, HostElapsed or normalized camera interest. The integer operation/outcome log and committed-prefix stops remain authoritative; no second host step accumulator, toy command reinterpretation or new save format is introduced.

`ws_slice_fill_geometry` copies one cached complete58-building footprint mesh:96-byte header,48-byte BIN/optional-featured-ID building rows,24-byte position/normal vertices and global u32 indices. Context has no EntityId. `ws_slice_fill_snapshot` returns a128-byte header, all residents in48-byte stable-ID rows and four48-byte summaries. It serializes into bounded scratch before one output copy. Header/count/offset/reserved fields, limits and ENU axis conventions are defined in ADR0031. Current geometry is71,532bytes; maximum dynamic output is1,760bytes. These are transport sizes, not device performance results.

`ws_slice_get_calendar_sky` returns the unchanged160-byte shape in the same fixed site ENU basis. Pair dynamic/sky under one caller gate and require matching tick/time/hash/scale/dataset. Queries never advance/flush/materialize, change history or use RNG. Short fill reports the required size without touching data; other slice-fill errors preserve data and `written`. Sky callers initialize only ABI/size. Native validation and caught prepublication panics preserve caller output; poison permits only diagnostic/destruction. Slice creation/diagnostic errors use the existing TLS convention.

Native/C/shared fixture tests cover exact layouts, old1–4 refusal, geometry counts, normalized integer replay, pause/prefix semantics, coherence, source/coordinate tiers, unaligned/null/reserved/error paths and actual query fault injection. [Fixture notes](tests/fixtures/README.md) explain the deliberately zero hash sentinel and Tier3 float comparisons. All56 pinned Editor tests pass, preserving the existing44 while adding real-slice layout/fixture/purity/allocation, play-camera, actual overview/street pixels and real-backend1×/100× GPU-fenced frame/worker evidence. Actual iPhone acceptance remains separate; no device claim is implied here.

## ABI7 inspection (ADR0032)

ABI6 was current at that point. `ws_slice_inspect` returns a bounded8192-byte maximum little-endian schema1 record for a selected resident or featured building. Probe/fill has96-byte header, kind-specific identity/state,48-byte timetable rows and80-byte factual event rows. Short capacity reports exact required size without payload writes; other failures zero a valid written control and leave payload untouched. Invalid targets are INVALID_ARGUMENT; resource/domain/encoding failures are STEP_FAILED; caught panic poisons ownership. Native queries remain pure. See accepted ADR0032 for exact offsets and caller lifetime/alias rules.


ADR0033 advances to ABI7/inspection schema2. Resident identity adds sex u32 at absolute136, moving its first string length to140. Resident state remains80bytes: health u32 at56 and life u32 at60 replace the former reserved word. Identity schema is2 for residents/buildings; building sizes stay unchanged. Sex0/1 is Female/Male, life0/1 Alive/Dead, health0..100 with living>=1/dead=0. Stored values come from the retained core vitals column, not presentation defaults. Old v1–6 fixtures and C consumer sources remain; clients explicitly refuse old global versions.

## ABI8 illness transport (ADR0034)

ABI8 was the illness ABI; callers initialize all ABI-bearing requests with8 and retain the existing exact structure sizes. Real-slice operation tag5 has resident/severity(u32)/duration_us then three zero words; framing errors precede domain execution, while semantic rejection is retained in the operation journal.

Inspection schema3 retains header96 and identity schema2, grows resident state80→112 and appends override kind, active flag, override root, last own root and active end at relative80/84/88/96/104. Health/life stay56/60; building state stays48. Recent rows remain80 bytes with closed new payload projections. `ws_slice_consequences` returns header96 plus up to16 rows of128 bytes (event80, at most two explicit parent/relation/confidence slots), capped8192. It never steps or materializes. Short/error/unwind/poison rules preserve data exactly like inspection. Exact offsets and vocabulary are authoritative in [ADR0034](../../../docs/adr/0034-slice-illness-and-consequences.md).

## ABI10 faith and goals (ADR0039), built on ABI9 god powers (ADR0038)

ABI10 adds the read-only export `ws_slice_god(handle, WsSliceGod*)` (560 bytes, caller sets `struct_size`; wrong size is `VERSION_MISMATCH`): day, block faith, up to three goal rows and up to thirty resident faith rows in ascending ID, unused rows zero (ADR0039 §5). No existing signature or format changes. `ws_slice_last_root` is an additive read-only export, so no existing signature changes. Real-slice operation tags6 `Bless {resident}`, 7 `Storm {duration_us}` and 8 `Gather {building, duration_us}` keep the 64-byte operation; every unused word must be zero (`INVALID_COMMAND`), a zero identity is `INVALID_ARGUMENT`, and semantic rejection (wrong target, bounds, a storm or gathering already active) is journalled with no state or event effect (`STEP_FAILED`). Recent-event rows gain the tag14–22 projections of ADR0038 §5. The consequence header is per root: `subject` (48) is the resident for illness and bless, the building for a gathering and 0 only for a storm; `severity` (76) is0 for every non-illness root; the end (88) is the applied time for bless and the episode end for storm and gathering. `ws_slice_last_root(handle, &root)` is a pure read of the newest accepted intervention root (tags5–8; 0 before any), taken in O(1) from the derived consequence index, so a rejected operation leaves it unchanged and it is identical after save/load; it is how a caller learns a storm root, which has no actor to inspect. Inspection keeps schema3 and its 96-byte header; before publication the native encoder checks override coherence (kind0–4, a root exactly when overridden, an overridden resident commuting or at home), and only kinds3 and4 may stand away from home: kind3 at the active gathering's building with the Home activity and Socialize, kind4 (a storm hold, root the storm root) exactly where its hold places the resident (ADR0038 §5, D1 as amended).
