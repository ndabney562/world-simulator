#!/bin/sh
# Experimental desktop browser build (#112 / accepted ADR0035).
# Native builds retain their unwind/poisoning contract.
set -eu
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
unity_version=6000.3.26f1
rust_version=1.97.0
target=wasm32-unknown-emscripten
editor_root=${WORLD_SIM_UNITY_ROOT:-/Applications/Unity/Hub/Editor/$unity_version}
emscripten_dir="$editor_root/PlaybackEngines/WebGLSupport/BuildTools/Emscripten"
if [ ! -f "$emscripten_dir/.emscripten" ]; then
  echo "Missing Unity $unity_version WebGL module: $emscripten_dir" >&2
  exit 1
fi
if ! "$editor_root/Unity.app/Contents/MacOS/Unity" -version 2>/dev/null | head -n 1 | grep -Fx "$unity_version" >/dev/null; then
  echo "Expected Unity $unity_version at $editor_root" >&2
  exit 1
fi
if ! rustup component list --toolchain "$rust_version" --installed | grep '^rust-src' >/dev/null; then
  echo "Install pinned standard-library sources: rustup component add --toolchain $rust_version rust-src" >&2
  exit 1
fi
export EM_CONFIG="$emscripten_dir/.emscripten"
export PATH="$emscripten_dir/emscripten:$PATH"
# Use a separate cache: browser fail-stop flags must never contaminate native builds.
export CARGO_TARGET_DIR="$repo/core/target/webgl-mvp"
cd "$repo/core"
# Unity's Binaryen 112 cannot consume newer Rust std Wasm features. Rebuild std,
# not only this crate, with one compatible feature set. Bootstrap is scoped to
# this pinned build-std command; no global toolchain/environment change.
CFLAGS="-fPIC" RUSTC_BOOTSTRAP=1 \
RUSTFLAGS="-C panic=abort -C target-cpu=mvp -C target-feature=-bulk-memory,-bulk-memory-opt,-reference-types,-call-indirect-overlong,-exception-handling,-multivalue,-nontrapping-fptoint,-sign-ext" \
cargo +"$rust_version" rustc -Zbuild-std=std,panic_abort --locked --release -p ws-ffi --lib --crate-type staticlib --target "$target"
plugin="$repo/apps/unity/Assets/Plugins/WebGL"
mkdir -p "$plugin"
# Preserve the archive timestamp when bytes match: Unity otherwise relinks
# the entire player even for a page-only change.
if ! cmp -s "$CARGO_TARGET_DIR/$target/release/libws_ffi.a" "$plugin/libws_ffi.a"; then
  cp "$CARGO_TARGET_DIR/$target/release/libws_ffi.a" "$plugin/libws_ffi.a"
fi
cat > "$plugin/libws_ffi.a.meta" <<'META'
fileFormatVersion: 2
guid: 8b3c9d1a4e5f6071a2b3c4d5e6f7a8b9
PluginImporter:
  externalObjects: {}
  serializedVersion: 3
  iconMap: {}
  executionOrder: {}
  defineConstraints: []
  isPreloaded: 0
  isOverridable: 0
  isExplicitlyReferenced: 0
  validateReferences: 1
  platformData:
  - first:
      Any:
    second:
      enabled: 0
      settings: {}
  - first:
      Editor: Editor
    second:
      enabled: 0
      settings: {}
  - first:
      WebGL: WebGL
    second:
      enabled: 1
      settings: {}
  userData:
  assetBundleName:
  assetBundleVariant:
META
printf 'Built experimental WebGL archive: %s\n' "$plugin/libws_ffi.a"
