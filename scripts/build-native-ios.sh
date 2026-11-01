#!/bin/sh
# Builds ws-ffi as a static library for iPhone and installs it, with iOS-only import settings, into the
# git-ignored Unity plugin folder. Needs macOS with Xcode command line tools and the aarch64-apple-ios target.
set -eu
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
target=aarch64-apple-ios
cd "$repo/core"
rustup target add "$target" >/dev/null 2>&1 || true
cargo build --release --locked -p ws-ffi --target "$target"
plugin="$repo/apps/unity/Assets/Plugins/iOS"
mkdir -p "$plugin"
cp "target/$target/release/libws_ffi.a" "$plugin/libws_ffi.a"
# Unity's default import enables every platform; the Mac Editor must not try to load an iOS archive.
cat > "$plugin/libws_ffi.a.meta" <<'META'
fileFormatVersion: 2
guid: 4f1c2a7e9b3d4c58a6d0e1f2a3b4c5d6
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
      iPhone: iOS
    second:
      enabled: 1
      settings: {}
  userData: 
  assetBundleName: 
  assetBundleVariant: 
META
printf 'Built %s plugin: %s\nIn Unity choose WorldSim > Build iOS real block.\n' "$target" "$plugin/libws_ffi.a"
