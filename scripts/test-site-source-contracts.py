#!/usr/bin/env python3
"""Source selection and development assets survive the old HTML retirement."""
import importlib.util
from pathlib import Path
import re
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('rest', Path(__file__).with_name('site-rest-check.py'))
rest = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rest)


class Sources(unittest.TestCase):
    def test_resting_check_uses_canonical_sources_not_legacy_html(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'content/scenes').mkdir(parents=True)
            source = root / 'content/scenes/current.amx'
            source.write_text('config { duration: 1 }')
            (root / 'legacy.amx').write_text('')
            (root / 'index.html').write_text('<amx-player src="legacy.amx">')
            previous = rest.ROOT
            try:
                rest.ROOT = root
                self.assertEqual(rest.players(), [source])
                source.unlink()
                self.assertEqual(rest.players(), [])
            finally:
                rest.ROOT = previous

    def test_developer_arena_has_local_styles_and_valid_logo(self):
        page = ROOT / 'web/dev/arena/index.html'
        html = page.read_text()
        for href in re.findall(r'<link[^>]+href="([^"]+)"', html):
            self.assertTrue((page.parent / href).is_file(), href)
        self.assertNotIn('site-chrome.js', html)
        self.assertIn('../../runtime/amx-player.js', html)
        self.assertIn('/dev/arena/', (ROOT / 'scripts/arena.py').read_text())
        css = page.with_name('base.css')
        for target in re.findall(r'@import url\("([^"]+)"\)', css.read_text()):
            self.assertTrue((css.parent / target).is_file(), target)


if __name__ == '__main__':
    unittest.main()
