#!/bin/sh
# Builds ws-ffi as a static library for Unity WebGL using Unity's bundled Emscripten.
# Installs libws_ffi.a with WebGL-only plugin import settings into Assets/Plugins/WebGL.
set -eu
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
target=wasm32-unknown-emscripten

# Locate Unity bundled Emscripten
emscripten_dir="/Applications/Unity/Hub/Editor/6000.3.26f1/PlaybackEngines/WebGLSupport/BuildTools/Emscripten"
if [ ! -d "$emscripten_dir" ]; then
  # Fallback to general editor discovery if specific version differs
  emscripten_dir=$(find /Applications/Unity/Hub/Editor -name WebGLSupport -exec echo "{}/BuildTools/Emscripten" \; 2>/dev/null | head -n 1)
fi

if [ -z "$emscripten_dir" ] || [ ! -d "$emscripten_dir" ]; then
  echo "Error: Unity WebGLSupport Emscripten toolchain not found." >&2
  exit 1
fi

export EM_CONFIG="$emscripten_dir/.emscripten"
export PATH="$emscripten_dir/emscripten:$PATH"

cd "$repo/core"
rustup target add "$target" >/dev/null 2>&1 || true

# Compile with position-independent code (-fPIC) and bypass wasm-opt flag mismatches (-C link-arg=-O0)
CFLAGS="-fPIC" RUSTFLAGS="-C link-arg=-O0" cargo build --release --locked -p ws-ffi --target "$target"

plugin="$repo/apps/unity/Assets/Plugins/WebGL"
mkdir -p "$plugin"
cp "target/$target/release/libws_ffi.a" "$plugin/libws_ffi.a"

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
      settings:
        DefaultValueInitialized: true
  - first:
      WebGL: WebGL
    second:
      enabled: 1
      settings: {}
  userData: 
  assetBundleName: 
  assetBundleVariant: 
META

printf 'Built %s plugin: %s\n' "$target" "$plugin/libws_ffi.a"
