# TP-008 ABI-v2 bridge verification (2026-10-17)

Implements accepted ADR 0019 §8 on #46 after #42/#44 landed. Main `b5f9006db281a3d330a0f68ae2bf21319782b1cd` was merged in `8887ec9`; the command-hashing and optional-Git fixes are retained. ABI/config/snapshot/managed versions are all 2. The 24-byte header and 32-byte debug-marker record geometry are unchanged.

## Native acceptance evidence

- A borrowed canonical v2 NoOp is copied into owned pending input. Before flush, journal/history are unchanged; after `ws_advance(h,0)` it appears once in `World::journal()` and emits its applied event without changing tick or simulation hash. Mutating the original input after return cannot change the queued command.
- A v1 config is refused, v1 batches return VERSION_MISMATCH, and snapshots carry ABI 2. The old fixture remains a negative managed-decoder test.
- Zero-step pause/resume flushes at the same tick; control reads expose only committed scale. Explicit steps work while paused and do not multiply scale again. Oversized advances fail before flushing. Tick overflow retains prior committed commands/ticks.
- Null/version/size/poison control errors leave output untouched. Native layout checks cover every offset and 16-byte size/alignment 4; only the two input fields must be initialized before a successful write.
- 1,024-command and exact 65,536-byte batches exercise native ingress; malformed/over-capacity/schema failures do not enqueue. Semantic LOD rejections receive outcomes and do not stop later commands.
- The generated header is verified by pinned cbindgen 0.29.2. A real C consumer compiles/links/runs against the native library, checking layout, versions, pending/flush semantics and explicit paused stepping. The same consumer runs in Linux/macOS CI.

## Unity verification

Unity 6000.3.26f1, Apple Silicon Mac, actual rebuilt ABI-v2 dylib: **29 passed, 0 failed**. The [redacted NUnit report](unity-abi-v2-results.xml) includes layout/legacy refusal, pending pause/resume/control-state reads, synthetic snapshot fixture/invalid frames, native lifecycle, rendering and catch-up/scale checks. Both warmed fill/decode and the new per-frame flush/control read allocate zero managed bytes on the calling thread over 100 measured calls. This is not zero total engine allocation or device profiling.

The host always flushes (including paused/no-tick frames), reads native scale and applies it once to future elapsed time. The old catch-up cap/drop/remainder policy is preserved. No gameplay UI, real-entity rendering or history-query export is added.

## Reproduce on Noah's Mac

1. Close Unity, switch to `codex/9-command-events`, and pull its latest commit in the intended checkout.
2. At the repository root run `scripts/build-native-macos.sh`; reopen `apps/unity/` with Unity 6000.3.26f1. Restarting unloads any old ABI-v1 plugin.
3. Open **Window → General → Test Runner → EditMode → Run All** with graphics enabled. Check `CommandsStayPendingUntilFlushAndResumeAtTheSameTick`, control layout/old-client refusal, both allocation tests and the Bootstrap Play Mode test.
4. Open Bootstrap and press Play; confirm moving cyan geometry and no Console errors. Record the exact head SHA and results on #46. Noah's independent rerun/acceptance is not inferred from the implementer's local run.

From `core/`, normal checks remain `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo build --workspace --all-targets --locked`, and `cargo test --workspace --locked`. From the root run documentation, Unity static, Python tests, dependency direction and header freshness checks listed in ci.md. Existing ignored long LOD tests remain nightly; no goldens/tolerances/assertions were weakened.

## Limits and preserved fixes

Historical command-overhead and history-memory/query measurements remain in ws-sim README. No per-command world hash was restored. Optional Git identity still falls back to nogit and keeps the source digest. No new dependency, service, credential, architecture decision or persistence format was introduced. Actor-index scaling remains #49, outside this adaptation.

The local iOS Rust target build completed but emitted SDK-link warnings because this Mac has Command Line Tools rather than full Xcode/iPhone SDKs. That is not device acceptance evidence. The hosted full-Xcode native/header job supplies clean target-build verification. No signing, device install, iPhone performance or thermal result is claimed.
