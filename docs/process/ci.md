# Continuous Integration

CI is built up incrementally. It checks only what exists; a check is added when the code it checks is added, not before. No job pretends to cover something that does not exist yet.

Workflow: [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml). It runs on every pull request, on pushes to `main`, and on manual dispatch.

## Current checks

| Job | Runs on | What it checks | Status |
|---|---|---|---|
| Docs consistency | Linux | `scripts/check_docs.py`: every relative Markdown link resolves; the ADR index lists every ADR exactly once with a matching status. Then `scripts/vs_block_extract.py check` (the [block 6049 extract](../../data/vertical-slice/bay-ridge-block-6049/README.md) regenerates byte for byte from its pinned raw snapshot) and its tests (every footprint closed and with a height, every lot with a footprint, the featured buildings present) | Active |
| Unity project (static) | Linux | `scripts/check_unity_project.py` on `apps/unity/` without the Unity Editor: Unity 6 editor version recorded consistently; only Unity-registry packages; every asset has a `.meta` and GUIDs are unique; scenes and settings are text-serialized; the serialized iOS graphics API list is exactly Metal, independent of the automatic-selection flag; the assembly layout (`WorldSim.Bridge` ← `WorldSim.Rendering`; Editor-only tests); no code in `Providers/`; build scene and render pipeline references resolve; Unity's generated folders are ignored and untracked. Then the script's own tests | Active |
| Rust core | Linux (x86-64) and macOS 26 (arm64) | In `core/`, with the toolchain pinned in `core/rust-toolchain.toml`: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` (includes the no-wall-clock lint from `core/clippy.toml`), `cargo build --workspace --all-targets --locked`, `cargo test --workspace --locked`; then `scripts/check_core_deps.py` (crate dependency direction, below) and its tests (`python3 -m unittest discover -s scripts -p 'test_*.py'`), including exact VS-002 compiled-building regeneration against the VS-001 extract | Active. There is no skip path: a missing `core/` workspace fails the job |
| Tier 1 golden tests | Linux (x86-64) and macOS 26 (arm64), inside the Rust core job's `cargo test` | `ws-math` cell IDs, sector membership, and re-expression against `core/crates/ws-math/tests/golden/cell_ids.txt`; `ws-rng` SipHash-2-4 reference vectors and derivation and draw values; `ws-world` procedural IDs, residents per building, and target choices against `core/crates/ws-world/tests/golden/toy_world.txt`. VS-002 adds `tests/golden/block_6049.txt` for real-building IDs and integer coordinates/heights/use. Any difference on either platform fails the build | Active (TP-005, TP-006, VS-002) |
| Tier 2 replay tests | Linux (x86-64) and macOS 26 (arm64), inside `cargo test` | `ws-sim` and `ws-world`: identical per-tick state hashes for 10,000 ticks with the same seed, with and without scripted LOD transitions, including a run in a separate process; insertion-order independence. Compared within one platform, never across | Active (TP-006, TP-007) |
| Simulation LOD tests | Linux (x86-64) and macOS 26 (arm64), inside `cargo test` | `ws-world` `tests/lod.rs` and `src/lod_tests.rs` (ADR 0017 §14): zero-time round trips, conservation properties, L1/L2 equivalence over one simulated hour, the invariant checker against injected faults, and `lod_soak_short` (1,000 agents, 30 simulated minutes, about 3 minutes in a debug build) | Active (TP-007) |
| Long-running LOD tests | Linux (`ubuntu-latest`), daily at 07:23 UTC and manual dispatch: `cargo test --release --locked -p ws-world --test lod -- --ignored` | `l2_matches_l1_over_eight_hours` and `lod_soak_long` (3,000 agents, 24 simulated hours), marked `#[ignore]` as nightly tests ([testing-policy](testing-policy.md) §4) | Active in [nightly.yml](../../.github/workflows/nightly.yml). Both tests passed in push validation, manual dispatch and the scheduled run; evidence below (#40) |

The TP-003 **Native bridge and header** job runs on macOS 26. It installs the workspace-pinned Rust toolchain and `aarch64-apple-ios`, installs cbindgen 0.29.2 as a build-only tool, verifies `ws_ffi.h` matches regenerated output, and builds the iOS target with `--release --locked`. ABI-v6 ingress/journal/zero-step/control-state/old-client refusal and FFI error/layout tests are included in both Rust matrix jobs. A real C consumer of the generated ABI-v6 header/library also compiles and runs in both Rust jobs (layout, version refusal, UTC/calendar/sky and flush/control semantics). Unity Editor integration/rendering tests remain a local verification step; static checks do not substitute for them.

TP-008 adds `ws-events` fixed-seed ID/reference/query/reconstruction properties, `ws-sim` command/rollback/fault-injection checks, and `ws-cli` separate-process journal replay to the workspace suite. The explicit **CLI Tier 2 replay** CI step exercises a real record/replay pair on both operating systems with the checked-in command script (NoOp, paused/repeated flushes, scale changes and L2/L1 transitions), in addition to integration tests for mixed commands, maximum batches and corrupt/incomplete artifacts. The standard suite still runs every existing test; the two existing ignored long tests stay in nightly. [Tape codec](../technical-prototype/command-event-tape.md) documents strict framing and environmental prefix fences. No thread-count comparison is claimed while scheduling is single-threaded.

The repository is public, so GitHub-hosted Linux and macOS runners cost nothing (ADR 0007).

The macOS jobs now request the explicit standard **`macos-26` arm64** image (#117). GitHub identifies this image as [generally available](https://github.blog/changelog/2026-02-26-macos-26-is-now-generally-available-for-github-hosted-runners/); [standard runners remain free for this public repository](https://docs.github.com/en/actions/reference/runners/github-hosted-runners). The earlier `macos-15` runs repeatedly could not acquire hosted runners and were cancelled with macOS arm64 capacity notices. This tests a different image pool; improved availability is unproven until a hosted run actually starts.

The existing check name **`Rust core (macos-15)` is retained as a historical compatibility identifier**, independently of its runner image. This preserves any required-check configuration without changing repository settings. The matrix `os` value supplies that identifier and temporary artifact names; `matrix.runner` selects the actual image. `Native bridge and header` keeps its existing name too. Each Mac-capable job logs `sw_vers` and explicitly verifies macOS major version 26 and `arm64` before running the unchanged checks.

Every original job/step, pinned Rust/cbindgen version, golden, native C consumer, replay/save-load test and iOS archive check remains. No failure is ignored, check made optional, performance budget changed or signing/account action introduced. This CI-only image change does not change the Unity Editor pin or game platform scope. #117 records hosted results and independent review before merge.

### Additional Intel pool diagnostic (#117)

The first macOS26 run [40734703787](https://github.com/ndabney562/world-simulator/actions/runs/40734703787) also failed to acquire either Mac runner; both jobs were cancelled before execution at 17:35:08 UTC on 2026-10-28. Linux Rust, docs and Unity-static checks passed. The alternative image therefore did not resolve the observed arm64 capacity problem.

Additive `macos-15-intel` probes now run every existing Rust-core and native-header/iOS-archive step on a separate standard Intel pool, with explicit macOS15/x86_64 diagnostics. [GitHub lists this as a free standard runner for public repositories](https://docs.github.com/en/actions/reference/runners/github-hosted-runners). Their check names are `Rust core (macos-15-intel)` and `Native bridge and header (Intel probe)`. The original Apple Silicon jobs, required-check identities and failure conditions remain. Intel success is comparative evidence and cannot replace a missing Apple Silicon result or authorize merge. No signing, credentials, settings, paid runner or game build changes are introduced.

The Intel experiment [40744059494](https://github.com/ndabney562/world-simulator/actions/runs/40744059494) at `db6c0e4c7d4b261e7e0071a313900000ec42517c` completed both Intel checks successfully, including full Rust/C ABI/replay/save-load verification and the native header/iOS archive job. The retained Apple Silicon jobs again could not acquire runners. A strictly additive `ubuntu-24.04-arm` row now tests the identical Rust/C ABI/replay/save-load steps on native ARM Linux, verifying `aarch64` explicitly. It retains all existing rows and checks. These results will inform the [proposed temporary fallback](ci-runner-fallback-proposal.md); that proposal requires Product Owner approval and is not enabled by the diagnostic matrix.

## Nightly LOD workflow

[`.github/workflows/nightly.yml`](../../.github/workflows/nightly.yml) runs the two existing ignored LOD tests in release mode on Linux. It uses the same pinned-toolchain installation and Cargo cache as PR CI, read-only contents permission, and no secrets. Failures fail the job; tests and PR checks are unchanged. The daily schedule is 07:23 UTC (03:23 New York during daylight saving time, 02:23 during standard time); GitHub may delay scheduled runs. Scheduled runs use `main`.

The job has a **120-minute timeout**, based on [validation run 38922438452](https://github.com/ndabney562/world-simulator/actions/runs/38922438452): both `l2_matches_l1_over_eight_hours` and `lod_soak_long` passed; tests took 3,029.04 seconds (50m29s), and the whole job took 50m57s. The limit is over twice the observed job duration, allowing for cold builds and runner variation. This was a push-triggered validation run, not evidence of manual dispatch. No tests were shortened. If runtime approaches the limit, investigate and adjust with evidence rather than weakening tests.

The temporary branch-only push trigger used for measurement was merged in #39; it is removed here. Production triggers are the daily schedule and manual dispatch only.

The workflow now exists on `main`, satisfying GitHub's [manual-dispatch registration requirement](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow). Select a branch in **Actions → Nightly LOD tests → Run workflow**, or run:

```sh
gh workflow run nightly.yml --repo ndabney562/world-simulator --ref main
gh run list --repo ndabney562/world-simulator --workflow nightly.yml --branch main
```

Manual dispatch is confirmed in [run 38930265659](https://github.com/ndabney562/world-simulator/actions/runs/38930265659), on `codex/29-measured-timeout` at `912a3827d8af623810d795c4050139f0da228235`: both `l2_matches_l1_over_eight_hours` and `lod_soak_long` passed (2 passed, 0 failed), with 3,095.55 seconds of tests and a 52m07s job. This is separate from the push validation above. The subsequent [scheduled run 39014956802](https://github.com/ndabney562/world-simulator/actions/runs/39014956802) on `main` at `de1a59eea49f2febfe65eec97213fa29ffab8526` also passed both tests: 2,916.30 seconds of tests, 49m04s for the job. These measurements complete the dispatch verification tracked in #40; retain run URLs, tested SHAs, test outcomes and job durations for future changes. Ordinary PR CI does not run ignored tests. No extra notification service is configured; GitHub's notification preferences apply.

## Running the checks locally

```sh
python3 scripts/check_docs.py
python3 scripts/check_unity_project.py
python3 -m unittest discover -s scripts -p 'test_*.py'
cd core
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --all-targets --locked
cargo test --workspace --locked
python3 ../scripts/check_core_deps.py
```

## Crate dependency direction

`scripts/check_core_deps.py` reads `cargo metadata` for `core/` and enforces [17-repository-structure.md](../architecture/17-repository-structure.md) §18.2. A workspace crate may have normal or build dependencies only on workspace crates in a strictly earlier layer; dev-dependencies are not restricted, except that they are also checked against the forbidden list.

| Layer | Crates |
|---|---|
| 0 Foundation | `ws-math`, `ws-time`, `ws-rng`, `ws-ecs` |
| 1 World model and events | `ws-world`, `ws-events` |
| 2 Domain | `ws-data`, `ws-procgen`, `ws-agents`, `ws-life`, `ws-physics`, `ws-lod`, `ws-interventions`, `ws-snapshot` |
| 3 Orchestration | `ws-sim` |
| 4 Persistence | `ws-persist` (builds and reads a `ws-sim` `World`; ADR 0020) |
| 5 Test support | `ws-testkit` (allowed only as a dev-dependency) |
| 6 Adapters | `ws-ffi`, `ws-cli` |

Domain crates do not depend on each other (same-layer edges fail); they share types through `ws-world` and `ws-events`. A workspace crate missing from the table fails the check, so each new crate gets a deliberate place. The script also fails on any dependency (of any kind) on Unity, rendering, windowing, platform-API, or visual-provider crates (`wgpu`, `bevy*`, `winit`, `objc*`, `cesium*`, `web-sys`, `windows*`, and others listed in the script). The table lives in the script and here; change both together.

VS-005 adds new navigation/behavior-rule-v1 goldens in `ws-life/tests/golden/behavior.txt`, checked by both Rust platforms and the independent Python source/path/need oracle. Workspace tests also cover an actual fine-simulation week, physical-tick needs, mixed-level conservation, domain/history failure rollback and separate-process active-slice replay. Existing goldens, tolerances, nightly paths and checks are unchanged.

VS-006 adds exact camera-threshold and slice-event-tag9 goldens,1000-cycle camera conservation properties (plus the maximum30-resident cohort), an independent occupancy/activity comparison at every physical tick of a week, controller/cache/history fault injection, host-prefix replay and separate-process camera/scale/bulk-view replay. Legacy payload encoding/save writes explicitly reject tag9. All checks and existing fixtures/tolerances/nightly tests remain enabled; the native/Unity contract is unchanged by this core issue.

VS-007 adds new exact solar/query-frame bit goldens (rule1), published-reference comparisons at≤0.1°, integer NY/DST/leap/model-Monday properties, pure/coherent native calendar queries and separate-process speed/calendar boundary replay. ABI4 migrates only obsolete version expectations and the C consumer entrypoint under accepted ADR0030; all v1/2/3 fixtures, old C consumer source and behavioral checks remain. Actual pinned Editor graphics/control/light/100× measurements remain local evidence, distinct from device checks.

VS-008 adds the separate ABI5 real-slice handle, bounded geometry/snapshot transport, layout/error/poison/coherence fixtures and exact topology tests. The C consumer entrypoint advances to v5 while retaining all v1–4 fixtures/consumer sources. Actual pinned Editor tests cover real massing, camera, street residents and GPU-fenced timings locally; these do not establish device acceptance. The Bootstrap-only default build-scene guard remains unchanged, with RealBlock selected by its explicit build entry.

## Planned additions

Each is added by the issue that introduces what it checks.

| Check | Added by |
|---|---|
| Unity edit-mode tests in CI, once the Product Owner adds the licence secrets (S-6, below) | Follow-up to TP-002 |
| Tier 2 replay tests with different thread counts (scheduler remains single-threaded) | When parallel scheduling exists |
| Expanded provenance and intervention invariants | TP-010 |
| iOS Unity build (if feasible on free runners) | TP-011 (see [the spike note](../spikes/tp-011-iphone-performance.md); recommendation pending device data) |
| Nightly Tier 3 drift reports and benchmarks | Follow-up issues when wanted; LOD soak is configured separately above |

## Unity edit-mode tests and S-6

**Question (S-6, [21-decisions-now-vs-later.md](../architecture/21-decisions-now-vs-later.md)):** can the Unity edit-mode tests run in GitHub Actions on the free Personal licence?

**Answer: yes, but only after the Product Owner adds three repository secrets, so until then they run on the Product Owner's Mac.**

- Unity no longer supports manual (offline file) activation of Personal licences. The free route that works is [GameCI](https://game.ci/docs/github/activation) (`game-ci/unity-test-runner`, MIT licence), which runs the editor in a Docker image on a GitHub-hosted Linux runner. That costs nothing on this public repository (ADR 0007).
- GameCI needs three Actions secrets: `UNITY_LICENSE` (the contents of the `Unity_lic.ulf` file that Unity Hub writes on the Mac after activating a Personal licence, at `/Library/Application Support/Unity/Unity_lic.ulf`), `UNITY_EMAIL`, and `UNITY_PASSWORD`. Adding a credential is a Product Owner gate ([human-approval-gates.md](human-approval-gates.md), "External services and credentials"); Claude never handles these values.
- Trade-offs to weigh before approving: the Unity account password is stored as an Actions secret (a Unity account used only for CI limits the exposure); secrets are not passed to workflows from forks, which does not matter while all branches live in this repository; the licence file may need regenerating after it expires or after a major editor upgrade.
- When the secrets exist, a follow-up issue adds a "Unity edit-mode tests" job pinned to the editor version in `apps/unity/ProjectSettings/ProjectVersion.txt`. It is not added before then, because a job that skips when secrets are missing would report green while checking nothing.

**Local procedure until then (Product Owner's Mac, Unity 6000.3.26f1 with iOS Build Support):**

1. Pull the PR branch and open `apps/unity/` in Unity Hub with the recorded editor version.
2. Confirm the Console shows no errors or warnings after the import.
3. Run **Window → General → Test Runner → EditMode → Run All** (or the batch-mode command in [apps/unity/README.md](../../apps/unity/README.md)). Every test must pass.
4. Switch the build target to iOS (**File → Build Profiles → iOS → Switch Platform**) and confirm it succeeds.
5. Run `git status`. Commit any files Unity regenerated (for example the rest of `ProjectSettings/` and `Packages/packages-lock.json`) to the PR branch; nothing under `Library/`, `Temp/`, `Logs/`, or `UserSettings/` should appear.
6. Paste the Console state, the Test Runner result, and the `git status` output into the PR.

The static checks in the "Unity project (static)" job run in CI on every PR regardless.

## Rules

- A red check is fixed by fixing its cause. Weakening, skipping, or deleting a check needs a written justification and Product Owner approval ([testing-policy.md](testing-policy.md), [human-approval-gates.md](human-approval-gates.md)).
- Branch protection (required checks on `main`) is a repository setting the Product Owner controls. Recommended now that TP-001 has landed: require "Docs consistency", "Rust core (ubuntu-latest)", and "Rust core (macos-15)"; add "Unity project (static)" once TP-002 lands.

VS-009 advances current consumers to ABI6 under ADR0032, preserving old ABI1–5 layouts/fixtures/consumer sources and behavioral tests. New tests cover bounded chronological actor queries, pure inspection, native failure/publication/fixture rules, strict managed decoding, returning-drag cancellation, scaled mesh picking and actual scene tap-only inspector behavior. Device evidence remains separate.

ADR0033 adds independent resident-vitals goldens, closed tag/health-life invariant tests, digest-field and previous-hash regressions, retained LOD/camera/replay state checks and ABI7/schema2 native/managed fixtures. Current C consumer v7 retains v6 properties; prior fixture/consumer files remain. Player panel tests require five actual fields, including both sex tags and valid constructed unhealthy/dead samples (not spawned residents).

VS-010 implements accepted ADR0034 with finite-episode, continuous-routine, provenance, rollback, derived-index reconstruction and separate-process replay coverage. ABI8 uses the retained positive C consumer checks plus actual illness/recovery/consequence checks in `abi-v8-smoke.c`; ABI1–7 fixtures and consumer sources remain. Current managed/native inspection schema3 and consequences schema1 are strict and bounded. Focused checks are used during editing; complete relevant suites and the same five hosted checks remain required for merge.

Issue #53 implements accepted ADR0025 in `ws-persist` (save format 3): version-pinned baseline regeneration (`baseline`), canonical structural deltas (`structural_overrides`, `notables`, `tombstones`, `spawned`), separated node LOD (`node_lod`), dataset content SHA-256 verification against `BaselineRegistry`, and explicit atomic migration (`migrate_to_v3`) while keeping format-1 (`tests/saves/toy-v1.wssave`) and format-2 (`tests/saves/toy-v2.wssave`) fixtures and adding `tests/saves/toy-v3.wssave`.


The first ARM Linux run at `2079dc51577bc31de380adb3355c3b80212105b0` acquired a runner and passed its architecture assertion, then exposed five test-buffer declarations hard-coded to signed `i8`. On ARM Linux, C `char` is unsigned; the existing exported functions correctly use `std::ffi::c_char`, so those tests failed to compile. The declarations now use `c_char` and byte comparisons use one-byte representations, retaining the exact existing UTF-8/truncation/sentinel assertions. Production exports, layouts, status codes and test expectations are unchanged.