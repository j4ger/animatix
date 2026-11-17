#!/usr/bin/env python3
"""Frozen inputs: migrated-source identity and complete relocation, without a renderer."""
import hashlib
import json
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / 'crates/animatix/examples/web_present_driver/fixtures'


class ExtractedContracts(unittest.TestCase):
    def assert_historical_provenance(self, identity):
        """Check retained provenance metadata without requiring old Git objects."""
        self.assertRegex(identity['origin_commit'], r'^[0-9a-f]{40}$')
        self.assertRegex(identity['origin_source_sha256'], r'^[0-9a-f]{64}$')

    def test_attention_identity_and_import_closure(self):
        identity = json.loads((FIXTURES / 'attention-identity.json').read_text())
        self.assert_historical_provenance(identity)
        source = (FIXTURES / 'attention.amx').read_bytes()
        self.assertEqual(hashlib.sha256(source).hexdigest(), identity['fixture_source_sha256'])

        current_origin_path = ROOT / identity['origin_source']
        current_origin = current_origin_path.read_bytes()
        self.assertEqual(
            hashlib.sha256(current_origin).hexdigest(),
            identity['migrated_origin_source_sha256'],
        )
        relocated = current_origin.replace(
            b'import "../../../../examples/lib/', b'import "lib/'
        )
        self.assertEqual(source, relocated)

        imports = re.findall(rb'^import "([^"]+)"', source, re.M)
        self.assertEqual(set(p.decode() for p in imports), set(m['file'] for m in identity['modules']))
        for module in identity['modules']:
            body = (FIXTURES / module['file']).read_bytes()
            self.assertEqual(
                hashlib.sha256(body).hexdigest(), module['migrated_sha256']
            )
            self.assertEqual(body, (ROOT / module['source']).read_bytes())
            self.assertRegex(module['sha256'], r'^[0-9a-f]{64}$')
            self.assertFalse(re.search(rb'^import ', body, re.M))

    def test_formatter_frozen_source(self):
        source = ROOT / 'crates/animatix-syntax/tests/fixtures/sorting_format.amx.txt'
        identity = json.loads((source.parent / 'sorting_format.json').read_text())
        self.assertRegex(identity['origin_commit'], r'^[0-9a-f]{40}$')
        self.assertRegex(identity['origin_sha256'], r'^[0-9a-f]{64}$')
        self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(), identity['sha256'])
        current_origin = (ROOT / identity['origin_source']).read_bytes()
        self.assertEqual(source.read_bytes(), current_origin)
        self.assertEqual(
            hashlib.sha256(current_origin).hexdigest(),
            identity['migrated_source_sha256'],
        )
    def test_marker_fixture_retains_all_original_times(self):
        source = (ROOT / 'crates/animatix-web/tests/fixtures/timeline_markers.amx').read_text()
        self.assertEqual(re.findall(r'^#([\d.]+)s', source, re.M),
                         ['0.10', '1.10', '2.10', '3.00', '4.40', '4.55', '5.20', '6.60', '7.40'])
        self.assertNotIn('import ', source)


if __name__ == '__main__':
    unittest.main()
