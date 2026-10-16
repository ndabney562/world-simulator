# Continuous Integration

CI is built up incrementally. It checks only what exists; a check is added when the code it checks is added, not before. No job pretends to cover something that does not exist yet.

Workflow: [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml). It runs on every pull request, on pushes to `main`, and on manual dispatch.

## Current checks

| Job | Runs on | What it checks | Status |
|---|---|---|---|
| Docs consistency | Linux | `scripts/check_docs.py`: every relative Markdown link resolves; the ADR index lists every ADR exactly once with a matching status | Active |
| Unity project (static) | Linux | `scripts/check_unity_project.py` on `apps/unity/` without the Unity Editor: Unity 6 editor version recorded consistently; only Unity-registry packages; every asset has a `.meta` and GUIDs are unique; scenes and settings are text-serialized; the assembly layout (`WorldSim.Bridge` ← `WorldSim.Rendering`; Editor-only tests); no code in `Providers/`; build scene and render pipeline references resolve; Unity's generated folders are ignored and untracked. Then the script's own tests | Active |
| Rust core | Linux (x86-64) and macOS 14 (arm64) | In `core/`, with the toolchain pinned in `core/rust-toolchain.toml`: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` (includes the no-wall-clock lint from `core/clippy.toml`), `cargo build --workspace --all-targets --locked`, `cargo test --workspace --locked`; then `scripts/check_core_deps.py` (crate dependency direction, below) and its tests (`python3 -m unittest discover -s scripts -p 'test_*.py'`) | Active. There is no skip path: a missing `core/` workspace fails the job |
| Tier 1 golden tests | Linux (x86-64) and macOS 14 (arm64), inside the Rust core job's `cargo test` | `ws-math` cell IDs, sector membership, and re-expression against `core/crates/ws-math/tests/golden/cell_ids.txt`; `ws-rng` SipHash-2-4 reference vectors and derivation and draw values; `ws-world` procedural IDs, residents per building, and target choices against `core/crates/ws-world/tests/golden/toy_world.txt`. Any difference on either platform fails the build | Active (TP-005, TP-006) |
| Tier 2 replay tests | Linux (x86-64) and macOS 14 (arm64), inside `cargo test` | `ws-sim` and `ws-world`: identical per-tick state hashes for 10,000 ticks with the same seed, with and without scripted LOD transitions, including a run in a separate process; insertion-order independence. Compared within one platform, never across | Active (TP-006, TP-007) |
| Simulation LOD tests | Linux (x86-64) and macOS 14 (arm64), inside `cargo test` | `ws-world` `tests/lod.rs` and `src/lod_tests.rs` (ADR 0017 §14): zero-time round trips, conservation properties, L1/L2 equivalence over one simulated hour, the invariant checker against injected faults, and `lod_soak_short` (1,000 agents, 30 simulated minutes, about 3 minutes in a debug build) | Active (TP-007) |
| Long-running LOD tests | Linux (`ubuntu-latest`), daily at 07:23 UTC and manual dispatch: `cargo test --release --locked -p ws-world --test lod -- --ignored` | `l2_matches_l1_over_eight_hours` and `lod_soak_long` (3,000 agents, 24 simulated hours), marked `#[ignore]` as nightly tests ([testing-policy](testing-policy.md) §4) | Configured in [nightly.yml](../../.github/workflows/nightly.yml); schedule activates after merge to `main`. First measured run pending |

The TP-003 **Native bridge and header** job runs on macOS 14. It installs the workspace-pinned Rust toolchain and `aarch64-apple-ios`, installs cbindgen 0.29.2 as a build-only tool, verifies `ws_ffi.h` matches regenerated output, and builds the iOS target with `--release --locked`. FFI unit/error/layout tests are included in both Rust matrix jobs. Unity Editor integration/rendering tests remain a local verification step; static checks do not substitute for them.

The repository is public, so GitHub-hosted Linux and macOS runners cost nothing (ADR 0007).

## Nightly LOD workflow

[`.github/workflows/nightly.yml`](../../.github/workflows/nightly.yml) runs the two existing ignored LOD tests in release mode on Linux. It uses the same pinned-toolchain installation and Cargo cache as PR CI, read-only contents permission, and no secrets. Failures fail the job; tests and PR checks are unchanged. The daily schedule is 07:23 UTC (03:23 New York during daylight saving time, 02:23 during standard time); GitHub may delay scheduled runs. Scheduled runs use `main`.

The job has a **provisional 350-minute ceiling**, below six hours. This is not a measured timeout: #29 remains verification-incomplete until a hosted run records both test results and wall-clock duration. Set the final timeout from that run with headroom for installation, cold compilation, and runner variation. If the tests approach the hosted-runner limit, propose separate jobs rather than shortening them.

GitHub [requires the workflow to exist on the default branch for manual dispatch](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow). This new file therefore cannot supply the issue's requested pre-merge dispatch evidence through the documented flow yet. Noah must resolve that acceptance-order constraint on #29 before merge; do not claim ordinary PR CI ran the ignored tests. Once the workflow is registered on `main`, a writer can select a branch in **Actions → Nightly LOD tests → Run workflow**, or run:

```sh
gh workflow run nightly.yml --repo ndabney562/world-simulator --ref codex/29-nightly-lod
gh run list --repo ndabney562/world-simulator --workflow nightly.yml --branch codex/29-nightly-lod
```

Link the resulting run on the PR, record job duration and tested SHA, confirm `l2_matches_l1_over_eight_hours` and `lod_soak_long` both passed, and calibrate the timeout. No extra notification service is configured; GitHub's notification preferences apply.

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
| 2 Domain | `ws-data`, `ws-procgen`, `ws-agents`, `ws-physics`, `ws-lod`, `ws-interventions`, `ws-persist`, `ws-snapshot` |
| 3 Orchestration | `ws-sim` |
| 4 Test support | `ws-testkit` (allowed only as a dev-dependency) |
| 5 C ABI | `ws-ffi` |

Domain crates do not depend on each other (same-layer edges fail); they share types through `ws-world` and `ws-events`. A workspace crate missing from the table fails the check, so each new crate gets a deliberate place. The script also fails on any dependency (of any kind) on Unity, rendering, windowing, platform-API, or visual-provider crates (`wgpu`, `bevy*`, `winit`, `objc*`, `cesium*`, `web-sys`, `windows*`, and others listed in the script). The table lives in the script and here; change both together.

## Planned additions

Each is added by the issue that introduces what it checks.

| Check | Added by |
|---|---|
| Unity edit-mode tests in CI, once the Product Owner adds the licence secrets (S-6, below) | Follow-up to TP-002 |

| Tier 2 replay tests with different thread counts, and journal replay | TP-008 |
| Save fixture compatibility tests | TP-009 |
| Event and causality invariant tests | TP-008, TP-010 |
| iOS Unity build (if feasible on free runners) | TP-011 |
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
- Branch protection (required checks on `main`) is a repository setting the Product Owner controls. Recommended now that TP-001 has landed: require "Docs consistency", "Rust core (ubuntu-latest)", and "Rust core (macos-14)"; add "Unity project (static)" once TP-002 lands.
