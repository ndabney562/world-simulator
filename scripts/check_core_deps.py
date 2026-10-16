#!/usr/bin/env python3
"""Crate dependency-direction check for the Rust core, run in CI.

Reads `cargo metadata` for the workspace in core/ and fails if:

1. A workspace crate is not listed in LAYERS (the table must be updated
   when a crate is added, so its place in the direction is decided).
2. A workspace crate has a normal or build dependency on a workspace crate
   in the same or a later layer. Direction follows
   docs/architecture/17-repository-structure.md section 18.2:
   foundation -> world/events -> domain -> ws-sim -> ws-ffi. Domain crates
   do not depend on each other; they share ws-world and ws-events types.
3. A workspace crate depends on ws-testkit other than as a dev-dependency.
4. A workspace crate depends (of any kind) on a forbidden external crate:
   Unity, rendering, platform, or visual-provider bindings (section 18.2
   rule 2; ADR 0008).

Uses only the Python standard library. Exits non-zero on any failure.
The layer table is documented in docs/process/ci.md; change both together.
"""

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CORE = ROOT / "core"

# Lower number = earlier layer. A crate may depend only on strictly lower layers.
LAYERS = {
    # Foundation
    "ws-math": 0,
    "ws-time": 0,
    "ws-rng": 0,
    "ws-ecs": 0,
    # World model and events
    "ws-world": 1,
    "ws-events": 1,
    # Domain crates
    "ws-data": 2,
    "ws-procgen": 2,
    "ws-agents": 2,
    "ws-physics": 2,
    "ws-lod": 2,
    "ws-interventions": 2,
    "ws-persist": 2,
    "ws-snapshot": 2,
    # Orchestration
    "ws-sim": 3,
    # Test support: may depend on anything up to ws-sim; used only as a dev-dependency.
    "ws-testkit": 4,
    # Host adapters: C ABI Unity links and headless CLI
    "ws-ffi": 5,
    "ws-cli": 5,
}

# Crates that may only be dev-dependencies of other workspace crates.
DEV_ONLY = {"ws-testkit"}

# External crates the core must never depend on (matched against the crate name).
FORBIDDEN_EXTERNAL = [
    # Unity and visual providers
    r"unity.*",
    r"cesium.*",
    r"google.*",
    # Rendering and windowing
    r"wgpu.*",
    r"bevy.*",
    r"winit",
    r"glium",
    r"glow",
    r"glutin.*",
    r"gl",
    r"vulkano.*",
    r"ash",
    r"metal",
    r"sdl2.*",
    r"three-d.*",
    # Platform APIs
    r"objc.*",
    r"objc2.*",
    r"cocoa.*",
    r"core-foundation.*",
    r"core-graphics.*",
    r"uikit.*",
    r"jni",
    r"ndk.*",
    r"android.*",
    r"windows.*",
    r"winapi",
    r"web-sys",
    r"wasm-bindgen.*",
]
_FORBIDDEN = re.compile(r"^(?:" + "|".join(FORBIDDEN_EXTERNAL) + r")$")


def check(metadata):
    """Returns a list of error strings for the given `cargo metadata` JSON."""
    errors = []
    members = set(metadata["workspace_members"])
    packages = [p for p in metadata["packages"] if p["id"] in members]
    names = {p["name"] for p in packages}

    for pkg in sorted(packages, key=lambda p: p["name"]):
        name = pkg["name"]
        if name not in LAYERS:
            errors.append(f"{name}: not in the LAYERS table of scripts/check_core_deps.py")
            continue
        for dep in sorted(pkg["dependencies"], key=lambda d: (d["name"], d["kind"] or "")):
            dep_name = dep["name"]
            kind = dep["kind"] or "normal"
            if dep_name in names or dep_name in LAYERS:
                if dep_name in DEV_ONLY:
                    if kind != "dev":
                        errors.append(f"{name} -> {dep_name}: {dep_name} may only be a dev-dependency")
                    continue
                if kind == "dev":
                    continue  # tests may use any workspace crate
                if dep_name not in LAYERS:
                    continue  # reported once as an unknown member above
                if LAYERS[dep_name] >= LAYERS[name]:
                    errors.append(
                        f"{name} (layer {LAYERS[name]}) -> {dep_name} (layer {LAYERS[dep_name]}): "
                        "dependencies must point to an earlier layer (17-repository-structure.md §18.2)"
                    )
            elif _FORBIDDEN.match(dep_name):
                errors.append(
                    f"{name} -> {dep_name}: forbidden external crate "
                    "(no Unity, rendering, platform, or provider code in core/)"
                )
    return errors


def load_metadata():
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
        cwd=CORE,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return json.loads(out)


def main():
    errors = check(load_metadata())
    for error in errors:
        print(f"error: {error}", file=sys.stderr)
    if errors:
        print(f"{len(errors)} dependency error(s)", file=sys.stderr)
        return 1
    print("core crate dependencies OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
