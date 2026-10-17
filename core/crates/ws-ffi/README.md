# ws-ffi

The only Rust library Unity links. Implements ABI 1 of accepted ADR 0018. Owns opaque world handles, bounded diagnostics and prototype debug marker count. Emits no world events; NoOp transport validates bytes but has no journal yet (TP-008).

## Contract

See `include/ws_ffi.h` for the generated C interface and [ADR 0018](../../../docs/adr/0018-rust-unity-bridge.md) for all lifetimes and byte offsets. Callers must serialize calls/disposal for each handle. Caller buffers and out-parameters must be valid and nonoverlapping; arbitrary/dangling pointers are not recoverable errors. Creation errors use thread-local diagnostics (`ws_last_error(NULL,...)`) on the same thread. Operational panics poison the handle; only diagnostics/destruction are then valid. Abort and allocation failures are not recoverable.

All snapshot bytes are initialized and little-endian: 24-byte header (version/header-size/tick/count/record-size), followed by 32-byte instances (ID/kind/east/north/up/heading/reserved). IDs are debug indices, not EntityIds. Heading starts at east and turns towards north. Shared fixture `tests/fixtures/snapshot-v1.hex` is decoded in Rust and Unity tests. Only kind 0 exists. The debug ENU origin is synthetic; real-world snapshots need a future version decision.

Config version/size/reserved must be 1/32/0, start must be an aligned 100ms boundary, count is 0..100000. Advance accepts 0..100 ticks and preserves successful prior ticks on failure. Snapshot capacity is exactly `24 + 32 * count`; a short buffer is untouched and returns required size. A valid `written` pointer is mandatory. A null `written` argument returns INVALID_ARGUMENT and records the diagnostic on the live handle; with no usable handle, diagnostics use the thread-local slot. A non-null `written` is reset to zero even when the handle is null. Normal frames need one fill into preallocated storage. No per-marker native calls or callbacks.

NoOp batch: little-endian u32 version=1 and count, then u32 opcode=0 / u32 payload-size=0 per command; 64 KiB/1024 command caps. Whole batch validates before success. Unknown version/opcode, trailing data and malformed size fail. New semantic commands/journaling belong to TP-008 and require ABI review.

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

The crate allows unsafe only for the explicit C pointer boundary, with local safety explanations. Byte encoding/decoding avoids alignment casts. Tests cover exported error paths, layout, golden bytes, unchanged canonical hash on fill/NoOp, UTF-8 diagnostic truncation, step failure and poison behavior. Destroy must be exactly once; double destruction is not a supported test.

Desktop benchmark and Editor verification results are recorded on the implementation PR. They do not establish the iPhone ≤2ms fill target or thermal budget.
