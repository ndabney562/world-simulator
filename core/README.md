# core: Rust simulation core

The headless simulation core of the World Simulator: a Cargo workspace that owns all authoritative simulation state. It has no Unity, rendering, platform, or visual-provider dependencies, and builds and tests on Linux and macOS ([17-repository-structure.md](../docs/architecture/17-repository-structure.md) §18.2).

## Layout

| Path | Purpose |
|---|---|
| `Cargo.toml` | Workspace: members, shared package settings, shared lints, profiles |
| `Cargo.lock` | Committed; CI builds with `--locked` |
| `rust-toolchain.toml` | Exact pinned Rust version, with `rustfmt` and `clippy` |
| `clippy.toml` | `disallowed-methods`: no wall-clock reads (`SystemTime::now`, `Instant::now`) |
| `crates/ws-ecs/` | Entity IDs and sorted component columns iterated in ascending ID order ([README](crates/ws-ecs/README.md)) |
| `crates/ws-math/` | Coordinates and frames: WGS84, ECEF, ENU, S2-style cell IDs, surface positions ([README](crates/ws-math/README.md)) |
| `crates/ws-rng/` | SipHash-2-4 derivations and counter-based random streams ([README](crates/ws-rng/README.md)) |
| `crates/ws-time/` | Simulation time, durations, UTC calendar, system steps ([README](crates/ws-time/README.md)) |
| `crates/ws-events/` | Typed factual history, causal references and read-only indexed queries ([README](crates/ws-events/README.md)) |
| `crates/ws-cli/` | Headless command/event recorder and independent debug-tape replay ([README](crates/ws-cli/README.md)) |
| `crates/ws-sim/` | The world, its clock, command queue, journal and tick loop ([README](crates/ws-sim/README.md)) |
| `crates/ws-world/` | The toy world: buildings, walking residents, procedural identity, state hash ([README](crates/ws-world/README.md)) |

Crates are added only when an issue needs them. Each new crate must be added to the layer table in [`scripts/check_core_deps.py`](../scripts/check_core_deps.py) and in [ci.md](../docs/process/ci.md).

## Building and testing

`rustup` installs the pinned toolchain automatically the first time `cargo` runs in this directory.

```sh
cd core
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --all-targets --locked
cargo test --workspace --locked
python3 ../scripts/check_core_deps.py
```

These are the same checks CI runs ([ci.md](../docs/process/ci.md)).

## Rules enforced here

- `unsafe_code = "deny"` for every crate. Only `ws-ffi` (TP-003) is expected to override it.
- `missing_docs`: every public item has a doc comment.
- `clippy::disallowed_methods = "deny"`: simulation logic never reads the wall clock ([25-determinism-policy.md](../docs/architecture/25-determinism-policy.md)).
- Release builds keep integer overflow checks on, so debug and release builds cannot diverge by silent wrapping.
- Crate dependency direction is checked by `scripts/check_core_deps.py`.

## TP-008 adapter prerequisite

[ADR 0019](../docs/adr/0019-command-journal-and-world-events.md) is accepted; its documentation PR must land first. The command/event implementation stays independent of the unmerged TP-003 bridge. It implements the Rust core and headless acceptance path. If PR #42 lands, its final ABI must be reconciled with ADR 0019 §8 before the command system reaches Unity; ABI v1 transport NoOp must not be described as a journaled semantic command. No C#/ABI upgrade is claimed here.

## Vertical Slice building baseline

VS-002 adds `ws_world::slice::SliceWorld::new(seed)`: the four real block 6049
buildings, their exact integer geometry and source identities, plus all 58
context footprints. See the [ws-world contract](crates/ws-world/README.md#real-block-baseline-vs-002).
The toy world remains the input to the existing simulator, CLI and bridge until
the corresponding resident/render/save slice issues integrate those layers.
