# VS-008 engineering verification: real block rendering

Issue [#72](https://github.com/ndabney562/world-simulator/issues/72), accepted [ADR0031](../../adr/0031-real-slice-render-transport-and-camera.md). This is verified engineering progress. The issue and its device acceptance remain open: no iPhone screenshot, device60fps/memory result, signing/provisioning or stage-exit claim is made here.

## Source and method

Branch `codex/72-real-slice`, based on main `d922461f9e0d943cb03e05cf131a958b0dd7f62b` after the independently reviewed, green design PR [#94](https://github.com/ndabney562/world-simulator/pull/94). The committed `tested-files.sha256` pins actual code, tests, native header/fixtures, Unity assets/packages/settings, scripts and data inputs used for local verification. Run `shasum -a256 -c docs/verification/issue-72/tested-files.sha256` at repository root. Documentation-only edits do not alter those runtime hashes. The final remote commit/review/CI is recorded on the implementation PR.

The registered source contains58 simple hole-free footprints,449 non-closing vertices:25convex and33concave polygons. Core version1 validates this limitation rather than silently flattening future holes/multiple parts. Exact i128 ear predicates and integer quality selection with source-order ties preserve roof area while avoiding sliver shading; all source wall edges remain. New roof-topology golden `855534150767469492` is version1, with no earlier golden replaced. The resulting cached mesh has2,245vertices and3,693indices. Four actual featured EntityIds remain distinct from54 source-only context BINs.

Vertices use explicit source geodetic→ECEF→the fixed site ENU at ellipsoidal height0 plus above-ground source roof height. NAVD88 ground values are not used as ellipsoidal height. Existing approximate navigation millimetres use checked inverse/forward adapters and round-trip within2mm at block scale. All previous source/ID/navigation/schedule/solar goldens remain enabled. Static transport is71,532bytes; maximum dynamic output for30people/four summaries is1,760bytes. Coordinate/normals fixture comparisons remain Tier3; integer identity/topology/count/time fields and dedicated sun/frame bits retain their stated exactness tiers. [Fixture details](../../../core/crates/ws-ffi/tests/fixtures/README.md).

## Checks

- 427 Rust tests passed,0failed, the same4 pre-existing ignored long tests. Workspace format, all-target clippy with warnings denied, all-target locked build and dependency-direction checks pass.
- 61 Python script tests, docs/ADR links and static Unity checks pass. Generated header matches pinned cbindgen0.29.2; actual C ABI5 consumer passes while retaining the previous consumer's positive checks and adding the separate slice interface. All old ABI1–4 samples/consumer sources remain; only accepted obsolete success-version expectations/new fixture selections and the C CI entrypoint advance.
- Rust builds the `aarch64-apple-ios` static archive. Existing CLI journal record/replay and save/load smoke checks pass. These are compiler/native checks, not a Unity iOS export or device run.
- 56 actual pinned Unity6000.3.26f1 Editor tests passed,0failed/0skipped, using Metal on Apple M1 Pro arm64/macOS27.0.1. Existing44 tests remain;12 new tests cover slice layouts/fixtures/read-only/coherence/allocation, four camera/control/play cases, actual scene pixels and real-backend performance. Individual test-case durations include actual play-mode work; NUnit's root duration can reset around play-mode domain reloads and is not the total wall duration.

The native tests include actual prepublication unwind injection for all three queries: caller data/control outputs remain unchanged and the handle becomes poisoned. Invalid/null/misaligned/reserved/old-version inputs, short capacities, UTF8 diagnostics, pause/scale/host pacing and integer camera replay are covered. The managed gate checks tick/time/hash/scale/dataset before publishing; rejected decodes leave the previous frame intact. Warmed real-slice fill/decode/sky and render submissions allocate zero managed bytes. Periodic HUD/CSV and changing date text are separately formatted and are not claimed allocation-free.

The existing `BootstrapIsTheOnlyBuildScene` assertion stays intact. RealBlock is selected explicitly by its separate build menu, and its Editor tests use [Unity's asset-path play-mode loader](https://docs.unity3d.com/ScriptReference/SceneManagement.EditorSceneManager.LoadSceneAsyncInPlayMode.html). No scene/static check was relaxed to add it.

## Actual Editor rendering and timing

Configured seed71449 produces30residents, starting at UTC2026-06-22T11:22:30Z (Monday07:22:30 New York). The street view first requests coarse camera interest, then uses a genuinely published nearby perimeter walker; it never invents a fine/off-map position. Unique material colours prove all four featured footprints render in portrait and landscape default fits. Orange-pixel checks prove an actual native capsule is visible at street scale. Core controls/committed sky, pause and100× progression are exercised in the same scene. The HUD has reserved viewport space and cannot obscure the default massing.

`editor-frames.csv.gz` contains every measured frame; `editor-summary.json` contains the directly recomputed case statistics. Each case warms for2seconds, then measures10seconds at1× or100×, on the **real slice** backend with30retained identities/all58source extrusions. The test uncaps the Editor, uses a full960×540 camera viewport, submits an explicit URP SingleCameraRequest every frame and synchronously reads one pixel to fence the GPU. Frame intervals include the Editor loop, fill/decode, explicit render/fence and CSV instrumentation. Fill includes handle-lock wait, native snapshot/sky and managed validation/decode. Native apply cost includes actual host/control operations; per-tick cost divides their captured total by committed ticks. The iOS hardware budget is not inferred from these desktop numbers.

The test starts at street view for1×. At100× the accepted core cap coarsens residents toL2; static real massing and exact aggregate counts remain. The raw CSV records that changing L1/L2 workload, plus scale/tick/hash/mesh counts, rather than implying fine agents stay rendered at100×. The throughput check permits2% and includes ordinary physical-tick boundary quantization at1×. No frames are removed from the statistics.

| Scale | Frames | FPS | Frame mean/p95/max ms | Fill mean ms | GPU-fenced render mean ms | Ticks/second |
|---|---:|---:|---|---:|---:|---:|
| 1× | 7904 | 790.38 | 1.265 / 1.531 / 3.486 | 0.0756 | 0.7370 | 10.000 |
| 100× | 7949 | 794.90 | 1.258 / 1.543 / 4.386 | 0.0730 | 0.7307 | 999.899 |

The PNGs are actual camera render requests, not device screenshots. Overview captures reserve the HUD strip (black in these camera-only images); IMGUI text itself is not composited by that capture path. The street image uses the full camera viewport. The normal scene still runs its HUD/controls during verification.

## Device handoff still required

The local pinned Editor installation currently has Mac Standalone Support only. Its iOS Build Support module is absent, so Unity iOS export/linking is not claimed. The Rust iOS archive and compile-checked **WorldSim → Build iOS real block** menu are ready. That entry selects RealBlock explicitly, includes Lit/Unlit and instancing variants, preserves the committed minimum iOS version and writes `Builds/iOS-RealBlock`; the older TP-011 entry keeps its prior behavior.

To complete #72, install iOS Build Support for6000.3.26f1, run `scripts/build-native-ios.sh`, export with the real-block menu, then perform the separately authorized signing/install/run on the iPhone15Pro. Capture the default overview with all four featured buildings and a street view with actual residents; retain actual CSV at slice scale demonstrating60fps/frame≤16.7ms and report process memory against the≈220MB baseline. Desktop process memory is unavailable, not a measured zero. No paid service, account/credential change or repository setting is introduced.

The uninterrupted20-minute device run, temperature/battery/iOS metadata, optional-backdrop criterion and Product Owner stage-complete declaration remain [#76](https://github.com/ndabney562/world-simulator/issues/76). This engineering PR does not waive or close those criteria.
