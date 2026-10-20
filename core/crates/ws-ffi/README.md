# ws-ffi

The only Rust library Unity links. Implements ABI4 under accepted ADR0030, extending ADR0018/0019/0022. Owns opaque world handles and bounded diagnostics; the toy world's size is set by `residents_per_building` (1 to 2,000, five buildings). Owns no independent history: typed commands enter ws-sim, which publishes journal outcomes/events at flush boundaries.

## Contract

See `include/ws_ffi.h` for the generated C interface and [ADR 0018](../../../docs/adr/0018-rust-unity-bridge.md) for all lifetimes and byte offsets. Callers must serialize calls/disposal for each handle. Caller buffers and out-parameters must be valid and nonoverlapping; arbitrary/dangling pointers are not recoverable errors. Creation errors use thread-local diagnostics (`ws_last_error(NULL,...)`) on the same thread. Operational panics poison the handle; only diagnostics/destruction are then valid. Process abort and operating-system out-of-memory kills are not recoverable. Fallible command/history reservations return explicit errors before their transaction publishes.

All snapshot bytes are initialized and little-endian: 24-byte header (version/header-size/tick/count/record-size), followed by 32-byte instances (raw EntityId, kind, east, north, up, a, b). Kind 1 is a building box (a = half extent, b = height, metres) and kind 2 a resident capsule (a = heading radians from east towards north, b = 0). Buildings come first, then residents, each in ascending ID order. Positions are metres east/north/up of the lowest-ID building, the snapshot origin. Shared fixture `tests/fixtures/snapshot-v4.hex` is decoded in Rust and Unity tests. Render data is derived from canonical state and is never hashed.

Config version/size/reserved must be4/32/0, start must be an aligned 100ms boundary, residents per building is 1..2000. Advance validates its 0..100 step limit before flushing, flushes once even for zero steps/paused scale, then executes explicit physical ticks independently of scale. A failed tick preserves earlier command commits and successful ticks. Snapshot capacity is exactly `24 + 32 * (5 + 5 * residents_per_building)`, i.e. the header count; a short buffer is untouched and returns required size. A valid `written` pointer is mandatory. A null `written` argument returns INVALID_ARGUMENT and records the diagnostic on the live handle; with no usable handle, diagnostics use the thread-local slot. A non-null `written` is reset to zero even when the handle is null. Normal frames need one fill into preallocated storage. No per-entity native calls or callbacks.

Command batches use ADR 0019 §8: little-endian version 2/count, then opcode/payload length plus schema 1 and issued_by. NoOp, SetTimeScale and SetNodeLod are supported with the same exact bounds as the core (64 KiB/1,024 commands, also for the pending queue). `ws_submit` atomically copies owned typed commands; OK means pending, not applied. Malformed/over-capacity inputs return INVALID_COMMAND, unsupported versions return VERSION_MISMATCH, null positive-length inputs return INVALID_ARGUMENT, reservation/boundary failures return STEP_FAILED. Semantic LOD failures are journaled rejections and do not fail an otherwise completed flush. No borrowed bytes survive return.

`ws_get_control_state` is read-only and never flushes. Its C struct is exactly 16 bytes/alignment 4: u32 ABI version, struct size, committed scale tag, reserved. Initialize the first two fields to4/16; output-only fields need not be initialized. A usable handle, non-null aligned output, matching version and exact size are required. Errors leave the destination untouched; success returns4/16/(0,1,10,100)/0. Existing per-handle/thread-local diagnostic and poison rules apply. There is no public history-query FFI.

Old v1, v2 and v3 clients fail the ABI preflight/config check; v1 command batches and snapshots are not reinterpreted. The legacy v1 fixture is retained only to test refusal. Geometry/header sizes remain 24/32 bytes; the snapshot ABI field is now4 (config field 24 is `residents_per_building`; snapshot records carry kind-specific `a`/`b`). The command batch format is unchanged at version 2. Unity flushes even while paused, reads committed scale and multiplies elapsed time once; native explicit steps do not multiply it again. Catch-up still drops excess whole steps beyond 100 and retains the fractional simulated remainder.

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

The crate allows unsafe only for the explicit C pointer boundary, with local safety explanations. Byte encoding/decoding avoids alignment casts. Tests cover exported error paths, layout, shared v4 bytes and v1/2/3 refusal, pending ownership, journaled NoOp/semantic rejection, exact maximum batches, pause/resume, unchanged canonical hash on fill/NoOp, UTF-8 diagnostic truncation, step failure and poison behavior. Destroy must be exactly once; double destruction is not a supported test.

Desktop benchmark and Editor verification results are recorded on the implementation PR. They do not establish the iPhone ≤2ms fill target or thermal budget.

## Calendar and sky (ABI4)

`ws_utc_to_sim` is a pure configuration conversion:40-byte/alignment4 `WsUtcLabel`, exact ABI/size/reserved/civil/range validation, checked wide fields, and an i64 result only on success. It preserves valid sub-grid input; creation independently rejects an unaligned100-ms start. No timezone/epoch arithmetic is duplicated in C#. Errors use thread-local diagnostics.

`ws_get_calendar_sky` returns160-byte/alignment8 `WsCalendarSky`: ABI/size, committed tick/time/hash, UTC and New York label fields, offset, scale, solar/NY/leap/site versions, flags, direction and site angles. Caller initializes only ABI/size. Bit0 means supported local/solar; otherwise local/offset/float fields are zero while UTC and committed metadata remain exact. Null/misaligned pointer, incompatible version/size, poison or caught panic leaves output untouched. Raw callers must serialize fill/query/mutation; Unity holds its existing gate across both calls and verifies equal ticks before publishing either output. All floats are finite; toward-sun direction uses the paired snapshot ENU basis and Unity only swaps axes.

The generated header is checked with cbindgen0.29.2. New `abi-v4-smoke.c` preserves the prior C consumer properties and adds UTC/sky/version/error coverage; v1/2/3 fixtures and v3 consumer source remain compatibility references. Version-only success expectations/CI entrypoint advance under ADR0030’s explicit migration acceptance; command envelope2, record shapes, old behavioral checks and core goldens stay unchanged.
