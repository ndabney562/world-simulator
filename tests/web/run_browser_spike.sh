#!/bin/sh
# Compiles and runs the WebAssembly browser compatibility spike test.
set -eu
repo=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)

# 1. Ensure WebGL native static library is built
"$repo/scripts/build-native-webgl.sh"

# Locate Unity Emscripten
emscripten_dir="/Applications/Unity/Hub/Editor/6000.3.26f1/PlaybackEngines/WebGLSupport/BuildTools/Emscripten"
if [ ! -d "$emscripten_dir" ]; then
  emscripten_dir=$(find /Applications/Unity/Hub/Editor -name WebGLSupport -exec echo "{}/BuildTools/Emscripten" \; 2>/dev/null | head -n 1)
fi

export EM_CONFIG="$emscripten_dir/.emscripten"
export PATH="$emscripten_dir/emscripten:$PATH"

out_dir="$repo/tests/web/build"
mkdir -p "$out_dir"

printf 'Compiling WebAssembly spike test runner...\n'
emcc \
     -I"$repo/core/crates/ws-ffi/include" \
     "$repo/tests/web/browser_slice_test.c" \
     "$repo/core/target/webgl-mvp/wasm32-unknown-emscripten/release/libws_ffi.a" \
     -o "$out_dir/browser_slice_test.js" \
     -sINITIAL_MEMORY=64MB \
     -sALLOW_MEMORY_GROWTH=1 \
     -sSTACK_SIZE=512KB

printf 'Executing spike runner in Node.js runtime...\n'
node "$out_dir/browser_slice_test.js"
