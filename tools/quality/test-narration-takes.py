#!/usr/bin/env python3
"""Exercise the paragraph worker CLI with local audio and a deterministic provider."""

import json
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import narration_takes as worker
import speech_chapter as speech


FIXTURE_PROVIDER = Path(__file__).with_name("test-speech-chapter.py").resolve()


class NarrationTakesTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="narration takes ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.manifest = self.root / "scene script.json"
        self.store = worker.Store(self.root / "take assets" / "scene")
        self.sentences = [
            {"id": "first", "text": "Alpha bravo.", "pause_after": 0.1, "markers": {"alpha": "alpha"}},
            {"id": "second", "text": "Charlie delta."},
        ]
        self.save_manifest()

    def save_manifest(self):
        self.manifest.write_bytes(speech.json_bytes({"voice": "sample", "sentences": self.sentences}))

    def run_tool(self, action, *options, ok=True):
        process = subprocess.run([
            sys.executable, worker.__file__, action, str(self.manifest), "-o", str(self.store.prefix),
            *options,
        ], capture_output=True, text=True)
        self.assertEqual(process.returncode, 0 if ok else 1, process.stdout + process.stderr)
        return json.loads(process.stdout.splitlines()[-1]) if ok else process.stderr

    def synthesize(self, name, *options):
        return self.run_tool("synthesize", name, "--provider-command", sys.executable,
                             f"--provider-arg={FIXTURE_PROVIDER}", "--provider-arg=--fixture", *options)

    def recording(self, name, words, count, sample=21):
        audio = self.root / f"{name}.wav"
        timing = self.root / f"{name}.json"
        speech.write_wav(audio, [(struct.pack("<h", sample) * count, 0)])
        timing.write_bytes(speech.json_bytes({"segments": [{"words": words}]}))
        return ("--audio", str(audio), "--timing", str(timing))

    def test_prepare_partial_chapter_reopen_and_replace_exact_candidate(self):
        initial = self.run_tool("inspect")
        self.assertEqual(initial["revision"], "empty")
        self.assertEqual([p["status"] for p in initial["paragraphs"]], ["missing", "missing"])
        first = self.synthesize("first")
        alternate = self.synthesize("first", "--voice", "other", "--rate=-20%")
        self.assertEqual(first["word_count"], 2)
        self.assertEqual(alternate["duration"], 1)
        self.assertEqual(alternate["rate"], "-20%")
        self.assertFalse(self.store.current.exists())
        partial = self.run_tool("adopt", "first", "--candidate", first["candidate"], "--expected-revision", "empty")
        second = self.run_tool("import", "second", *self.recording("mic", [
            {"word": " Charlie", "start": 0.01, "end": 0.09, "probability": 0.94},
            {"word": " delta", "start": 0.11, "end": 0.18},
        ], 4800))
        full = self.run_tool("adopt", "second", "--candidate", second["candidate"])
        self.assertEqual(second["kind"], "recorded")
        self.assertIsNone(second["voice"])
        self.assertEqual(full["affected_markers"], [])
        before = speech.read_pcm(full["audio"])
        self.assertEqual(before, speech.read_pcm(partial["audio"]) + speech.read_pcm(second["audio"]))
        state = self.run_tool("inspect")
        self.assertEqual([p["active_take"] for p in state["paragraphs"]], [first["candidate"], second["candidate"]])
        self.assertCountEqual([candidate["candidate"] for candidate in state["paragraphs"][0]["candidates"]],
                              [first["candidate"], alternate["candidate"]])

        new = self.run_tool("adopt", "first", "--candidate", alternate["candidate"],
                            "--expected-revision", full["revision"])
        after = speech.read_pcm(new["audio"])
        self.assertEqual(after, speech.read_pcm(alternate["audio"]) + bytes(4800) + speech.read_pcm(second["audio"]))
        self.assertEqual(speech.read_pcm(full["audio"]), before)
        self.assertEqual(speech.read_json(Path(new["transcript"]))["segments"][1]["words"][0]["start"], 1.11)
        self.assertEqual(new["affected_markers"][0]["marker"], "alpha")
        reopened = self.run_tool("inspect", "--base-audio", full["audio"], "--base-transcript", full["transcript"])
        self.assertEqual(reopened["revision"], full["revision"])
        self.assertEqual(reopened["paragraphs"][0]["active_take"], first["candidate"])
        self.assertIn("same revision", self.run_tool("inspect", "--base-audio", full["audio"],
                                                     "--base-transcript", new["transcript"], ok=False))
        branch = self.run_tool("adopt", "first", "--candidate", first["candidate"], "--base-audio", partial["audio"],
                               "--base-transcript", partial["transcript"], "--expected-revision", partial["revision"])
        self.assertEqual(speech.read_pcm(branch["audio"]), speech.read_pcm(partial["audio"]))
        self.assertEqual(self.run_tool("inspect")["revision"], new["revision"])

    def test_explicit_empty_base_treats_authored_source_as_commit_boundary(self):
        first = self.synthesize("first")
        published = self.run_tool("adopt", "first", "--candidate", first["candidate"], "--empty-base")
        self.assertFalse(self.store.current.exists())
        empty = self.run_tool("inspect", "--empty-base")
        self.assertEqual(empty["revision"], "empty")
        self.assertEqual(empty["paragraphs"][0]["status"], "missing")
        self.assertEqual(empty["paragraphs"][0]["candidates"][0]["candidate"], first["candidate"])
        reopened = self.run_tool("inspect", "--base-audio", published["audio"],
                                 "--base-transcript", published["transcript"])
        self.assertEqual(reopened["paragraphs"][0]["status"], "ready")

    def test_managed_partial_chapter_preserves_duplicate_paragraph_identity(self):
        first = "p_0123456789abcdef_1"
        second = "p_0123456789abcdef_2"
        self.sentences = [{"id": name, "text": "Alpha bravo."} for name in (first, second)]
        self.save_manifest()
        candidate = self.synthesize(second)
        adopted = self.run_tool("adopt", second, "--candidate", candidate["candidate"])
        output = speech.read_json(Path(adopted["transcript"]))
        self.assertEqual(output["narration_takes"], {second: {"start": 0, "end": 1}})
        self.assertEqual(output["markers"], {})

        audio, transcript = self.root / "copied.wav", self.root / "copied.speech.json"
        shutil.copyfile(adopted["audio"], audio)
        shutil.copyfile(adopted["transcript"], transcript)
        reopened = self.run_tool("inspect", "--base-audio", str(audio), "--base-transcript", str(transcript))
        self.assertEqual([p["status"] for p in reopened["paragraphs"]], ["missing", "ready"])

    def test_external_chapter_splices_only_selected_speech_and_rebases_markers(self):
        audio = self.root / "prior narration.wav"
        transcript = self.root / "prior narration.speech.json"
        original = struct.pack("<24000h", *range(24000))
        speech.write_wav(audio, [(original, 0)])
        source = {
            "duration": 1,
            "markers": {"intro": 0.05, "alpha": 0.2, "gap": 0.4, "charlie": 0.55, "outro": 0.95},
            "segments": [
                timed_segment("Alpha", 0.2, 0.24), timed_segment("bravo.", 0.26, 0.3),
                timed_segment("Charlie delta.", 0.55, 0.8),
            ],
        }
        transcript.write_bytes(speech.json_bytes(source))
        base = ("--base-audio", str(audio), "--base-transcript", str(transcript))
        self.sentences[0] = {"id": "reworded", "text": "Alpha repeats."}
        self.save_manifest()
        old = self.run_tool("inspect", *base)
        self.assertEqual([p["status"] for p in old["paragraphs"]], ["stale", "ready"])
        selected = self.run_tool("import", "reworded", *self.recording("new", [
            {"word": "Alpha", "start": 0.05, "end": 0.1},
            {"word": "repeats.", "start": 0.16, "end": 0.19},
        ], 4800))
        final = self.run_tool("adopt", "reworded", "--candidate", selected["candidate"], *base,
                              "--expected-revision", old["revision"])
        self.assertEqual(speech.read_pcm(audio), original)
        self.assertEqual(speech.read_pcm(final["audio"]),
                         original[:9600] + speech.read_pcm(selected["audio"]) + original[14400:])
        final_timing = speech.read_json(Path(final["transcript"]))
        self.assertEqual(final_timing["markers"], {
            "intro": 0.05, "alpha": 0.25, "gap": 0.5, "charlie": 0.65, "outro": 1.05,
        })
        self.assertEqual(final_timing["segments"][1]["words"][0]["start"], 0.65)
        self.assertEqual(final["affected_markers"][1],
                         {"marker": "charlie", "before": 0.55, "after": 0.65, "delta": 0.1})

    def test_existing_speech_without_an_authored_paragraph_is_preserved(self):
        audio = self.root / "old.wav"
        transcript = self.root / "old.speech.json"
        original = struct.pack("<24000h", *range(24000))
        speech.write_wav(audio, [(original, 0)])
        transcript.write_bytes(speech.json_bytes({"segments": [
            timed_segment("Unwritten opening.", 0.01, 0.1),
            timed_segment("Charlie delta.", 0.3, 0.5),
            timed_segment("Unwritten ending.", 0.7, 0.9),
        ]}))
        self.sentences = self.sentences[1:]
        self.save_manifest()
        base = ("--base-audio", str(audio), "--base-transcript", str(transcript))
        take = self.synthesize("second")
        adopted = self.run_tool("adopt", "second", "--candidate", take["candidate"], *base)
        self.assertEqual(speech.read_pcm(adopted["audio"]),
                         original[:14400] + speech.read_pcm(take["audio"]) + original[24000:])
        retained_text = [segment["text"] for segment in speech.read_json(Path(adopted["transcript"]))["segments"]]
        self.assertEqual(retained_text, ["Unwritten opening.", "Charlie delta.", "Unwritten ending."])

    def test_stale_candidate_revision_and_tampered_alignment_cannot_publish(self):
        first = self.synthesize("first")
        published = self.run_tool("adopt", "first", "--candidate", first["candidate"])
        baseline = self.store.current.read_bytes()
        self.assertIn("chapter changed", self.run_tool("adopt", "first", "--candidate", first["candidate"],
                                                       "--expected-revision", "empty", ok=False))
        self.sentences[0].update({"id": "rewritten", "text": "Echo bravo.", "markers": {}})
        self.save_manifest()
        status = self.run_tool("inspect")
        self.assertEqual(status["paragraphs"][0]["status"], "stale")
        self.assertIn("candidate is stale", self.run_tool("adopt", "rewritten", "--candidate", first["candidate"],
                                                          ok=False))
        mismatch = self.recording("bad", [{"word": "unrelated", "start": 0, "end": 0.01}], 480)
        self.assertIn("do not match", self.run_tool("import", "rewritten", *mismatch, ok=False))
        replacement = self.synthesize("rewritten")
        Path(replacement["transcript"]).write_text("{}", encoding="utf-8")
        self.assertIn("was modified", self.run_tool("adopt", "rewritten", "--candidate", replacement["candidate"],
                                                    ok=False))
        self.assertEqual(self.store.current.read_bytes(), baseline)
        self.assertEqual(self.run_tool("inspect")["revision"], published["revision"])

    def test_failed_pointer_publication_keeps_previous_revision_available(self):
        first = self.synthesize("first")
        published = self.run_tool("adopt", "first", "--candidate", first["candidate"])
        chapter, revision, audio, transcript, _ = self.store.baseline(speech.read_chapter(self.manifest), None, None)
        self.assertEqual(revision, published["revision"])
        with tempfile.TemporaryDirectory(dir=self.store.cache) as scratch:
            with patch.object(Path, "replace", side_effect=OSError("simulated interruption")):
                with self.assertRaisesRegex(OSError, "simulated interruption"):
                    self.store.write_revision(Path(scratch), chapter)
        reopened = self.run_tool("inspect")
        self.assertEqual((reopened["revision"], reopened["audio"], reopened["transcript"]),
                         (revision, str(audio), str(transcript)))


def timed_segment(text, start, end):
    tokens = text.split()
    interval = (end - start) / len(tokens)
    return {"text": text, "start": start, "end": end,
            "words": [{"word": token, "start": start + index * interval,
                       "end": start + (index + 1) * interval} for index, token in enumerate(tokens)]}


if __name__ == "__main__":
    unittest.main()
