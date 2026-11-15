#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('promotion', Path(__file__).with_name('cloudflare-promotion.py'))
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)


class PromotionTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.path = Path(self.tmp.name)/'state.sqlite'
        self.c = p.Coordinator(self.path)
        self.addCleanup(self.tmp.cleanup)
        self.addCleanup(self.c.close)

    def test_late_old_build_and_stale_selection(self):
        self.c.select(0, 'a'*64)
        self.c.select(1, 'b'*64)
        with self.assertRaises(p.Rejected):
            self.c.begin(1, 'a'*64)
        with self.assertRaises(p.Rejected):
            self.c.select(1, 'a'*64)
        ticket = self.c.begin(2, 'b'*64)
        self.c.confirm(ticket, 'deployment-b')
        with self.assertRaises(p.Rejected):
            self.c.begin(2, 'b'*64)

    def test_pending_survives_crash_blocks_new_and_duplicate_requests(self):
        self.c.select(0, 'a'*64)
        ticket = self.c.begin(1, 'a'*64)
        other = p.Coordinator(self.path)
        self.addCleanup(other.close)
        with self.assertRaises(p.Rejected):
            other.select(1, 'b'*64)
        with self.assertRaises(p.Rejected):
            other.begin(1, 'a'*64)
        with self.assertRaises(p.Rejected):
            other.confirm('wrong', 'deployment-a')
        other.confirm(ticket, 'deployment-a')
        self.assertEqual(other.select(1, 'b'*64), 2)

    def test_rollback_is_new_selection_and_fences_old_completions(self):
        for epoch, candidate in [(1, 'a'*64), (2, 'b'*64), (3, 'a'*64)]:
            self.c.select(epoch-1, candidate)
            self.c.confirm(self.c.begin(epoch, candidate), f'deployment-{epoch}')
        with self.assertRaises(p.Rejected):
            self.c.begin(2, 'b'*64)


if __name__ == '__main__':
    unittest.main()
