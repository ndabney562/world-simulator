# TP-003 bridge verification (2026-10-16)

This records local implementation evidence, not iPhone performance or a stage-exit declaration.

## Unity 6000.3.26f1 on Apple Silicon macOS

- Built the actual arm64 dylib with `scripts/build-native-macos.sh`.
- All **22** Editor/integration tests passed, zero failed/skipped. [NUnit report](unity-results.xml).
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

| Instances | Median estimate | Reported interval |
|---|---|---|
| 1,000 | 1.0883 µs | 1.0872–1.0892 µs |
| 10,000 | 10.690 µs | 10.638–10.756 µs |

```sh
cd core
cargo bench -p ws-ffi --bench snapshot -- --sample-size 20 --warm-up-time 1 --measurement-time 2
```

These results do not establish the ≤2ms iPhone target. TP-011 must measure release device behavior, managed allocations with Profiler, memory and thermals, and replace debug marker IDs/coordinates with a reviewed real-entity snapshot contract.
