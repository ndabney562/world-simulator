# Continuous Integration

CI is built up incrementally. It checks only what exists; a check is added when the code it checks is added, not before. No job pretends to cover something that does not exist yet.

Workflow: [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml). It runs on every pull request, on pushes to `main`, and on manual dispatch.

## Current checks

| Job | Runs on | What it checks | Status |
|---|---|---|---|
| Docs consistency | Linux | `scripts/check_docs.py`: every relative Markdown link resolves; the ADR index lists every ADR exactly once with a matching status | Active |
| Rust core | Linux (x86-64) and macOS 14 (arm64) | In `core/`, with the toolchain pinned in `core/rust-toolchain.toml`: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` (includes the no-wall-clock lint from `core/clippy.toml`), `cargo build --workspace --all-targets --locked`, `cargo test --workspace --locked`; then `scripts/check_core_deps.py` (crate dependency direction, below) and its tests (`python3 -m unittest discover -s scripts -p 'test_*.py'`) | Active. There is no skip path: a missing `core/` workspace fails the job |
| Tier 1 golden tests | Linux (x86-64) and macOS 14 (arm64), inside the Rust core job's `cargo test` | `ws-math` cell IDs, sector membership, and re-expression against `core/crates/ws-math/tests/golden/cell_ids.txt`; any difference on either platform fails the build | Active (TP-005) |

The repository is public, so GitHub-hosted Linux and macOS runners cost nothing (ADR 0007).

## Running the checks locally

```sh
python3 scripts/check_docs.py
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
| 0 Foundation | `ws-math`, `ws-time`, `ws-rng` |
| 1 World model and events | `ws-world`, `ws-events` |
| 2 Domain | `ws-ecs`, `ws-data`, `ws-procgen`, `ws-agents`, `ws-physics`, `ws-lod`, `ws-interventions`, `ws-persist`, `ws-snapshot` |
| 3 Orchestration | `ws-sim` |
| 4 Test support | `ws-testkit` (allowed only as a dev-dependency) |
| 5 C ABI | `ws-ffi` |

Domain crates do not depend on each other (same-layer edges fail); they share types through `ws-world` and `ws-events`. A workspace crate missing from the table fails the check, so each new crate gets a deliberate place. The script also fails on any dependency (of any kind) on Unity, rendering, windowing, platform-API, or visual-provider crates (`wgpu`, `bevy*`, `winit`, `objc*`, `cesium*`, `web-sys`, `windows*`, and others listed in the script). The table lives in the script and here; change both together.

## Planned additions

Each is added by the issue that introduces what it checks.

| Check | Added by |
|---|---|
| Unity project validation; Unity edit-mode tests if Unity licence activation works in CI on the free plan, otherwise a documented local check on the Product Owner's Mac (S-6) | TP-002 |
| Native library build for `aarch64-apple-ios`; C header generation check (`cbindgen` output committed and up to date) | TP-003 |
| Tier 1 golden tests for procedural generation checked on both Linux and macOS | TP-006 |
| Tier 2 replay tests (separate processes, different thread counts) | TP-008 |
| Save fixture compatibility tests | TP-009 |
| Event and causality invariant tests | TP-008, TP-010 |
| iOS Unity build (if feasible on free runners) | TP-011 |
| Nightly: soak, Tier 3 drift report, benchmarks | When the first long-running scenario exists |

## Rules

- A red check is fixed by fixing its cause. Weakening, skipping, or deleting a check needs a written justification and Product Owner approval ([testing-policy.md](testing-policy.md), [human-approval-gates.md](human-approval-gates.md)).
- Branch protection (required checks on `main`) is a repository setting the Product Owner controls. Recommended now that TP-001 has landed: require "Docs consistency", "Rust core (ubuntu-latest)", and "Rust core (macos-14)".
