#!/usr/bin/env python3
"""Offline chapter workflow checks with real WAVs from a deterministic local provider."""

import json
import math
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest

import speech_chapter as chapter


TOOL = Path(chapter.__file__).resolve()
FIXTURE = Path(__file__).resolve()


def fixture_provider():
    request = json.load(sys.stdin)
    log = Path("requests.jsonl")
    with log.open("a", encoding="utf-8") as stream:
        stream.write(json.dumps(request) + "\n")
    variant = len(log.read_text(encoding="utf-8").splitlines())
    samples = chapter.SAMPLE_RATE if request["rate"] == "-20%" else chapter.SAMPLE_RATE // 2
    pcm = struct.pack(f"<{samples}h", *(int(10_000 * math.sin(i * 0.01 * variant)) for i in range(samples)))
    chapter.write_wav(Path(request["audio"]), [(pcm, 0)])
    tokens = request["text"].split()
    step = samples / chapter.SAMPLE_RATE / (len(tokens) + 1)
    words = [{"word": token, "start": index * step, "end": (index + 0.5) * step}
             for index, token in enumerate(tokens, start=1)]
    if request["voice"] == "bad-timing":
        words[0]["end"] = 5
    Path(request["words"]).write_text(json.dumps(words), encoding="utf-8")


class ChapterTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.manifest = self.root / "source.json"
        self.audio = self.root / "chapter.wav"
        self.transcript = self.root / "chapter.speech.json"
        self.receipt = self.root / "chapter.takes" / "receipt.json"
        self.source = {"voice": "sample-voice", "sentences": [
            {"id": "lead", "text": "Alpha repeats repeats.", "pause_after": 0.1,
             "markers": {"intro": "Alpha", "repeat_twice": {"word": "repeats", "occurrence": 2}}},
            {"id": "end", "text": "Omega stays.", "markers": {"outro": "Omega"}},
        ]}
        self.save_chapter()

    def save_chapter(self):
        self.manifest.write_bytes(chapter.json_bytes(self.source))

    def run_tool(self, action, *args, output="chapter", ok=True):
        result = subprocess.run([
            sys.executable, str(TOOL), action, str(self.manifest), "--output", str(self.root / output),
            "--provider-command", sys.executable, "--provider-arg", str(FIXTURE), "--provider-arg=--fixture",
            *args,
        ], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0 if ok else 1, result.stdout + result.stderr)
        return result.stdout if ok else result.stderr

    def requests(self):
        return [json.loads(line) for line in (self.root / "requests.jsonl").read_text().splitlines()]

    def test_build_and_retake_reuse_other_takes_and_rebase_anchors(self):
        self.assertIn("2 synthesized, 0 reused", self.run_tool("build"))
        before = chapter.read_json(self.transcript)
        self.assertAlmostEqual(before["markers"]["repeat_twice"], 0.375)
        self.assertNotIn("confidence", before["segments"][0]["words"][0])
        baseline = chapter.read_pcm(self.audio)
        self.assertEqual(len(baseline) // 2, round(1.1 * chapter.SAMPLE_RATE))
        self.assertEqual(before["duration"], 1.1)
        self.assertIn("0 synthesized, 2 reused", self.run_tool("build"))
        self.assertEqual(self.audio.stat().st_size, len(baseline) + 44)
        self.assertEqual(len(self.requests()), 2)

        self.source["sentences"][0]["rate"] = "-20%"
        self.save_chapter()
        result = self.run_tool("retake", "lead")
        self.assertIn("1 synthesized, 1 reused", result)
        self.assertEqual([(request["text"], request["rate"]) for request in self.requests()], [
            ("Alpha repeats repeats.", "+0%"), ("Omega stays.", "+0%"), ("Alpha repeats repeats.", "-20%"),
        ])
        self.assertEqual(chapter.read_pcm(self.audio)[-chapter.SAMPLE_RATE:], baseline[-chapter.SAMPLE_RATE:])
        after = chapter.read_json(self.transcript)
        self.assertEqual(after["duration"], 1.6)
        self.assertAlmostEqual(after["segments"][1]["start"] - before["segments"][1]["start"], 0.5)
        report = json.loads(result.split("\n", 1)[1])
        self.assertIn({"marker": "outro", "before": before["markers"]["outro"],
                       "after": after["markers"]["outro"], "delta": 0.5}, report["affected_markers"])

    def test_audition_preserves_chapter_and_adopts_exact_selected_audio(self):
        self.run_tool("build")
        baseline = [path.read_bytes() for path in (self.audio, self.transcript, self.receipt)]
        self.run_tool("audition", "lead", "--candidate", "sample-voice", "--candidate", "another@-20%",
                      output="reads")
        bundle = self.root / "reads"
        candidates = chapter.read_json(bundle / "audition.json")["candidates"]
        self.assertEqual([(take["voice"], take["rate"], take["comparison_start"]) for take in candidates],
                         [("sample-voice", "+0%", 0.0), ("another", "-20%", 1.0)])
        self.assertEqual([request["text"] for request in self.requests()[2:]],
                         [self.source["sentences"][0]["text"]] * 2)
        first, second = [chapter.read_pcm(bundle / take["audio"]) for take in candidates]
        self.assertEqual(chapter.read_pcm(bundle / "comparison.wav"), first + bytes(chapter.SAMPLE_RATE) + second)
        self.assertEqual([path.read_bytes() for path in (self.audio, self.transcript, self.receipt)], baseline)
        adopt = ("--from-audition", str(bundle), "--candidate", "2")
        self.assertIn("does not match", self.run_tool("retake", "lead", *adopt, ok=False))
        self.source["sentences"][0].update({"voice": "another", "rate": "-20%"})
        self.save_chapter()
        self.assertIn("1 audition take applied", self.run_tool("retake", "lead", *adopt))
        self.assertEqual(chapter.read_pcm(self.audio)[:len(second)], second)
        self.assertEqual(len(self.requests()), 4)

    def test_retake_rejects_other_changes_and_invalid_alignment(self):
        self.run_tool("build")
        paths = (self.audio, self.transcript, self.receipt)
        baseline = [path.read_bytes() for path in paths]
        self.source["sentences"][1]["rate"] = "-20%"
        self.save_chapter()
        self.assertIn("cannot reuse sentence 'end'", self.run_tool("retake", "lead", ok=False))
        self.assertEqual(len(self.requests()), 2)

        del self.source["sentences"][1]["rate"]
        self.source["sentences"][0]["voice"] = "bad-timing"
        self.save_chapter()
        self.assertIn("out-of-range word boundary", self.run_tool("retake", "lead", ok=False))
        self.assertEqual([path.read_bytes() for path in paths], baseline)

    def test_build_recovers_corrupted_cache_without_reusing_it(self):
        self.run_tool("build")
        record = chapter.read_json(self.receipt)["takes"]["lead"]
        clip = self.receipt.with_name(record["pcm"] + ".wav")
        pcm = chapter.read_pcm(clip)
        chapter.write_wav(clip, [(bytes(len(pcm)), 0)])
        self.assertIn("1 synthesized, 1 reused", self.run_tool("build"))
        self.assertEqual(self.requests()[-1]["text"], "Alpha repeats repeats.")
        self.assertIn("0 synthesized, 2 reused", self.run_tool("build"))

    def test_build_reuses_validated_sentence_after_later_failure(self):
        self.source["sentences"][1]["voice"] = "bad-timing"
        self.save_chapter()
        self.assertIn("out-of-range word boundary", self.run_tool("build", ok=False))
        self.assertFalse(self.audio.exists())
        self.assertFalse(self.transcript.exists())
        self.assertFalse(self.receipt.exists())

        del self.source["sentences"][1]["voice"]
        self.save_chapter()
        self.assertIn("1 synthesized, 1 reused", self.run_tool("build"))
        self.assertEqual([request["text"] for request in self.requests()],
                         ["Alpha repeats repeats.", "Omega stays.", "Omega stays."])

    def test_concurrent_writer_preserves_published_chapter(self):
        self.run_tool("build")
        baseline = self.audio.read_bytes()
        with chapter.output_lock(self.receipt.parent):
            self.assertIn("could not lock", self.run_tool("retake", "lead", ok=False))
        self.assertEqual(self.audio.read_bytes(), baseline)
        self.assertEqual(len(self.requests()), 2)


if __name__ == "__main__":
    if sys.argv[1:] == ["--fixture"]:
        fixture_provider()
    else:
        unittest.main()
