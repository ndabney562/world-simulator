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

## Review follow-up: diagnostic routing and pending tool approval

The null `written` error from `ws_fill_render_snapshot` now follows the same ownership rule as other operational failures: a usable live handle receives its own diagnostic; an unusable/null handle uses the thread-local slot. Non-null output storage is still reset to zero on a null-handle error. Regression `snapshot_output_errors_use_the_live_handle_diagnostic` first provokes an older error, verifies the new handle message replaces it, checks the thread-local sentinel stays untouched, then checks null-handle routing. It fails on the prior head and passes with the correction; all eight native boundary tests pass. The full normal workspace suite passes with 223 tests, zero failures and the two existing nightly-only LOD tests ignored; formatting, Clippy, all-target builds, header freshness, dependency direction, documentation/static checks and all 36 Python tests also pass. This changes no ABI, snapshot layout or successful-call behavior. Unity Editor tests were not rerun for this error-path-only correction.

The licence review remains open. Pinned cbindgen 0.29.2 declares [MPL-2.0 in its upstream package metadata](https://raw.githubusercontent.com/mozilla/cbindgen/v0.29.2/Cargo.toml); ADR 0018 currently describes a permissively licensed generator. Earlier text treating the design approval as settling this licence condition was inaccurate.

Recommendation for Noah: approve a narrowly scoped **build-tool-only exception for cbindgen 0.29.2**, used locally/in CI to generate and check the C header, with no cbindgen runtime dependency or linkage into the simulator. Approval should also explicitly authorize correcting ADR 0018's generator-licence statement; record it on #36/#42 before treating the condition as resolved. It would not authorize introducing other MPL tools, shipping cbindgen as part of the app, changing runtime dependencies or future tool upgrades automatically. This is a proposed project-policy exception, not a claim about legal obligations.

If Noah declines, keep the PR blocked and propose a policy-compatible header generation/verification approach through the ADR process, retaining header freshness, layout and cross-language fixture checks. Do not remove a required check just to avoid the dependency. No exception is accepted or policy/accepted ADR changed by this follow-up.
