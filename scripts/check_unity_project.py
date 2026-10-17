#!/usr/bin/env python3
"""Static checks on the Unity project at apps/unity/, run in CI.

The Unity Editor cannot run in CI yet (docs/process/ci.md, S-6), so these
checks cover what can be verified from the files alone:

1. ProjectSettings/ProjectVersion.txt names a Unity 6 editor, and
   apps/unity/README.md records the same version.
2. Packages/manifest.json uses only Unity packages from the default registry
   (no scoped registries, git, or file sources) and includes URP and the
   Test Framework.
3. Every file and folder under Assets/ has a .meta file, every .meta file has
   its asset, and every GUID is well formed and unique.
4. Scenes, assets, and ProjectSettings are text-serialized (Force Text).
5. The assembly definitions match the agreed layout: WorldSim.Bridge,
   WorldSim.Rendering (references Bridge), WorldSim.Tests.EditMode (Editor
   only); Bridge references no other WorldSim assembly.
6. Assets/WorldSim/Providers/ contains no code (TP-012 adds it).
7. The build-settings scene list and the default render pipeline resolve to
   assets in this project.
8. apps/unity/.gitignore ignores Unity's generated folders, and none of them
   is tracked by git.
9. The serialized iOS graphics API list contains exactly Metal, independent
   of the automatic-selection flag (issue #37).

Uses only the Python standard library. Exits non-zero on any failure.
"""

import json
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
PROJECT = REPO / "apps" / "unity"

EDITOR_VERSION = re.compile(r"^m_EditorVersion:\s*(6000\.\d+\.\d+f\d+)\s*$", re.M)
GUID = re.compile(r"^guid:\s*([0-9a-f]{32})\s*$", re.M)
YAML_HEADER = "%YAML 1.1"
TEXT_SERIALIZED = {".unity", ".asset", ".prefab", ".mat", ".anim", ".controller", ".lighting"}

REQUIRED_PACKAGES = {"com.unity.render-pipelines.universal", "com.unity.test-framework"}
PACKAGE_VERSION = re.compile(r"^\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?$")

ASSEMBLIES = {
    "Assets/WorldSim/Bridge/WorldSim.Bridge.asmdef": "WorldSim.Bridge",
    "Assets/WorldSim/Rendering/WorldSim.Rendering.asmdef": "WorldSim.Rendering",
    "Assets/WorldSim/Tests/EditMode/WorldSim.Tests.EditMode.asmdef": "WorldSim.Tests.EditMode",
}
PLACEHOLDER_FOLDERS = ["Assets/WorldSim/Providers", "Assets/WorldSim/UI"]
NO_CODE_FOLDERS = ["Assets/WorldSim/Providers"]

GENERATED_FOLDERS = ["Library", "Temp", "Obj", "Logs", "UserSettings", "Builds"]


def read(path):
    return path.read_text(encoding="utf-8")


def check_editor_version(project, errors):
    version_file = project / "ProjectSettings" / "ProjectVersion.txt"
    if not version_file.is_file():
        errors.append("ProjectSettings/ProjectVersion.txt is missing")
        return
    match = EDITOR_VERSION.search(read(version_file))
    if not match:
        errors.append("ProjectVersion.txt does not name a Unity 6 editor (6000.x.yfz)")
        return
    readme = project / "README.md"
    if not readme.is_file() or match.group(1) not in read(readme):
        errors.append(f"apps/unity/README.md does not record the editor version {match.group(1)}")


def check_ios_graphics_api(project, errors):
    """Check the committed iOS API list, not Unity's selection-mode flag.

    Read only Unity's canonical graphics-API list block; another platform's
    API value or a similarly named field elsewhere must not satisfy iOS.
    Metal's enum value is 16, serialized as one little-endian u32: 10000000.
    """
    settings = project / "ProjectSettings" / "ProjectSettings.asset"
    if not settings.is_file():
        errors.append("ProjectSettings/ProjectSettings.asset is missing")
        return
    sections = re.findall(
        r"^  m_BuildTargetGraphicsAPIs:[ \t]*\n((?:(?:  - |    )[^\n]*(?:\n|$))*)",
        read(settings), re.M,
    )
    if len(sections) != 1:
        errors.append("ProjectSettings.asset must contain one m_BuildTargetGraphicsAPIs list")
        return
    entries = re.split(r"^  - ", sections[0], flags=re.M)[1:]
    ios = [entry for entry in entries if re.match(r"m_BuildTarget:[ \t]*iOSSupport[ \t]*(?:\n|$)", entry)]
    if len(ios) != 1:
        errors.append("ProjectSettings.asset graphics API list must contain exactly one iOSSupport entry")
        return
    apis = re.findall(r"^    m_APIs:[ \t]*([0-9a-fA-F]*)[ \t]*$", ios[0], re.M)
    if apis != ["10000000"]:
        errors.append("ProjectSettings.asset iOSSupport graphics APIs must be exactly Metal (m_APIs: 10000000)")


def check_manifest(project, errors):
    manifest_file = project / "Packages" / "manifest.json"
    if not manifest_file.is_file():
        errors.append("Packages/manifest.json is missing")
        return
    try:
        manifest = json.loads(read(manifest_file))
    except json.JSONDecodeError as e:
        errors.append(f"Packages/manifest.json is not valid JSON: {e}")
        return
    if manifest.get("scopedRegistries"):
        errors.append("Packages/manifest.json declares scoped registries; packages need review (ADR 0007)")
    deps = manifest.get("dependencies", {})
    for name, version in sorted(deps.items()):
        if not name.startswith("com.unity."):
            errors.append(f"package {name} is not a Unity package; third-party packages need review (ADR 0007)")
        if not PACKAGE_VERSION.match(str(version)):
            errors.append(f"package {name} uses a non-registry source {version!r}")
    for name in sorted(REQUIRED_PACKAGES - deps.keys()):
        errors.append(f"Packages/manifest.json is missing {name}")


def asset_guids(project, errors):
    """Checks .meta pairing under Assets/ and returns {guid: asset path}."""
    assets = project / "Assets"
    guids = {}
    if not assets.is_dir():
        errors.append("Assets/ is missing")
        return guids
    for path in sorted(assets.rglob("*")):
        rel = path.relative_to(project).as_posix()
        if path.name.startswith(".") or path.name.endswith("~"):
            continue  # Unity ignores hidden and ~ entries and gives them no .meta
        if path.suffix == ".meta":
            asset = path.with_name(path.name[: -len(".meta")])
            if not asset.exists():
                errors.append(f"{rel} has no asset")
                continue
            match = GUID.search(read(path))
            if not match:
                errors.append(f"{rel} has no well-formed guid")
                continue
            guid = match.group(1)
            if guid in guids:
                errors.append(f"{rel} reuses guid {guid} of {guids[guid]}")
            else:
                guids[guid] = asset.relative_to(project).as_posix()
        elif not path.with_name(path.name + ".meta").is_file():
            errors.append(f"{rel} has no .meta file")
    return guids


def check_text_serialization(project, errors):
    candidates = [p for p in (project / "Assets").rglob("*") if p.suffix in TEXT_SERIALIZED]
    candidates += list((project / "ProjectSettings").glob("*.asset"))
    for path in sorted(candidates):
        with path.open("rb") as f:
            head = f.read(len(YAML_HEADER))
        if head != YAML_HEADER.encode():
            errors.append(f"{path.relative_to(project).as_posix()} is not text-serialized YAML")


def check_assemblies(project, errors):
    defs = {}
    for rel, expected in ASSEMBLIES.items():
        path = project / rel
        if not path.is_file():
            errors.append(f"{rel} is missing")
            continue
        try:
            data = json.loads(read(path))
        except json.JSONDecodeError as e:
            errors.append(f"{rel} is not valid JSON: {e}")
            continue
        if data.get("name") != expected:
            errors.append(f"{rel} is named {data.get('name')!r}, expected {expected!r}")
        defs[expected] = data
    for path in sorted((project / "Assets").rglob("*.asmdef")):
        rel = path.relative_to(project).as_posix()
        if rel not in ASSEMBLIES:
            errors.append(f"{rel} is not in the agreed assembly layout")
    bridge = defs.get("WorldSim.Bridge")
    if bridge is not None:
        own = [r for r in bridge.get("references", []) if r.startswith("WorldSim.")]
        if own:
            errors.append(f"WorldSim.Bridge must not reference other WorldSim assemblies: {own}")
    rendering = defs.get("WorldSim.Rendering")
    if rendering is not None:
        own = sorted(r for r in rendering.get("references", []) if r.startswith("WorldSim."))
        if own != ["WorldSim.Bridge"]:
            errors.append(f"WorldSim.Rendering must reference exactly WorldSim.Bridge, found {own}")
    tests = defs.get("WorldSim.Tests.EditMode")
    if tests is not None:
        if tests.get("includePlatforms") != ["Editor"]:
            errors.append("WorldSim.Tests.EditMode must be an Editor-only assembly")
        if "UNITY_INCLUDE_TESTS" not in tests.get("defineConstraints", []):
            errors.append("WorldSim.Tests.EditMode must be constrained to UNITY_INCLUDE_TESTS")


def check_folders(project, errors):
    for rel in PLACEHOLDER_FOLDERS:
        if not (project / rel).is_dir():
            errors.append(f"{rel}/ is missing")
    if not (project / "Assets/WorldSim/Providers/README.md").is_file():
        errors.append("Assets/WorldSim/Providers/README.md is missing")
    for rel in NO_CODE_FOLDERS:
        folder = project / rel
        code = sorted(p.relative_to(project).as_posix() for p in folder.rglob("*") if p.suffix in {".cs", ".asmdef", ".dll"})
        for c in code:
            errors.append(f"{c}: {rel}/ holds no code until TP-012")


def check_references(project, guids, errors):
    build = project / "ProjectSettings" / "EditorBuildSettings.asset"
    if build.is_file():
        text = read(build)
        scenes = re.findall(r"path:\s*(\S+)\s*\n\s*guid:\s*([0-9a-f]{32})", text)
        if not scenes:
            errors.append("EditorBuildSettings.asset lists no scenes")
        for path, guid in scenes:
            if guids.get(guid) != path:
                errors.append(f"EditorBuildSettings.asset scene {path} has guid {guid}, which is {guids.get(guid)}")
    else:
        errors.append("ProjectSettings/EditorBuildSettings.asset is missing")
    graphics = project / "ProjectSettings" / "GraphicsSettings.asset"
    if graphics.is_file():
        match = re.search(r"m_CustomRenderPipeline:\s*\{fileID:\s*\d+,\s*guid:\s*([0-9a-f]{32})", read(graphics))
        if not match:
            errors.append("GraphicsSettings.asset sets no default render pipeline")
        elif not guids.get(match.group(1), "").endswith(".asset"):
            errors.append("GraphicsSettings.asset default render pipeline is not an asset in this project")
    else:
        errors.append("ProjectSettings/GraphicsSettings.asset is missing")


def check_gitignore(project, errors, tracked=None):
    gitignore = project / ".gitignore"
    if not gitignore.is_file():
        errors.append("apps/unity/.gitignore is missing")
        return
    # Fold case classes such as [Ll]ibrary to library before matching.
    text = re.sub(r"\[([A-Za-z])[A-Za-z]\]", r"\1", read(gitignore)).lower()
    patterns = {line.strip() for line in text.splitlines()}
    for folder in GENERATED_FOLDERS:
        if f"/{folder.lower()}/" not in patterns:
            errors.append(f"apps/unity/.gitignore does not ignore {folder}/")
    if tracked is None:
        tracked = tracked_files(project)
    generated = {f.lower() for f in GENERATED_FOLDERS}
    for rel in tracked:
        if rel.split("/", 1)[0].lower() in generated:
            errors.append(f"{rel} is generated by Unity and must not be committed")


def tracked_files(project):
    try:
        out = subprocess.run(
            ["git", "ls-files", "-z", "--", "."], cwd=project, check=True, capture_output=True
        ).stdout.decode()
    except (OSError, subprocess.CalledProcessError):
        return []
    return [f for f in out.split("\0") if f]


def check(project, tracked=None):
    errors = []
    if not project.is_dir():
        return [f"{project} is missing"]
    check_editor_version(project, errors)
    check_manifest(project, errors)
    check_ios_graphics_api(project, errors)
    guids = asset_guids(project, errors)
    check_text_serialization(project, errors)
    check_assemblies(project, errors)
    check_folders(project, errors)
    check_references(project, guids, errors)
    check_gitignore(project, errors, tracked)
    return errors


def main():
    errors = check(PROJECT)
    for e in errors:
        print(f"error: {e}", file=sys.stderr)
    if errors:
        print(f"{len(errors)} Unity project check(s) failed", file=sys.stderr)
        return 1
    print("Unity project checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
