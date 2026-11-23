#!/usr/bin/env python3
"""Offline checks for speech synthesis and transcript conversion."""

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


class ConversionTests(unittest.TestCase):
    def convert(self, content, suffix):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / f"input{suffix}"
            output = Path(directory) / "output.speech.json"
            source.write_bytes(content.encode("utf-8"))
            speech_sync.convert_transcript(str(source), str(output))
            return json.loads(output.read_text(encoding="utf-8"))

    def test_srt_preserves_cue_times_and_multiline_text(self):
        transcript = self.convert(
            "\ufeff1\r\n00:00:00,000 --> 00:00:01,234\r\nHello\r\nworld!\r\n"
            "\r\n \r\n2\r\n01:02:03,004 --> 01:02:04,567\r\nEncore\r\n",
            ".SRT",
        )

        self.assertEqual(transcript["duration"], 3724.567)
        self.assertEqual(transcript["markers"], {})
        first, second = transcript["segments"]
        self.assertEqual(
            (first["id"], first["start"], first["end"], first["text"]),
            (0, 0.0, 1.234, "Hello\nworld!"),
        )
        self.assertEqual(first["words"], [])
        self.assertEqual(
            (second["id"], second["start"], second["end"], second["text"]),
            (1, 3723.004, 3724.567, "Encore"),
        )

    def test_invalid_srt_reports_the_cue_and_timestamp(self):
        with self.assertRaisesRegex(SystemExit, r"invalid SRT .*cue 1: invalid timestamp"):
            self.convert("1\n00:00:01,234 --> invalid\nHello\n", ".srt")

    def test_json_preserves_zero_and_defaults_only_missing_numbers(self):
        transcript = self.convert(json.dumps({
            "duration": 0.0,
            "markers": {"intro": 0.0},
            "segments": [
                {"start": 0.0, "start_s": 1.0, "end": 0.0, "end_s": 1.5},
                {"start_s": 1.5, "end_s": 2.0, "text": "  Hello  ", "words": [
                    {"word": "Hello", "start": 0.0, "start_s": 1.5,
                     "end": 0.0, "end_s": 1.6, "confidence": 0.0, "probability": 0.9},
                    {"text": "again", "start": None, "start_s": 0.0, "end_s": 0.0,
                     "confidence": None, "probability": 0.0},
                    {"word": "!", "start": 1.75, "end": 0.0},
                    {"word": "default"},
                ]},
            ],
        }), ".json")

        self.assertEqual(transcript["duration"], 0.0)
        self.assertEqual(transcript["markers"], {"intro": 0.0})
        first, second = transcript["segments"]
        self.assertEqual((first["start"], first["end"]), (0.0, 0.0))
        self.assertEqual((second["start"], second["end"], second["text"]), (1.5, 2.0, "Hello"))
        self.assertEqual(second["words"], [
            {"word": "Hello", "start": 0.0, "end": 0.0, "confidence": 0.0},
            {"word": "again", "start": 0.0, "end": 0.0, "confidence": 0.0},
            {"word": "!", "start": 1.75, "end": 0.0, "confidence": 1.0},
            {"word": "default", "start": 1.5, "end": 1.5, "confidence": 1.0},
        ])


if __name__ == "__main__":
    unittest.main()
