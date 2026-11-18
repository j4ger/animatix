#!/usr/bin/env python3
"""Test Mandarin word mapping without loading the optional voice models."""

from contextlib import redirect_stderr
import io
import json
from pathlib import Path
import tempfile
import unittest

import qwen_chapter_provider as qwen


class WordAlignmentTests(unittest.TestCase):
    def test_groups_authored_terms_and_preserves_display_spelling(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "lexicon.json"
            path.write_text(json.dumps({"卷积": [], "卷积核": [], "它": ["他"]}), encoding="utf-8")
            lexicon = qwen.read_lexicon(path)
        tokens = ["<|zh|>", "他", "用", "卷", "积", "核", "和", " al", "ex", " net", "。"]
        starts = [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]
        notices = io.StringIO()
        with redirect_stderr(notices):
            words = qwen.match_words("它用卷积核和ＡlexNet。", tokens, starts, 1.2, lexicon)
        self.assertEqual([(word["word"], word["start"], word["end"]) for word in words],
                         [("它", 0.1, 0.2), ("用", 0.2, 0.3), ("卷积核", 0.3, 0.6),
                          ("和", 0.6, 0.7), ("ＡlexNet", 0.7, 1.2)])
        self.assertIn("Accepted recognition alias", notices.getvalue())

    def test_rejects_recognition_disagreement(self):
        with self.assertRaisesRegex(ValueError, "could not align '猫'"):
            qwen.match_words("猫。", ["狗"], [0.1], 0.8, {})
        with self.assertRaisesRegex(ValueError, "additional spoken text"):
            qwen.match_words("猫。", ["猫", "猫"], [0.1, 0.4], 0.8, {})
        with self.assertRaisesRegex(ValueError, "could not align '猫'"):
            qwen.match_words("猫。", ["猫", "咪"], [0.1, 0.4], 0.8, {"猫": {"猫", "猫咪"}})

    def test_checks_timing_bounds_and_unresolved_token_splits(self):
        with self.assertRaisesRegex(ValueError, "different counts"):
            qwen.match_words("猫。", ["猫"], [], 0.8, {})
        with self.assertRaisesRegex(ValueError, "invalid timestamp"):
            qwen.match_words("看猫。", ["看", "猫"], [0.4, 0.1], 0.8, {})
        with self.assertRaisesRegex(ValueError, "did not distinguish the boundary"):
            qwen.match_words("看猫。", ["看猫"], [0.1], 0.8, {})

    def test_rate_filters_use_valid_pitch_preserving_factors(self):
        self.assertEqual(qwen.tempo_filters("+0%"), [])
        self.assertEqual(qwen.tempo_filters("-12%"), ["atempo=0.88"])
        self.assertEqual(qwen.tempo_filters("-75%"), ["atempo=0.5", "atempo=0.5"])
        self.assertEqual(qwen.tempo_filters("+300%"), ["atempo=2", "atempo=2"])


if __name__ == "__main__":
    unittest.main()
