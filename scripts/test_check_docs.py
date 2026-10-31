import tempfile
import unittest
from pathlib import Path

import check_docs


def tree(files):
    root = Path(tempfile.mkdtemp())
    for rel, text in files.items():
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
    return root


BASE = {
    "core/crates/ws-ffi/src/lib.rs": "pub const ABI_VERSION: u32 = 9;\n",
    "core/crates/ws-persist/src/header.rs": "pub const FORMAT_VERSION: u32 = 5;\n",
    "core/crates/ws-ffi/README.md": "Current ABI: **9** here.\n## Transport (introduced in ABI7)\n",
    "apps/unity/README.md": "Current ABI: **9**.\n## Real block (current ABI9)\n",
    "core/crates/ws-persist/README.md": "Current save format: **5**.\n",
}


class CheckVersions(unittest.TestCase):
    def errors(self, **changes):
        files = dict(BASE)
        files.update({k.replace("__", "/"): v for k, v in changes.items()})
        errors = []
        check_docs.check_versions(errors, tree(files))
        return errors

    def test_consistent_tree_passes(self):
        self.assertEqual(self.errors(), [])

    def test_stale_current_abi_claim_fails(self):
        errors = self.errors(**{"apps__unity__README.md": "Current ABI: **9**.\n## Real block (current ABI7)\n"})
        self.assertEqual(errors, ["apps/unity/README.md:2: says ABI 7, code has 9"])

    def test_stale_current_abi_is_claim_fails(self):
        errors = self.errors(**{"core__crates__ws-ffi__README.md": "Current ABI: **9** here.\nCurrent ABI is6. The current ABI is9.\n"})
        self.assertEqual(errors, ["core/crates/ws-ffi/README.md:2: says ABI 6, code has 9"])

    def test_stale_writer_format_claim_fails(self):
        errors = self.errors(**{"core__crates__ws-persist__README.md": "Current save format: **5**.\nWriters always write format 3.\n"})
        self.assertEqual(errors, ["core/crates/ws-persist/README.md:2: says format 3, code has 5"])

    def test_missing_marker_fails_after_a_bump(self):
        errors = self.errors(**{"core__crates__ws-persist__src__header.rs": "pub const FORMAT_VERSION: u32 = 6;\n"})
        self.assertIn("core/crates/ws-persist/README.md:1: says format 5, code has 6", errors)
        self.assertIn("core/crates/ws-persist/README.md: missing 'Current save format: **6**'", errors)


if __name__ == "__main__":
    unittest.main()
