# core: Rust simulation core

The headless simulation core of the World Simulator: a Cargo workspace that owns all authoritative simulation state. It has no Unity, rendering, platform, or visual-provider dependencies, and builds and tests on Linux and macOS ([17-repository-structure.md](../docs/architecture/17-repository-structure.md) §18.2).

## Layout

| Path | Purpose |
|---|---|
| `Cargo.toml` | Workspace: members, shared package settings, shared lints, profiles |
| `Cargo.lock` | Committed; CI builds with `--locked` |
| `rust-toolchain.toml` | Exact pinned Rust version, with `rustfmt` and `clippy` |
| `clippy.toml` | `disallowed-methods`: no wall-clock reads (`SystemTime::now`, `Instant::now`) |
| `crates/ws-sim/` | The world and its tick loop ([README](crates/ws-sim/README.md)) |

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
