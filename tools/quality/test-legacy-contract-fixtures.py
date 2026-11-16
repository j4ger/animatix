#!/usr/bin/env python3
"""Frozen inputs: identity and complete source relocation, without a renderer."""
import hashlib
import json
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / 'crates/animatix/examples/web_present_driver/fixtures'


class ExtractedContracts(unittest.TestCase):
    def test_attention_identity_and_import_closure(self):
        identity = json.loads((FIXTURES / 'attention-identity.json').read_text())
        source = (FIXTURES / 'attention.amx').read_bytes()
        self.assertEqual(hashlib.sha256(source).hexdigest(), identity['fixture_source_sha256'])
        restored = source.replace(b'import "lib/', b'import "../../../../examples/lib/')
        self.assertEqual(hashlib.sha256(restored).hexdigest(), identity['origin_source_sha256'])
        imports = re.findall(rb'^import "([^"]+)"', source, re.M)
        self.assertEqual(set(p.decode() for p in imports), set(m['file'] for m in identity['modules']))
        for module in identity['modules']:
            body = (FIXTURES / module['file']).read_bytes()
            self.assertEqual(hashlib.sha256(body).hexdigest(), module['sha256'])
            self.assertFalse(re.search(rb'^import ', body, re.M))

    def test_formatter_frozen_source(self):
        source = ROOT / 'crates/animatix-syntax/tests/fixtures/sorting_format.amx.txt'
        identity = json.loads((source.parent / 'sorting_format.json').read_text())
        self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(), identity['sha256'])

    def test_marker_fixture_retains_all_original_times(self):
        source = (ROOT / 'crates/animatix-web/tests/fixtures/timeline_markers.amx').read_text()
        self.assertEqual(re.findall(r'^#([\d.]+)s', source, re.M),
                         ['0.10', '1.10', '2.10', '3.00', '4.40', '4.55', '5.20', '6.60', '7.40'])
        self.assertNotIn('import ', source)


if __name__ == '__main__':
    unittest.main()
