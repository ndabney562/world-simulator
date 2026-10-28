#!/bin/sh
# Full export, deliberately local/manual: no CI licence secret or deployment.
set -eu
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
editor_root=${WORLD_SIM_UNITY_ROOT:-/Applications/Unity/Hub/Editor/6000.3.26f1}
"$repo/scripts/build-native-webgl.sh"
python3 "$repo/scripts/run_unity_web.py"
python3 "$repo/scripts/collect_web_notices.py"
python3 "$repo/scripts/web_build_manifest.py" "$repo/apps/unity/Builds/DesktopWeb"
