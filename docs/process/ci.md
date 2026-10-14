# Continuous Integration

CI is built up incrementally. It checks only what exists; a check is added when the code it checks is added, not before. No job pretends to cover something that does not exist yet.

Workflow: [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml). It runs on every pull request, on pushes to `main`, and on manual dispatch.

## Current checks

| Job | Runs on | What it checks | Status |
|---|---|---|---|
| Docs consistency | Linux | `scripts/check_docs.py`: every relative Markdown link resolves; the ADR index lists every ADR exactly once with a matching status | Active |
| Rust core | Linux (x86-64) and macOS 14 (arm64) | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo build --workspace --all-targets --locked`, `cargo test --workspace --locked`, all in `core/` | **Skipped with a notice until `core/Cargo.toml` exists.** TP-001 creates the workspace and removes the skip, so that a missing workspace fails CI from then on |

The repository is public, so GitHub-hosted Linux and macOS runners cost nothing (ADR 0007).

## Running the checks locally

```sh
python3 scripts/check_docs.py
cd core
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

## Planned additions

Each is added by the issue that introduces what it checks.

| Check | Added by |
|---|---|
| Remove the Rust skip; commit `Cargo.lock`; pinned toolchain; no-wall-clock lint (`clippy.toml` `disallowed-methods`); crate dependency-direction test | TP-001 |
| Unity project validation; Unity edit-mode tests if Unity licence activation works in CI on the free plan, otherwise a documented local check on the Product Owner's Mac (S-6) | TP-002 |
| Native library build for `aarch64-apple-ios`; C header generation check (`cbindgen` output committed and up to date) | TP-003 |
| Tier 1 golden tests checked on both Linux and macOS | TP-005, TP-006 |
| Tier 2 replay tests (separate processes, different thread counts) | TP-008 |
| Save fixture compatibility tests | TP-009 |
| Event and causality invariant tests | TP-008, TP-010 |
| iOS Unity build (if feasible on free runners) | TP-011 |
| Nightly: soak, Tier 3 drift report, benchmarks | When the first long-running scenario exists |

## Rules

- A red check is fixed by fixing its cause. Weakening, skipping, or deleting a check needs a written justification and Product Owner approval ([testing-policy.md](testing-policy.md), [human-approval-gates.md](human-approval-gates.md)).
- Branch protection (required checks on `main`) is a repository setting the Product Owner controls. Recommended once TP-001 lands: require "Docs consistency" and both "Rust core" jobs.
