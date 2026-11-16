#!/usr/bin/env python3
"""Pages upload constraints use isolated fixtures, never production media."""
import gzip
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('cloudflare_site_check', Path(__file__).with_name('cloudflare-site-check.py'))
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class CloudflareSiteCheckTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / 'site'
        (self.root / 'runtime').mkdir(parents=True)
        for name in ('index.html', '404.html'):
            (self.root / name).write_text('<h1>Fixture</h1>')
        (self.root / 'runtime/runtime-manifest.json').write_text('{}')
        self.manifest({'schema_version': 1, 'base': ''})

    def manifest(self, data):
        (self.root / 'site-manifest.json').write_text(json.dumps(data))

    def errors(self):
        return '\n'.join(CHECK.check_site(self.root))

    def test_valid_static_site(self):
        (self.root / 'runtime/module.wasm.gz').write_bytes(gzip.compress(b'fixture'))
        self.assertEqual(self.errors(), '')

    def test_missing_root_or_required_file(self):
        self.assertTrue(CHECK.check_site(self.root / 'absent'))
        for name in ('404.html', 'runtime/runtime-manifest.json'):
            with self.subTest(name=name):
                path = self.root / name
                old = path.read_bytes()
                path.unlink()
                self.assertIn(name, self.errors())
                path.write_bytes(old)

    def test_empty_required_file(self):
        (self.root / '404.html').write_bytes(b'')
        self.assertIn('404.html', self.errors())

    def test_server_code_rejected(self):
        (self.root / '_worker.js').write_text('export default {}')
        (self.root / 'functions').mkdir()
        self.assertIn('_worker.js', self.errors())
        self.assertIn('functions', self.errors())

    def test_symlinks_rejected_without_traversal(self):
        outside = Path(self.temp.name) / 'outside'
        outside.mkdir()
        (outside / 'module.gz').write_bytes(b'not gzip')
        (self.root / 'linked-dir').symlink_to(outside, target_is_directory=True)
        (self.root / 'broken-link').symlink_to(outside / 'absent')
        errors = self.errors()
        self.assertIn('linked-dir', errors)
        self.assertIn('broken-link', errors)
        self.assertNotIn('module.gz', errors)
        self.assertTrue(CHECK.check_site(self.root / 'linked-dir'))

    def test_single_file_boundary(self):
        path = self.root / 'large.wasm'
        with path.open('wb') as target:
            target.truncate(CHECK.MAX_FILE_BYTES)
        self.assertEqual(self.errors(), '')
        with path.open('ab') as target:
            target.write(b'x')
        self.assertIn('exceeds 25 MiB', self.errors())

    def test_file_count_boundary(self):
        with patch.object(CHECK, 'MAX_FILES', 4):
            self.assertEqual(self.errors(), '')
            (self.root / 'extra.txt').write_text('extra')
            self.assertIn('exceeds 4 files: 5', self.errors())

    def test_manifest_root_base(self):
        self.manifest({'schema_version': 1, 'base': '/animatix'})
        self.assertIn('root base', self.errors())
        self.manifest([])
        self.assertIn('root base', self.errors())
        (self.root / 'site-manifest.json').write_text('{broken')
        self.assertIn('invalid site manifest', self.errors())

    def test_bad_sdk_gzip(self):
        (self.root / 'runtime/module.gz').write_bytes(b'not gzip')
        self.assertIn('gzip signature', self.errors())

    def test_special_file_rejected(self):
        os.mkfifo(self.root / 'pipe')
        self.assertIn('non-regular upload entry', self.errors())


if __name__ == '__main__':
    unittest.main()
