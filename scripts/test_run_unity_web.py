import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from run_unity_web import run


class UnityBuildIsolationTests(unittest.TestCase):
    def test_failed_editor_restores_exact_preexisting_settings_and_assets(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            settings = root / 'ProjectSettings/ProjectSettings.asset'
            pipeline = root / 'Assets/Pipeline.asset'
            package = root / 'Packages/packages-lock.json'
            for path in (settings, pipeline, package):
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b'preexisting user content\r\n')
            def editor(args):
                for path in (settings, pipeline, package):
                    path.write_bytes(b'editor build-only edits')
                return 1
            with patch('run_unity_web.subprocess.call', side_effect=editor):
                self.assertEqual(run('Unity', root, root / 'log'), 1)
            for path in (settings, pipeline, package):
                self.assertEqual(path.read_bytes(), b'preexisting user content\r\n')
