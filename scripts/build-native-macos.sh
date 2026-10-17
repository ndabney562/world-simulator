#!/bin/sh
set -eu
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
case "$(uname -m)" in
  arm64) target=aarch64-apple-darwin ;;
  x86_64) target=x86_64-apple-darwin ;;
  *) echo 'Unsupported Editor architecture' >&2; exit 1 ;;
esac
cd "$repo/core"
cargo build --release --locked -p ws-ffi --target "$target"
plugin="$repo/apps/unity/Assets/Plugins/macOS"
mkdir -p "$plugin"
cp "target/$target/release/libws_ffi.dylib" "$plugin/libws_ffi.dylib"
printf 'Built %s plugin: %s\nRestart Unity before testing a rebuilt loaded plugin.\n' "$target" "$plugin/libws_ffi.dylib"
