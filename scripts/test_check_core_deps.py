#!/usr/bin/env python3
"""Tests for check_core_deps.py. Standard library only.

Run from the repository root:
    python3 -m unittest discover -s scripts -p 'test_*.py'
"""

import unittest

import check_core_deps


def dep(name, kind=None):
    return {"name": name, "kind": kind}


def metadata(*packages):
    """Builds minimal `cargo metadata` JSON from (name, [deps]) pairs."""
    pkgs = [{"id": f"{name}-id", "name": name, "dependencies": deps} for name, deps in packages]
    return {"workspace_members": [p["id"] for p in pkgs], "packages": pkgs}


class CheckCoreDepsTest(unittest.TestCase):
    def test_current_shape_passes(self):
        self.assertEqual(check_core_deps.check(metadata(("ws-sim", []))), [])

    def test_downward_dependencies_pass(self):
        md = metadata(
            ("ws-math", []),
            ("ws-world", [dep("ws-math")]),
            ("ws-agents", [dep("ws-world"), dep("ws-math")]),
            ("ws-sim", [dep("ws-agents"), dep("ws-world")]),
            ("ws-ffi", [dep("ws-sim")]),
        )
        self.assertEqual(check_core_deps.check(md), [])

    def test_upward_dependency_fails(self):
        md = metadata(("ws-math", [dep("ws-sim")]), ("ws-sim", []))
        errors = check_core_deps.check(md)
        self.assertEqual(len(errors), 1)
        self.assertIn("ws-math (layer 0) -> ws-sim (layer 3)", errors[0])

    def test_sim_depending_on_ffi_fails(self):
        md = metadata(("ws-sim", [dep("ws-ffi")]), ("ws-ffi", []))
        self.assertEqual(len(check_core_deps.check(md)), 1)

    def test_persist_sits_above_sim(self):
        md = metadata(("ws-persist", [dep("ws-sim")]), ("ws-sim", []))
        self.assertEqual(check_core_deps.check(md), [])
        md = metadata(("ws-sim", [dep("ws-persist")]), ("ws-persist", []))
        errors = check_core_deps.check(md)
        self.assertEqual(len(errors), 1)
        self.assertIn("ws-sim (layer 3) -> ws-persist (layer 4)", errors[0])

    def test_same_layer_dependency_fails(self):
        md = metadata(("ws-agents", [dep("ws-lod")]), ("ws-lod", []))
        errors = check_core_deps.check(md)
        self.assertEqual(len(errors), 1)
        self.assertIn("ws-agents (layer 2) -> ws-lod (layer 2)", errors[0])

    def test_build_dependency_is_checked_like_normal(self):
        md = metadata(("ws-world", [dep("ws-sim", "build")]), ("ws-sim", []))
        self.assertEqual(len(check_core_deps.check(md)), 1)

    def test_upward_dev_dependency_passes(self):
        md = metadata(("ws-math", [dep("ws-sim", "dev")]), ("ws-sim", []))
        self.assertEqual(check_core_deps.check(md), [])

    def test_testkit_as_dev_dependency_passes(self):
        md = metadata(("ws-sim", [dep("ws-testkit", "dev")]), ("ws-testkit", [dep("ws-sim")]))
        self.assertEqual(check_core_deps.check(md), [])

    def test_testkit_as_normal_dependency_fails(self):
        md = metadata(("ws-sim", [dep("ws-testkit")]), ("ws-testkit", []))
        errors = check_core_deps.check(md)
        self.assertEqual(len(errors), 1)
        self.assertIn("may only be a dev-dependency", errors[0])

    def test_unknown_workspace_crate_fails(self):
        errors = check_core_deps.check(metadata(("ws-mystery", [])))
        self.assertEqual(len(errors), 1)
        self.assertIn("not in the LAYERS table", errors[0])

    def test_forbidden_external_crates_fail(self):
        for name in ["wgpu", "bevy_ecs", "winit", "objc2", "cesium-native", "web-sys", "windows-sys"]:
            with self.subTest(name=name):
                md = metadata(("ws-sim", [dep(name)]))
                errors = check_core_deps.check(md)
                self.assertEqual(len(errors), 1)
                self.assertIn("forbidden external crate", errors[0])

    def test_forbidden_external_dev_dependency_fails(self):
        self.assertEqual(len(check_core_deps.check(metadata(("ws-sim", [dep("wgpu", "dev")])))), 1)

    def test_allowed_external_crate_passes(self):
        md = metadata(("ws-math", [dep("proptest", "dev")]), ("ws-sim", [dep("serde")]))
        self.assertEqual(check_core_deps.check(md), [])

    def test_similar_but_allowed_names_pass(self):
        # Anchored patterns: "glam" is not "gl", "ashpd" is not "ash".
        md = metadata(("ws-math", [dep("glam"), dep("ashpd")]))
        self.assertEqual(check_core_deps.check(md), [])


if __name__ == "__main__":
    unittest.main()
