#!/bin/sh
set -eu
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
if [ "$(cbindgen --version)" != 'cbindgen 0.29.2' ]; then
  echo 'Install cbindgen 0.29.2: cargo install cbindgen --version 0.29.2 --locked' >&2
  exit 1
fi
header=$(mktemp)
trap 'rm -f "$header"' EXIT HUP INT TERM
cd "$repo/core"
cbindgen --config crates/ws-ffi/cbindgen.toml --crate ws-ffi --output "$header"
diff -u crates/ws-ffi/include/ws_ffi.h "$header"
