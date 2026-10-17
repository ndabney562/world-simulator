# ws-ffi

The only Rust library Unity links. Implements ABI 2 under accepted ADR 0019 §8, extending ADR 0018. Owns opaque world handles, bounded diagnostics and prototype debug marker count. Owns no independent history: typed commands enter ws-sim, which publishes journal outcomes/events at flush boundaries.

## Contract

See `include/ws_ffi.h` for the generated C interface and [ADR 0018](../../../docs/adr/0018-rust-unity-bridge.md) for all lifetimes and byte offsets. Callers must serialize calls/disposal for each handle. Caller buffers and out-parameters must be valid and nonoverlapping; arbitrary/dangling pointers are not recoverable errors. Creation errors use thread-local diagnostics (`ws_last_error(NULL,...)`) on the same thread. Operational panics poison the handle; only diagnostics/destruction are then valid. Process abort and operating-system out-of-memory kills are not recoverable. Fallible command/history reservations return explicit errors before their transaction publishes.

All snapshot bytes are initialized and little-endian: 24-byte header (version/header-size/tick/count/record-size), followed by 32-byte instances (ID/kind/east/north/up/heading/reserved). IDs are debug indices, not EntityIds. Heading starts at east and turns towards north. Shared fixture `tests/fixtures/snapshot-v2.hex` is decoded in Rust and Unity tests. Only kind 0 exists. The debug ENU origin is synthetic; real-world snapshots need a future version decision.

Config version/size/reserved must be 2/32/0, start must be an aligned 100ms boundary, count is 0..100000. Advance validates its 0..100 step limit before flushing, flushes once even for zero steps/paused scale, then executes explicit physical ticks independently of scale. A failed tick preserves earlier command commits and successful ticks. Snapshot capacity is exactly `24 + 32 * count`; a short buffer is untouched and returns required size. A valid `written` pointer is mandatory. A null `written` argument returns INVALID_ARGUMENT and records the diagnostic on the live handle; with no usable handle, diagnostics use the thread-local slot. A non-null `written` is reset to zero even when the handle is null. Normal frames need one fill into preallocated storage. No per-marker native calls or callbacks.

Command batches use ADR 0019 §8: little-endian version 2/count, then opcode/payload length plus schema 1 and issued_by. NoOp, SetTimeScale and SetNodeLod are supported with the same exact bounds as the core (64 KiB/1,024 commands, also for the pending queue). `ws_submit` atomically copies owned typed commands; OK means pending, not applied. Malformed/over-capacity inputs return INVALID_COMMAND, unsupported versions return VERSION_MISMATCH, null positive-length inputs return INVALID_ARGUMENT, reservation/boundary failures return STEP_FAILED. Semantic LOD failures are journaled rejections and do not fail an otherwise completed flush. No borrowed bytes survive return.

`ws_get_control_state` is read-only and never flushes. Its C struct is exactly 16 bytes/alignment 4: u32 ABI version, struct size, committed scale tag, reserved. Initialize the first two fields to 2/16; output-only fields need not be initialized. A usable handle, non-null aligned output, matching version and exact size are required. Errors leave the destination untouched; success returns 2/16/(0,1,10,100)/0. Existing per-handle/thread-local diagnostic and poison rules apply. There is no public history-query FFI.

Old v1 clients fail the ABI preflight/config check; v1 command batches and snapshots are not reinterpreted. The legacy v1 fixture is retained only to test refusal. Geometry/header sizes remain 24/32 bytes; the snapshot ABI field is now 2. Unity flushes even while paused, reads committed scale and multiplies elapsed time once; native explicit steps do not multiply it again. Catch-up still drops excess whole steps beyond 100 and retains the fractional simulated remainder.

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

The crate allows unsafe only for the explicit C pointer boundary, with local safety explanations. Byte encoding/decoding avoids alignment casts. Tests cover exported error paths, layout, shared v2 bytes and v1 refusal, pending ownership, journaled NoOp/semantic rejection, exact maximum batches, pause/resume, unchanged canonical hash on fill/NoOp, UTF-8 diagnostic truncation, step failure and poison behavior. Destroy must be exactly once; double destruction is not a supported test.

Desktop benchmark and Editor verification results are recorded on the implementation PR. They do not establish the iPhone ≤2ms fill target or thermal budget.
