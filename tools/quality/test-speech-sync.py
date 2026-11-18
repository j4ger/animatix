#!/usr/bin/env python3
"""Offline checks for the Edge TTS word-boundary contract."""

import asyncio
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

import speech_sync


class SynthesisTests(unittest.TestCase):
    def synthesize(self, events):
        async def stream():
            for event in events:
                yield event

        return Mock(Communicate=Mock(return_value=Mock(stream=stream)))

    def test_requests_and_writes_word_boundaries(self):
        edge_tts = self.synthesize([
            {"type": "audio", "data": b"mock mp3"},
            {"type": "WordBoundary", "text": "Hello", "offset": 1_000_000, "duration": 2_000_000},
        ])
        with tempfile.TemporaryDirectory() as directory:
            prefix = Path(directory) / "voice"
            with patch.object(speech_sync, "edge_tts", edge_tts):
                asyncio.run(speech_sync.synthesize_speech("Hello", str(prefix), "voice-id"))
            edge_tts.Communicate.assert_called_once_with("Hello", "voice-id", boundary="WordBoundary")
            self.assertEqual(prefix.with_suffix(".mp3").read_bytes(), b"mock mp3")
            transcript = json.loads(prefix.with_suffix(".speech.json").read_text())
            word = transcript["segments"][0]["words"][0]
            self.assertEqual((word["word"], word["start"], word["end"]), ("Hello", 0.1, 0.3))

    def test_missing_word_boundaries_preserve_existing_files(self):
        edge_tts = self.synthesize([{"type": "audio", "data": b"unexpected audio"}])
        with tempfile.TemporaryDirectory() as directory:
            prefix = Path(directory) / "voice"
            audio = prefix.with_suffix(".mp3")
            transcript = prefix.with_suffix(".speech.json")
            audio.write_bytes(b"old audio")
            transcript.write_text("old transcript")
            with patch.object(speech_sync, "edge_tts", edge_tts):
                with self.assertRaisesRegex(SystemExit, "word boundaries"):
                    asyncio.run(speech_sync.synthesize_speech("Hello", str(prefix)))
            self.assertEqual(audio.read_bytes(), b"old audio")
            self.assertEqual(transcript.read_text(), "old transcript")


if __name__ == "__main__":
    unittest.main()
