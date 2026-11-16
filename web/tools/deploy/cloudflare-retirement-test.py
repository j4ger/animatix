#!/usr/bin/env python3
"""Keep retired Worker entrypoints out while retaining live Pages contracts."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[3]


class RetirementTests(unittest.TestCase):
    def test_worker_entrypoints_are_removed(self):
        for name in ('wrangler.jsonc', '.github/cloudflare/preview-target.json',
                     'web/deploy/static-worker.js', 'scripts/cloudflare-candidate.py',
                     'scripts/cloudflare-candidate-upload.py',
                     'scripts/cloudflare-release-coordinator.py'):
            with self.subTest(path=name):
                self.assertFalse((ROOT / name).exists())
        gates = (ROOT / 'tools/ci/ci.sh').read_text()
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.assertNotIn('cloudflare-candidate-', gates)
        self.assertNotIn('cloudflare_candidate:', workflow)
        self.assertNotIn('cloudflare_candidate_dispatch:', workflow)

    def test_pages_security_dependencies_remain(self):
        for name in ('cloudflare-pages', 'cloudflare-pages-release', 'cloudflare-permit',
                     'cloudflare-sdk-publisher', 'cloudflare-sdk-consumer',
                     'cloudflare-verified-consumer', 'cloudflare-nix-bootstrap'):
            with self.subTest(module=name):
                self.assertTrue((ROOT / 'web/tools/deploy' / (name + '.py')).is_file())
        self.assertTrue((ROOT / '.github/cloudflare/trusted-root.jsonl').is_file())
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.assertIn('  cloudflare_pages:', workflow)
        self.assertIn('  cloudflare_pages_release:', workflow)


if __name__ == '__main__':
    unittest.main()
