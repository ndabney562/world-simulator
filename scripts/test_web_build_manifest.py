import tempfile
import unittest
from pathlib import Path
from web_build_manifest import manifest


class WebManifestTests(unittest.TestCase):
    def test_full_payload_includes_data_and_notices_and_is_repeatable(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            for path, content in [('index.html', b'html'), ('game.wasm.unityweb', b'wasm'),
                                  ('game.data.unityweb', b'data'), ('THIRD_PARTY_NOTICES.txt', b'credits')]:
                (root / path).write_bytes(content)
            before = manifest(root)
            (root / 'build-manifest.json').write_text('old manifest')
            self.assertEqual(manifest(root), before)
            self.assertEqual(before['total_payload_bytes'], 19)
            self.assertEqual(len(before['files']), 4)

    def test_core_module_alone_is_not_a_player(self):
        with tempfile.TemporaryDirectory() as name:
            (Path(name) / 'core.wasm').write_bytes(b'wasm')
            with self.assertRaisesRegex(ValueError, 'index.html'):
                manifest(name)
