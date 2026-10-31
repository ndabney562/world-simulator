# ws-ffi

The only Rust library Unity links. Implements ABI8 under accepted ADR0034, extending ADR0018/0019/0022/0030/0031. Owns opaque world handles and bounded diagnostics; the toy world's size is set by `residents_per_building` (1 to 2,000, five buildings). Owns no independent history: typed commands enter ws-sim, which publishes journal outcomes/events at flush boundaries.

## Contract

See `include/ws_ffi.h` for the generated C interface and [ADR 0018](../../../docs/adr/0018-rust-unity-bridge.md) for all lifetimes and byte offsets. Callers must serialize calls/disposal for each handle. Caller buffers and out-parameters must be valid and nonoverlapping; arbitrary/dangling pointers are not recoverable errors. Creation errors use thread-local diagnostics (`ws_last_error(NULL,...)`) on the same thread. Operational panics poison the handle; only diagnostics/destruction are then valid. Process abort and operating-system out-of-memory kills are not recoverable. Fallible command/history reservations return explicit errors before their transaction publishes.

All snapshot bytes are initialized and little-endian: 24-byte header (version/header-size/tick/count/record-size), followed by 32-byte instances (raw EntityId, kind, east, north, up, a, b). Kind 1 is a building box (a = half extent, b = height, metres) and kind 2 a resident capsule (a = heading radians from east towards north, b = 0). Buildings come first, then residents, each in ascending ID order. Positions are metres east/north/up of the lowest-ID building, the snapshot origin. Shared fixture `tests/fixtures/snapshot-v6.hex` is decoded in Rust and Unity tests. Render data is derived from canonical state and is never hashed.

Config version/size/reserved must be8/32/0, start must be an aligned 100ms boundary, residents per building is 1..2000. Advance validates its 0..100 step limit before flushing, flushes once even for zero steps/paused scale, then executes explicit physical ticks independently of scale. A failed tick preserves earlier command commits and successful ticks. Snapshot capacity is exactly `24 + 32 * (5 + 5 * residents_per_building)`, i.e. the header count; a short buffer is untouched and returns required size. A valid `written` pointer is mandatory. A null `written` argument returns INVALID_ARGUMENT and records the diagnostic on the live handle; with no usable handle, diagnostics use the thread-local slot. A non-null `written` is reset to zero even when the handle is null. Normal frames need one fill into preallocated storage. No per-entity native calls or callbacks.

Command batches use ADR 0019 §8: little-endian version 2/count, then opcode/payload length plus schema 1 and issued_by. NoOp, SetTimeScale and SetNodeLod are supported with the same exact bounds as the core (64 KiB/1,024 commands, also for the pending queue). `ws_submit` atomically copies owned typed commands; OK means pending, not applied. Malformed/over-capacity inputs return INVALID_COMMAND, unsupported versions return VERSION_MISMATCH, null positive-length inputs return INVALID_ARGUMENT, reservation/boundary failures return STEP_FAILED. Semantic LOD failures are journaled rejections and do not fail an otherwise completed flush. No borrowed bytes survive return.

`ws_get_control_state` is read-only and never flushes. Its C struct is exactly 16 bytes/alignment 4: u32 ABI version, struct size, committed scale tag, reserved. Initialize the first two fields to8/16; output-only fields need not be initialized. A usable handle, non-null aligned output, matching version and exact size are required. Errors leave the destination untouched; success returns8/16/(0,1,10,100)/0. Existing per-handle/thread-local diagnostic and poison rules apply. The toy handle has no public history-query FFI; the real-slice bounded inspection/consequence APIs are described below.

Old v1, v2, v3 and v4 clients fail the ABI preflight/config check; v1 command batches and snapshots are not reinterpreted. The legacy v1 fixture is retained only to test refusal. Geometry/header sizes remain 24/32 bytes; the snapshot ABI field is now5 (config field 24 is `residents_per_building`; snapshot records carry kind-specific `a`/`b`). The command batch format is unchanged at version 2. Unity flushes even while paused, reads committed scale and multiplies elapsed time once; native explicit steps do not multiply it again. Catch-up still drops excess whole steps beyond 100 and retains the fractional simulated remainder.

## Build and verify

From repository root with the pinned Rust toolchain installed:

```sh
scripts/build-native-macos.sh
cargo install cbindgen --version 0.29.2 --locked
scripts/check-ffi-header.sh
cd core
cargo test --locked -p ws-ffi
cargo bench -p ws-ffi --bench snapshot
```

Header generation (run in `core/`): `cbindgen --config crates/ws-ffi/cbindgen.toml --crate ws-ffi --output crates/ws-ffi/include/ws_ffi.h`. `cbindgen` 0.29.2 (MPL-2.0) is a build tool only, not shipped or linked into the app. Criterion is dev-only (MIT/Apache-2.0). Runtime dependency is ws-sim only. CI verifies generated-header freshness and builds `aarch64-apple-ios`; device linking/signing remains TP-011.

The crate allows unsafe only for the explicit C pointer boundary, with local safety explanations. Byte encoding/decoding avoids alignment casts. Tests cover exported error paths, layout, shared v5 bytes and v1/2/3/4 refusal, pending ownership, journaled NoOp/semantic rejection, exact maximum batches, pause/resume, unchanged canonical hash on fill/NoOp, UTF-8 diagnostic truncation, step failure and poison behavior. Destroy must be exactly once; double destruction is not a supported test.

Desktop benchmark and Editor verification results are recorded on the implementation PR. They do not establish the iPhone ≤2ms fill target or thermal budget.

## Calendar and sky (ABI7)

`ws_utc_to_sim` is a pure configuration conversion:40-byte/alignment4 `WsUtcLabel`, exact ABI/size/reserved/civil/range validation, checked wide fields, and an i64 result only on success. It preserves valid sub-grid input; creation independently rejects an unaligned100-ms start. No timezone/epoch arithmetic is duplicated in C#. Errors use thread-local diagnostics.

`ws_get_calendar_sky` returns160-byte/alignment8 `WsCalendarSky`: ABI/size, committed tick/time/hash, UTC and New York label fields, offset, scale, solar/NY/leap/site versions, flags, direction and site angles. Caller initializes only ABI/size. Bit0 means supported local/solar; otherwise local/offset/float fields are zero while UTC and committed metadata remain exact. Null/misaligned pointer, incompatible version/size, poison or caught panic leaves output untouched. Raw callers must serialize fill/query/mutation; Unity holds its existing gate across both calls and verifies equal ticks before publishing either output. All floats are finite; toward-sun direction uses the paired snapshot ENU basis and Unity only swaps axes.

The generated header is checked with cbindgen0.29.2. Current `abi-v7-smoke.c` preserves the prior C consumer properties and adds UTC/sky/version/error coverage; v1/2/3/4/5 fixtures and v3/v4/v5 consumer sources remain compatibility references. Version-only success expectations/CI entrypoint advance under ADR0031’s explicit migration acceptance; command envelope2, record shapes, old behavioral checks and core goldens stay unchanged.


## Real-slice transport (VS-008, current ABI7)

[ADR0031](../../../docs/adr/0031-real-slice-render-transport-and-camera.md) adds a separate `WsSliceHandle`; it is never interchangeable with `WsHandle`. `ws_slice_create` takes the32-byte seed/start config and stores the existing calendar-aligned schedule epoch. The cohort remains the existing10–30 synthetic residents. `ws_slice_apply` validates a64-byte coarse operation before applying typed Steps(≤100), scale, HostElapsed or normalized camera interest. The integer operation/outcome log and committed-prefix stops remain authoritative; no second host step accumulator, toy command reinterpretation or new save format is introduced.

`ws_slice_fill_geometry` copies one cached complete58-building footprint mesh:96-byte header,48-byte BIN/optional-featured-ID building rows,24-byte position/normal vertices and global u32 indices. Context has no EntityId. `ws_slice_fill_snapshot` returns a128-byte header, all residents in48-byte stable-ID rows and four48-byte summaries. It serializes into bounded scratch before one output copy. Header/count/offset/reserved fields, limits and ENU axis conventions are defined in ADR0031. Current geometry is71,532bytes; maximum dynamic output is1,760bytes. These are transport sizes, not device performance results.

`ws_slice_get_calendar_sky` returns the unchanged160-byte shape in the same fixed site ENU basis. Pair dynamic/sky under one caller gate and require matching tick/time/hash/scale/dataset. Queries never advance/flush/materialize, change history or use RNG. Short fill reports the required size without touching data; other slice-fill errors preserve data and `written`. Sky callers initialize only ABI/size. Native validation and caught prepublication panics preserve caller output; poison permits only diagnostic/destruction. Slice creation/diagnostic errors use the existing TLS convention.

Native/C/shared fixture tests cover exact layouts, old1–4 refusal, geometry counts, normalized integer replay, pause/prefix semantics, coherence, source/coordinate tiers, unaligned/null/reserved/error paths and actual query fault injection. [Fixture notes](tests/fixtures/README.md) explain the deliberately zero hash sentinel and Tier3 float comparisons. All56 pinned Editor tests pass, preserving the existing44 while adding real-slice layout/fixture/purity/allocation, play-camera, actual overview/street pixels and real-backend1×/100× GPU-fenced frame/worker evidence. Actual iPhone acceptance remains separate; no device claim is implied here.

## ABI7 inspection (ADR0032)

Current ABI is6. Existing toy/config/control/calendar/slice layouts and semantics remain; all v1–5 fixtures and C consumer sources are retained. Current consumers select v6 fixtures and refuse versions1–5. `ws_slice_inspect` returns a bounded8192-byte maximum little-endian schema1 record for a selected resident or featured building. Probe/fill has96-byte header, kind-specific identity/state,48-byte timetable rows and80-byte factual event rows. Short capacity reports exact required size without payload writes; other failures zero a valid written control and leave payload untouched. Invalid targets are INVALID_ARGUMENT; resource/domain/encoding failures are STEP_FAILED; caught panic poisons ownership. Native queries remain pure. See accepted ADR0032 for exact offsets and caller lifetime/alias rules.


ADR0033 advances to ABI7/inspection schema2. Resident identity adds sex u32 at absolute136, moving its first string length to140. Resident state remains80bytes: health u32 at56 and life u32 at60 replace the former reserved word. Identity schema is2 for residents/buildings; building sizes stay unchanged. Sex0/1 is Female/Male, life0/1 Alive/Dead, health0..100 with living>=1/dead=0. Stored values come from the retained core vitals column, not presentation defaults. Old v1–6 fixtures and C consumer sources remain; clients explicitly refuse old global versions.

## ABI8 illness transport (ADR0034)

ABI8 was the illness ABI; callers initialize all ABI-bearing requests with8 and retain the existing exact structure sizes. Older ABI1–7 consumers/fixtures remain as compatibility/refusal references. Its smoke consumer `tests/abi-v8-smoke.c` is retained. Real-slice operation tag5 has resident/severity(u32)/duration_us then three zero words; framing errors precede domain execution, while semantic rejection is retained in the operation journal.

Inspection schema3 retains header96 and identity schema2, grows resident state80→112 and appends override kind, active flag, override root, last own root and active end at relative80/84/88/96/104. Health/life stay56/60; building state stays48. Recent rows remain80 bytes with closed new payload projections. `ws_slice_consequences` returns header96 plus up to16 rows of128 bytes (event80, at most two explicit parent/relation/confidence slots), capped8192. It never steps or materializes. Short/error/unwind/poison rules preserve data exactly like inspection. Exact offsets and vocabulary are authoritative in [ADR0034](../../../docs/adr/0034-slice-illness-and-consequences.md).

## Current ABI9 god powers (ADR0038)

The current ABI is9; ABI1–8 consumers/fixtures remain as compatibility/refusal references, and `tests/abi-v9-smoke.c` is the current smoke consumer, with v9 fixtures added beside the retained ABI8 fixtures and `abi-v8-smoke.c`. `ws_slice_last_root` is an additive read-only export, so no existing signature changes. Real-slice operation tags6 `Bless {resident}`, 7 `Storm {duration_us}` and 8 `Gather {building, duration_us}` keep the 64-byte operation; every unused word must be zero (`INVALID_COMMAND`), a zero identity is `INVALID_ARGUMENT`, and semantic rejection (wrong target, bounds, a storm or gathering already active) is journalled with no state or event effect (`STEP_FAILED`). Recent-event rows gain the tag14–22 projections of ADR0038 §5. The consequence header is per root: `subject` (48) is the resident for illness and bless, the building for a gathering and 0 only for a storm; `severity` (76) is0 for every non-illness root; the end (88) is the applied time for bless and the episode end for storm and gathering. `ws_slice_last_root(handle, &root)` is a pure read of the newest accepted intervention root (tags5–8; 0 before any), taken in O(1) from the derived consequence index, so a rejected operation leaves it unchanged and it is identical after save/load; it is how a caller learns a storm root, which has no actor to inspect. Inspection keeps schema3 and its 96-byte header; before publication the native encoder checks override coherence (kind0–4, a root exactly when overridden, an overridden resident commuting or at home), and only kinds3 and4 may stand away from home: kind3 at the active gathering's building with the Home activity and Socialize, kind4 (a storm hold, root the storm root) exactly where its hold places the resident (ADR0038 §5, D1 as amended).
