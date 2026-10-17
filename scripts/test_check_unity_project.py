#!/usr/bin/env python3
"""Tests for check_unity_project.py. Standard library only.

Each test copies the real apps/unity/ project into a temporary directory,
breaks one thing, and checks that exactly that problem is reported.

Run from the repository root:
    python3 -m unittest discover -s scripts -p 'test_*.py'
"""

import json
import shutil
import tempfile
import unittest
from pathlib import Path

import check_unity_project as cup


class CheckUnityProjectTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.project = Path(self._tmp.name) / "unity"
        shutil.copytree(cup.PROJECT, self.project, ignore=shutil.ignore_patterns(*cup.GENERATED_FOLDERS))

    def tearDown(self):
        self._tmp.cleanup()

    def errors(self, tracked=()):
        return cup.check(self.project, tracked=list(tracked))

    def assertReports(self, fragment, tracked=()):
        errors = self.errors(tracked)
        self.assertTrue(any(fragment in e for e in errors), f"{fragment!r} not reported in {errors}")

    def path(self, rel):
        return self.project / rel

    def edit_json(self, rel, change):
        p = self.path(rel)
        data = json.loads(p.read_text())
        change(data)
        p.write_text(json.dumps(data))

    def test_repository_project_passes(self):
        self.assertEqual(cup.check(cup.PROJECT), [])

    def test_copied_project_passes(self):
        self.assertEqual(self.errors(), [])

    def test_editor_must_be_unity_6(self):
        self.path("ProjectSettings/ProjectVersion.txt").write_text("m_EditorVersion: 2022.3.50f1\n")
        self.assertReports("does not name a Unity 6 editor")

    def test_readme_must_record_editor_version(self):
        self.path("ProjectSettings/ProjectVersion.txt").write_text("m_EditorVersion: 6000.3.99f1\n")
        self.assertReports("does not record the editor version 6000.3.99f1")

    def edit_player_settings(self, change):
        path = self.path("ProjectSettings/ProjectSettings.asset")
        path.write_text(change(path.read_text()))

    def test_ios_non_metal_graphics_api_fails(self):
        self.edit_player_settings(lambda s: s.replace("m_APIs: 10000000", "m_APIs: 0b000000"))
        self.assertReports("iOSSupport graphics APIs must be exactly Metal")

    def test_ios_mixed_graphics_apis_fail(self):
        self.edit_player_settings(lambda s: s.replace("m_APIs: 10000000", "m_APIs: 100000000b000000"))
        self.assertReports("iOSSupport graphics APIs must be exactly Metal")

    def test_ios_empty_graphics_apis_fail(self):
        self.edit_player_settings(lambda s: s.replace("m_APIs: 10000000", "m_APIs:"))
        self.assertReports("iOSSupport graphics APIs must be exactly Metal")

    def test_ios_automatic_selection_flag_is_not_the_contract(self):
        for automatic in (0, 1):
            with self.subTest(automatic=automatic):
                self.edit_player_settings(
                    lambda s: s.replace("m_Automatic: 0", "m_Automatic: 1")
                    if automatic else s.replace("m_Automatic: 1", "m_Automatic: 0")
                )
                self.assertEqual(self.errors(), [])

    def test_other_platform_metal_does_not_replace_ios_entry(self):
        self.edit_player_settings(lambda s: s.replace("m_BuildTarget: iOSSupport", "m_BuildTarget: Android"))
        self.assertReports("exactly one iOSSupport entry")

    def insert_graphics_api_entry(self, platform, apis):
        header = "  m_BuildTargetGraphicsAPIs:\n"
        entry = f"  - m_BuildTarget: {platform}\n    m_APIs: {apis}\n    m_Automatic: 0\n"
        self.edit_player_settings(lambda s: s.replace(header, header + entry, 1))

    def test_other_platform_graphics_apis_do_not_change_ios_requirement(self):
        settings = self.path("ProjectSettings/ProjectSettings.asset")
        original = settings.read_text()
        for trailing in ("", "  m_BuildTargetGraphicsJobs: []\n  unrelatedSetting: 0\n"):
            with self.subTest(trailing_fields=bool(trailing)):
                settings.write_text(original + trailing)
                self.insert_graphics_api_entry("Android", "0b000000")
                self.assertEqual(self.errors(), [])
                self.edit_player_settings(lambda s: s.replace("m_APIs: 10000000", "m_APIs: 0b000000"))
                self.assertReports("iOSSupport graphics APIs must be exactly Metal")

    def test_ios_graphics_entry_cannot_come_from_another_section(self):
        self.edit_player_settings(lambda s: s.replace("m_BuildTargetGraphicsAPIs:", "unrelatedList:"))
        self.assertReports("one m_BuildTargetGraphicsAPIs list")

    def test_duplicate_ios_graphics_entries_fail(self):
        settings = self.path("ProjectSettings/ProjectSettings.asset")
        original = settings.read_text()
        for trailing in ("", "  m_BuildTargetGraphicsJobs: []\n  unrelatedSetting: 0\n"):
            with self.subTest(trailing_fields=bool(trailing)):
                settings.write_text(original + trailing)
                self.insert_graphics_api_entry("iOSSupport", "10000000")
                self.assertReports("exactly one iOSSupport entry")

    def test_duplicate_ios_api_fields_fail(self):
        self.edit_player_settings(lambda s: s.replace("    m_APIs: 10000000", "    m_APIs: 10000000\n    m_APIs: 0b000000"))
        self.assertReports("iOSSupport graphics APIs must be exactly Metal")

    def test_player_settings_file_is_required(self):
        self.path("ProjectSettings/ProjectSettings.asset").unlink()
        self.assertReports("ProjectSettings/ProjectSettings.asset is missing")

    def test_third_party_package_fails(self):
        self.edit_json("Packages/manifest.json", lambda m: m["dependencies"].update({"com.cesium.unity": "1.0.0"}))
        self.assertReports("com.cesium.unity is not a Unity package")

    def test_git_package_source_fails(self):
        url = "https://github.com/example/pkg.git"
        self.edit_json("Packages/manifest.json", lambda m: m["dependencies"].update({"com.unity.example": url}))
        self.assertReports("non-registry source")

    def test_scoped_registry_fails(self):
        reg = [{"name": "x", "url": "https://example.invalid", "scopes": ["com.x"]}]
        self.edit_json("Packages/manifest.json", lambda m: m.update({"scopedRegistries": reg}))
        self.assertReports("scoped registries")

    def test_missing_urp_fails(self):
        self.edit_json("Packages/manifest.json", lambda m: m["dependencies"].pop("com.unity.render-pipelines.universal"))
        self.assertReports("missing com.unity.render-pipelines.universal")

    def test_asset_without_meta_fails(self):
        self.path("Assets/WorldSim/UI/Notes.txt").write_text("x")
        self.assertReports("Assets/WorldSim/UI/Notes.txt has no .meta file")

    def test_folder_without_meta_fails(self):
        self.path("Assets/WorldSim/UI/Views").mkdir()
        self.assertReports("Assets/WorldSim/UI/Views has no .meta file")

    def test_orphan_meta_fails(self):
        self.path("Assets/WorldSim/UI/README.md").unlink()
        self.assertReports("Assets/WorldSim/UI/README.md.meta has no asset")

    def test_duplicate_guid_fails(self):
        src = self.path("Assets/WorldSim/UI/README.md.meta").read_text()
        self.path("Assets/WorldSim/UI/Copy.md").write_text("x")
        self.path("Assets/WorldSim/UI/Copy.md.meta").write_text(src)
        self.assertReports("reuses guid")

    def test_binary_serialized_asset_fails(self):
        p = self.path("Assets/WorldSim/Rendering/Pipeline/UrpMobile.asset")
        p.write_bytes(b"\x00\x01binary")
        self.assertReports("UrpMobile.asset is not text-serialized YAML")

    def test_bridge_must_not_reference_rendering(self):
        self.edit_json(
            "Assets/WorldSim/Bridge/WorldSim.Bridge.asmdef",
            lambda d: d.update({"references": ["WorldSim.Rendering"]}),
        )
        self.assertReports("WorldSim.Bridge must not reference")

    def test_rendering_must_reference_bridge(self):
        self.edit_json(
            "Assets/WorldSim/Rendering/WorldSim.Rendering.asmdef",
            lambda d: d.update({"references": []}),
        )
        self.assertReports("WorldSim.Rendering must reference exactly WorldSim.Bridge")

    def test_test_assembly_must_be_editor_only(self):
        self.edit_json(
            "Assets/WorldSim/Tests/EditMode/WorldSim.Tests.EditMode.asmdef",
            lambda d: d.update({"includePlatforms": []}),
        )
        self.assertReports("must be an Editor-only assembly")

    def test_unplanned_assembly_fails(self):
        self.path("Assets/WorldSim/UI/WorldSim.UI.asmdef").write_text(json.dumps({"name": "WorldSim.UI"}))
        self.path("Assets/WorldSim/UI/WorldSim.UI.asmdef.meta").write_text(
            "fileFormatVersion: 2\nguid: 0123456789abcdef0123456789abcdef\n"
        )
        self.assertReports("is not in the agreed assembly layout")

    def test_code_in_providers_fails(self):
        self.path("Assets/WorldSim/Providers/GoogleTiles.cs").write_text("class GoogleTiles {}")
        self.path("Assets/WorldSim/Providers/GoogleTiles.cs.meta").write_text(
            "fileFormatVersion: 2\nguid: 0123456789abcdef0123456789abcdef\n"
        )
        self.assertReports("Providers/ holds no code until TP-012")

    def test_build_scene_guid_must_match(self):
        p = self.path("ProjectSettings/EditorBuildSettings.asset")
        p.write_text(p.read_text().replace("af6d213af4f647caab09496b4b728579", "0" * 32))
        self.assertReports("EditorBuildSettings.asset scene Assets/WorldSim/Scenes/Bootstrap.unity")

    def test_render_pipeline_must_resolve(self):
        p = self.path("ProjectSettings/GraphicsSettings.asset")
        p.write_text(p.read_text().replace("65810247bb1042c0864284b1c042bf50", "0" * 32))
        self.assertReports("default render pipeline is not an asset in this project")

    def test_gitignore_must_cover_library(self):
        p = self.path(".gitignore")
        p.write_text("\n".join(l for l in p.read_text().splitlines() if "ibrary" not in l))
        self.assertReports("does not ignore Library/")

    def test_tracked_generated_file_fails(self):
        self.assertReports("Library/ArtifactDB is generated by Unity", tracked=["Library/ArtifactDB"])


if __name__ == "__main__":
    unittest.main()
