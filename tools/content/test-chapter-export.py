#!/usr/bin/env python3
"""Test chapter cache invalidation, failure recovery, and frame-grid assembly."""

import contextlib
import importlib.util
import io
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location("chapter_export", Path(__file__).with_name("chapter-export.py"))
export = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = export
SPEC.loader.exec_module(export)


class CacheTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.source = self.root / "lesson.amx"
        self.source.write_text("composition")
        self.voice = self.root / "middle tone.wav"
        self.voice.write_bytes(b"voice")
        self.animatix = self.root / "animatix"
        self.animatix.write_bytes(b"renderer")
        self.args = export.parser().parse_args([str(self.source), "-o", str(self.root / "lesson.mp4")])
        self.plan = {
            "version": 1, "width": 32, "height": 32, "fps": 30, "duration_s": 1,
            "codec": "auto", "preset": "medium", "shared_source": "global",
            "files": {str(self.source): export.file_hash(self.source)},
            "scenes": {name: {"source": name, "assets": []} for name in ("A", "B", "C", "D")},
            "chapters": [
                {"name": name, "start_frame": i * 7, "end_frame": (i + 1) * 7,
                 "sampling": name, "scenes": contributors}
                for i, (name, contributors) in enumerate(
                    [("A", ["A"]), ("B", ["A", "B"]), ("C", ["B", "C"]), ("D", ["C", "D"])]
                )
            ],
            "audio": [{"source": str(self.voice), "start_time_s": 0.3, "duration_s": 0.4,
                       "volume": 1, "volume_envelope": None}],
        }
        self.plan["scenes"]["B"]["assets"].append(str(self.voice))
        self.calls = []
        self.fail_scene = None
        self.edit_while_rendering = None

    def export(self):
        self.calls.clear()
        with (
            patch.object(export, "command_path", side_effect=lambda name: str(self.animatix) if name == "animatix" else name),
            patch.object(export, "run_json", return_value=self.plan),
            patch.object(export, "validate_video"),
            patch.object(export, "validate_audio"),
            patch.object(export.subprocess, "run", side_effect=self.fake_run),
            contextlib.redirect_stdout(io.StringIO()),
        ):
            export.run(self.args)
        return list(self.calls)

    def fake_run(self, command, **_kwargs):
        if "--scene" in command:
            label = command[command.index("--scene") + 1]
        else:
            label = "mix" if "audio" in command else "assemble"
        self.calls.append(label)
        Path(command[-1]).write_bytes(label.encode())
        if label == self.edit_while_rendering:
            self.voice.write_bytes(b"voice changed while rendering")
        if label == self.fail_scene:
            raise subprocess.CalledProcessError(1, command)

    def test_reuses_parts_and_regenerates_transition_neighbors(self):
        self.assertEqual(self.export(), ["A", "B", "C", "D", "mix", "assemble"])
        self.assertEqual(self.export(), ["assemble"])
        self.plan["scenes"]["B"]["source"] = "the middle changed"
        self.assertEqual(self.export(), ["B", "C", "assemble"])
        self.voice.write_bytes(b"new voice")
        self.assertEqual(self.export(), ["B", "C", "mix", "assemble"])
        self.plan["chapters"][2]["sampling"] = "a different crossfade"
        self.assertEqual(self.export(), ["C", "assemble"])

    def test_reuses_equal_samples_after_an_earlier_chapter_grows(self):
        self.export()
        self.plan["chapters"][0]["end_frame"] += 1
        for chapter in self.plan["chapters"][1:]:
            chapter["start_frame"] += 1
            chapter["end_frame"] += 1
        self.plan["chapters"][1]["sampling"] = "new incoming transition"
        self.plan["audio"][0]["start_time_s"] += 1 / 30
        self.assertEqual(self.export(), ["A", "B", "mix", "assemble"])

    def test_global_render_settings_and_binary_invalidate_all_video_parts(self):
        self.export()
        self.plan["shared_source"] = "new global colors"
        self.assertEqual(self.export(), ["A", "B", "C", "D", "assemble"])
        self.plan["preset"] = "fast"
        self.assertEqual(self.export(), ["A", "B", "C", "D", "assemble"])
        self.animatix.write_bytes(b"new renderer")
        self.assertEqual(self.export(), ["A", "B", "C", "D", "mix", "assemble"])

    def test_failed_render_and_corrupt_hit_are_rebuilt(self):
        self.args.output.write_bytes(b"original")
        self.fail_scene = "B"
        with self.assertRaises(subprocess.CalledProcessError):
            self.export()
        self.assertEqual(self.args.output.read_bytes(), b"original")
        self.fail_scene = None
        self.assertEqual(self.export(), ["B", "C", "D", "mix", "assemble"])
        cache = self.root / ".animatix-chapters"
        file = next(path for path in cache.glob("chapter-*.mp4") if path.read_bytes() == b"C")
        file.write_bytes(b"corrupt")
        self.assertEqual(self.export(), ["C", "assemble"])

    def test_edit_during_render_keeps_prior_film_and_successful_parts(self):
        self.args.output.write_bytes(b"original")
        self.edit_while_rendering = "B"
        with self.assertRaisesRegex(ValueError, "Input changed during the export"):
            self.export()
        self.assertEqual(self.args.output.read_bytes(), b"original")
        self.edit_while_rendering = None
        self.assertEqual(self.export(), ["B", "C", "D", "mix", "assemble"])


@unittest.skipUnless(shutil.which("ffmpeg") and shutil.which("ffprobe"), "FFmpeg tools are unavailable")
class AssemblyTests(unittest.TestCase):
    def test_joins_fractional_frame_parts_and_continuous_audio(self):
        with tempfile.TemporaryDirectory(prefix="chapter's test ") as folder:
            root = Path(folder)
            fps = 29
            plan = {"width": 32, "height": 32, "fps": fps, "chapters": []}
            parts = []
            start = 0
            for length in (1, 10, 1, 7, 33, 1, 3):
                part = root / f"safe{len(parts)}.mp4"
                self.ffmpeg("-f", "lavfi", "-i", f"color=red:s=32x32:r={fps}",
                            "-c:v", "libx264", "-bf", "0", "-frames:v", str(length), str(part))
                plan["chapters"].append({"name": f"Chapter {len(parts)}# : 学习", "start_frame": start,
                                         "end_frame": start + length})
                parts.append(part)
                start += length
            audio = root / "audio.m4a"
            self.ffmpeg("-f", "lavfi", "-i", "sine=frequency=440", "-t", str(start / fps),
                        "-c:a", "aac", str(audio))
            output = root / "assembled.mp4"
            export.assemble("ffmpeg", "ffprobe", root, output, plan, parts, audio, export.Inputs(0))
            expect_audio = export.probe("ffprobe", audio)["streams"][0]
            actual_audio = export.probe("ffprobe", output)["streams"][1]
            self.assertEqual(actual_audio["duration_ts"], expect_audio["duration_ts"])
            self.assertEqual(actual_audio["start_pts"], expect_audio["start_pts"])

    def ffmpeg(self, *args):
        result = subprocess.run(["ffmpeg", "-loglevel", "error", "-nostdin", *args],
                                capture_output=True, text=True)
        if "Unknown encoder" in result.stderr:
            self.skipTest("FFmpeg is missing the fixture's software encoder")
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
