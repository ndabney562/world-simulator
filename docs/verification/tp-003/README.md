# TP-003 bridge verification (2026-10-16)

This records local implementation evidence, not iPhone performance or a stage-exit declaration.

## Unity 6000.3.26f1 on Apple Silicon macOS

- Built the actual arm64 dylib with `scripts/build-native-macos.sh`.
- All **23** Editor/integration tests passed, zero failed/skipped. [NUnit report](unity-results.xml).
- `BootstrapRendersAndAdvancesFromNativeSnapshots` enters Play Mode in Bootstrap, checks native tick progress, renders to a 960×540 texture and verifies visible cyan geometry. [Rendered frame](bootstrap.png).
- The warmed fill/decode allocation test performs 100 fills of 1,000 records after warm-up, with **0 managed bytes** measured by `GC.GetAllocatedBytesForCurrentThread` on the calling thread. This covers decoding/native fill, not the entire Unity engine or an on-device Profiler session.
- Shared Rust/C# fixture, config layout, ABI rejection, malformed/truncated frames, nonfinite values, native create/advance/fill/dispose and idempotent managed disposal are exercised.

![Bootstrap debug markers](bootstrap.png)

Run from repository root (close the Editor using that checkout first):

```sh
scripts/build-native-macos.sh
/Applications/Unity/Hub/Editor/6000.3.26f1/Unity.app/Contents/MacOS/Unity \
  -batchmode -projectPath "$PWD/apps/unity" \
  -runTests -testPlatform EditMode \
  -testResults /tmp/worldsim-bridge-results.xml -logFile /tmp/worldsim-bridge-editor.log
```

Graphics must remain enabled for the rendering test. Restart after rebuilding a loaded dylib. Temporary import-generated settings/probe scripts are not included in the change.

## Rust and ABI checks

- Normal workspace suite passed (221 passed, zero failed, two existing ignored long LOD tests). Additional snapshot layout/fixture tests added afterward also pass; CI reruns the final workspace.
- All 7 FFI tests pass; `cargo clippy --workspace --all-targets --locked -- -D warnings` passes.
- Pinned cbindgen 0.29.2 produces the committed header; `scripts/check-ffi-header.sh` passes. A deliberate extra line in the header causes exit 1; restoring original bytes passes again.
- Dependency direction, documentation consistency, Unity static checks and all 36 existing Python tests pass.
- Local iOS Rust target compilation produced an archive, but this Mac has Command Line Tools without the iPhone SDK; the accompanying dylib link reports SDK warnings. The dedicated macOS CI job with full Xcode must validate the complete iOS-target build. No signing, Unity iOS build or device install was attempted.

## Desktop snapshot timing

Criterion, release build, Apple Silicon Mac, 20 samples, 1-second warm-up and 2-second measurement, preallocated output, tick zero. Only the snapshot fill is measured (no tick stepping or C# rendering):

| Instances | Central estimate | Reported interval |
|---|---|---|
| 1,000 | 1.0883 µs | 1.0872–1.0892 µs |
| 10,000 | 10.690 µs | 10.638–10.756 µs |

```sh
cd core
cargo bench -p ws-ffi --bench snapshot -- --sample-size 20 --warm-up-time 1 --measurement-time 2
```

These results do not establish the ≤2ms iPhone target. TP-011 must measure release device behavior, managed allocations with Profiler, memory and thermals, and replace debug marker IDs/coordinates with a reviewed real-entity snapshot contract.

## Review follow-up: diagnostic routing and approved tool exception

The null `written` error from `ws_fill_render_snapshot` now follows the same ownership rule as other operational failures: a usable live handle receives its own diagnostic; an unusable/null handle uses the thread-local slot. Non-null output storage is still reset to zero on a null-handle error. Regression `snapshot_output_errors_use_the_live_handle_diagnostic` first provokes an older error, verifies the new handle message replaces it, checks the thread-local sentinel stays untouched, then checks null-handle routing. It fails on the prior head and passes with the correction; all eight native boundary tests pass. The full normal workspace suite passes with 223 tests, zero failures and the two existing nightly-only LOD tests ignored; formatting, Clippy, all-target builds, header freshness, dependency direction, documentation/static checks and all 36 Python tests also pass. This changes no ABI, snapshot layout or successful-call behavior. Unity Editor tests were not rerun for this error-path-only correction.

Pinned cbindgen 0.29.2 declares [MPL-2.0 in its upstream package metadata](https://raw.githubusercontent.com/mozilla/cbindgen/v0.29.2/Cargo.toml). Earlier text treating the initial design approval as settling this licence condition was inaccurate: ADR 0018 had described a permissively licensed generator.

Noah subsequently approved the narrowly scoped **build-tool-only exception for cbindgen 0.29.2** and correction, recorded on [PR #36](https://github.com/ndabney562/world-simulator/pull/36#issuecomment-6225389721) and [PR #42](https://github.com/ndabney562/world-simulator/pull/42#issuecomment-6225389994). The corrected ADR specifies local/CI header generation/freshness only, with no cbindgen runtime dependency/linkage or app distribution of the tool. It does not authorize other MPL tools, runtime dependency changes or automatic future upgrades. The general project licence policy and all header/layout/fixture checks remain intact. This records a project-policy decision, not a legal conclusion.

## Independent-review corrections (2026-10-17)

ABI remains v1. The demo accumulator now takes at most 100 ticks per host call, discards excess whole steps, and retains only sub-step time, matching Rust's host clock policy. Two pure EditMode regressions cover a 60-second stall, absence of subsequent debt, and fractional-time preservation. The native STEP_FAILED diagnostic distinguishes time overflow from world-system errors; the completed-prefix regression checks the overflow diagnostic.

Unity 6000.3.26f1 on this Apple Silicon Mac passed **25/25 tests**, including both new accumulator regressions and actual native snapshot/rendering integration. The redacted [review-run report](unity-review-results.xml) records this run; the earlier 23-test report remains historical evidence with its remaining home path redacted. Both XML reports parse and contain no `/Users/` path. Unity-generated settings/import changes from this isolated verification checkout are excluded from this scoped review correction.

The canonical-state proof for native NoOp and snapshot fill is Rust's `snapshot_and_noop_leave_canonical_state_unchanged`, which calls the real ABI exports; ABI v1 intentionally exports no state hash. The planned-checks Markdown table is contiguous again. No ABI, header, status number, command envelope or snapshot layout changed. ABI-v2 adaptation belongs to #46 after #42 lands, per Noah's recorded merge-order decision.
