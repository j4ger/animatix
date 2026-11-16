#!/usr/bin/env python3
"""Import closure must follow authored paths, never a legacy bundled library."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('imports', Path(__file__).with_name('check-scene-imports.py'))
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)


class Imports(unittest.TestCase):
    def test_real_paths_and_missing_or_outside_targets(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / 'repo'
            (root / 'scenes').mkdir(parents=True)
            (root / 'lib').mkdir()
            (root / 'lib/theme.amx').write_text('pub let ink = "white"\n')
            (root / 'scenes/main.amx').write_text('import "../lib/theme.amx"\n')
            self.assertEqual(list(check.missing_imports(root, ['scenes/main.amx'])), [])
            (root / 'scenes/main.amx').write_text('import "../lib/missing.amx"\n')
            self.assertEqual(len(list(check.missing_imports(root, ['scenes/main.amx']))), 1)
            (root.parent / 'external.amx').write_text('')
            (root / 'scenes/main.amx').write_text('import "../../external.amx"\n')
            self.assertEqual(len(list(check.missing_imports(root, ['scenes/main.amx']))), 1)

    def test_deletions_and_commented_imports(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'main.amx').write_text('// import "missing.amx"\n')
            self.assertEqual(list(check.missing_imports(root, ['main.amx', 'deleted.amx'])), [])


if __name__ == '__main__':
    unittest.main()
