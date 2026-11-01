#!/bin/bash
# Mandatory native evidence during the temporary hosted-runner fallback (#117).
set -euo pipefail
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo"
test "$(uname -s)" = Darwin
test "$(uname -m)" = arm64
if test -n "$(git status --porcelain --untracked-files=all)"; then
  echo 'Commit or remove checkout changes before collecting exact-source evidence.' >&2
  exit 1
fi
tested_commit=$(git rev-parse HEAD)
native_tree=$(git rev-parse HEAD:core)
printf 'Tested commit: %s\nNative source tree: %s\n' "$tested_commit" "$native_tree"
sw_vers
uname -m
evidence_tmp=$(mktemp -d)
trap 'rm -rf "$evidence_tmp"' EXIT
cd core
native_target=aarch64-apple-darwin
export CARGO_TARGET_DIR="$repo/core/target"
channel=$(sed -n 's/^channel *= *"\(.*\)"/\1/p' rust-toolchain.toml)
test -n "$channel"
rustc +"$channel" --version
cargo +"$channel" --version
cargo +"$channel" fmt --all --check
cargo +"$channel" clippy --target "$native_target" --workspace --all-targets --locked -- -D warnings
cargo +"$channel" build --target "$native_target" --workspace --all-targets --locked
cargo +"$channel" test --target "$native_target" --workspace --locked
cc -arch arm64 -std=c11 -Wall -Wextra -Werror -Icrates/ws-ffi/include \
  crates/ws-ffi/tests/abi-v11-smoke.c -L"target/$native_target/debug" -lws_ffi \
  -Wl,-rpath,"$PWD/target/$native_target/debug" -o "$evidence_tmp/abi-smoke"
"$evidence_tmp/abi-smoke"
for run in first second; do
  cargo +"$channel" run --target "$native_target" --locked -p ws-cli -- run --seed 42 --ticks 2000 --hash-every 200 --script crates/ws-cli/tests/fixtures/ci-slice.txt > "$evidence_tmp/slice-$run.txt"
done
diff "$evidence_tmp/slice-first.txt" "$evidence_tmp/slice-second.txt"
cargo +"$channel" run --target "$native_target" --locked -p ws-cli -- save --seed 42 --ticks 200 --hash-every 100 --script crates/ws-cli/tests/fixtures/ci-slice.txt --out "$evidence_tmp/slice.wssave"
cargo +"$channel" run --target "$native_target" --locked -p ws-cli -- load --file "$evidence_tmp/slice.wssave" --ticks 50 --hash-every 25
python3 ../scripts/check_core_deps.py
cd ..
python3 -m unittest discover -s scripts -p 'test_*.py' -v
# Existing checker enforces cbindgen 0.29.2; no credential or signing step.
PATH="$HOME/.cargo/bin:$PATH" scripts/check-ffi-header.sh
cd core
cargo +"$channel" build --release --locked -p ws-ffi --target aarch64-apple-ios
test "$(git rev-parse HEAD:core)" = "$native_tree"
test "$(git rev-parse HEAD)" = "$tested_commit"
test -z "$(git -C "$repo" status --porcelain --untracked-files=all)"
printf 'PASS: Apple Silicon native verification; tested commit %s; core tree %s\n' "$tested_commit" "$native_tree"
