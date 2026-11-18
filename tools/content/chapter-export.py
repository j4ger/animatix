#!/usr/bin/env python3
"""Render changed Animatix chapters, then assemble the whole MP4."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from fractions import Fraction
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time


def parser():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("input", type=Path, help="multi-scene .amx composition")
    cli.add_argument("-o", "--output", type=Path, required=True, help="final .mp4 file")
    cli.add_argument("--cache-dir", type=Path, help="default: .animatix-chapters beside the source")
    cli.add_argument("--animatix", default="animatix", help="video-enabled CLI executable")
    cli.add_argument("--ffmpeg", default="ffmpeg", help="FFmpeg executable")
    cli.add_argument("--ffprobe", default="ffprobe", help="FFprobe executable")
    cli.add_argument("--dependency", action="append", default=[], type=Path,
                     help="additional file that affects rendering, such as an installed font")
    cli.add_argument("--width", type=int)
    cli.add_argument("--height", type=int)
    cli.add_argument("--fps", type=int)
    cli.add_argument("--duration", type=float)
    cli.add_argument("--hold", type=float)
    cli.add_argument("-j", "--threads")
    cli.add_argument("--codec", choices=("auto", "libx264", "h264_nvenc", "h264_vaapi"))
    cli.add_argument("--preset")
    cli.add_argument("--export-preset")
    cli.add_argument("--debug-bounds", action="store_true")
    return cli


def canonical_hash(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def file_hash(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def file_identity(path):
    stat = path.stat()
    return (stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns)


class Inputs:
    def __init__(self, plan_started):
        self.plan_started = plan_started
        self.files = {}

    def digest(self, raw_path):
        if str(raw_path).startswith("data:"):
            return canonical_hash(str(raw_path))
        if str(raw_path).startswith(("http://", "https://")):
            raise ValueError(f"Chapter export needs a local file for {raw_path}")
        path = Path(raw_path).absolute()
        if path not in self.files:
            identity = file_identity(path)
            digest = file_hash(path)
            if identity[-1] > self.plan_started or identity != file_identity(path):
                raise ValueError(f"Input changed while preparing the export: {path}. Run again.")
            self.files[path] = (identity, digest)
        return self.files[path][1]

    def verify(self):
        for path, (identity, _) in self.files.items():
            if identity != file_identity(path):
                raise ValueError(f"Input changed during the export: {path}. Run again.")


def command_path(name):
    path = shutil.which(name)
    if path is None:
        raise ValueError(f"Executable not found: {name}")
    return str(Path(path).absolute())


def run_json(command):
    result = subprocess.run(command, check=True, stdout=subprocess.PIPE, text=True)
    return json.loads(result.stdout)


def flags(args, names):
    result = []
    for name in names:
        value = getattr(args, name)
        if value is not None:
            result.extend(("--" + name.replace("_", "-"), str(value)))
    return result


def validate_plan(plan):
    if plan["version"] != 1:
        raise ValueError("Unsupported chapter plan version. Use the CLI from this checkout.")
    if plan["codec"] not in ("auto", "libx264", "h264_nvenc", "h264_vaapi"):
        raise ValueError("Chapter export requires an H.264 encoder")
    next_frame = 0
    for chapter in plan["chapters"]:
        if chapter["start_frame"] != next_frame or chapter["end_frame"] <= next_frame:
            raise ValueError("Chapter plan does not cover a contiguous frame range")
        next_frame = chapter["end_frame"]
    if next_frame == 0:
        raise ValueError("Chapter plan contains no frames")


def chapter_key(plan, chapter, scene_keys, render_settings):
    return canonical_hash({
        "render": render_settings,
        "shared_source": plan["shared_source"],
        "samples": chapter["sampling"],
        "frames": chapter["end_frame"] - chapter["start_frame"],
        "scenes": {name: scene_keys[name] for name in chapter["scenes"]},
    })


def probe(ffprobe, path):
    return run_json([ffprobe, "-v", "error", "-show_streams", "-show_chapters", "-of", "json", str(path)])


def validate_video(ffprobe, path, plan, frames, audio=False, chapters=()):
    media = probe(ffprobe, path)
    streams = media["streams"]
    if chapters and streams[-1]["codec_type"] == "data" and streams[-1].get("codec_tag_string") == "text":
        streams = streams[:-1]  # QuickTime chapter track in MP4.
    expected = [("video", "h264"), ("audio", "aac")] if audio else [("video", "h264")]
    if [(s["codec_type"], s["codec_name"]) for s in streams] != expected:
        raise ValueError(f"Unexpected streams in {path}")
    video = streams[0]
    if (video["width"], video["height"], int(video["nb_frames"])) != (plan["width"], plan["height"], frames):
        raise ValueError(f"Unexpected dimensions or frame count in {path}")
    time_base = Fraction(video["time_base"])
    if int(video["start_pts"]) != 0 or int(video["duration_ts"]) * time_base != Fraction(frames, plan["fps"]):
        raise ValueError(f"Unexpected video timestamps in {path}")
    actual = media.get("chapters", [])
    if len(actual) != len(chapters):
        raise ValueError(f"Unexpected chapter count in {path}")
    for expected, chapter in zip(chapters, actual):
        # The MP4 chapter timescale is milliseconds. Video retains the exact frame grid.
        chapter_time_base = Fraction(chapter["time_base"])
        if (chapter.get("tags", {}).get("title") != expected["name"]
                or abs(chapter["start"] * chapter_time_base
                       - Fraction(expected["start_frame"], plan["fps"])) > Fraction(1, 1000)
                or abs(chapter["end"] * chapter_time_base
                       - Fraction(expected["end_frame"], plan["fps"])) > Fraction(1, 1000)):
            raise ValueError(f"Unexpected chapter title or timestamps in {path}")


def validate_audio(ffprobe, path):
    streams = probe(ffprobe, path)["streams"]
    if len(streams) != 1 or streams[0]["codec_type"] != "audio" or streams[0]["codec_name"] != "aac":
        raise ValueError(f"Expected a single AAC stream in {path}")


@dataclass(frozen=True)
class CacheEntry:
    path: Path

    @property
    def checksum(self):
        return self.path.with_suffix(self.path.suffix + ".sha256")

    def valid(self):
        try:
            expected = self.checksum.read_text(encoding="ascii").strip()
            return self.path.stat().st_size > 0 and file_hash(self.path) == expected
        except (OSError, UnicodeError):
            return False

    def publish(self, staged):
        checksum = staged.with_suffix(".sha256")
        checksum.write_text(file_hash(staged) + "\n", encoding="ascii")
        os.replace(staged, self.path)
        os.replace(checksum, self.checksum)


def cached_export(entry, command, validate, inputs, label):
    if entry.valid():
        print(f"Reuse {label}", flush=True)
        return entry.path
    print(f"Render {label}", flush=True)
    with tempfile.TemporaryDirectory(prefix=".render-", dir=entry.path.parent) as folder:
        staged = Path(folder) / entry.path.name
        inputs.verify()
        subprocess.run([*command, "-o", str(staged)], check=True)
        validate(staged)
        inputs.verify()
        entry.publish(staged)
    return entry.path


def microseconds(frame, fps):
    return (frame * 1_000_000 + fps // 2) // fps


def chapter_metadata(plan):
    text = [";FFMETADATA1"]
    for chapter in plan["chapters"]:
        title = "".join("\\" + char if char in "\\=#;\n" else char for char in chapter["name"])
        text.extend(("[CHAPTER]", f"TIMEBASE=1/{plan['fps']}", f"START={chapter['start_frame']}",
                     f"END={chapter['end_frame']}", f"title={title}"))
    return "\n".join(text) + "\n"


def assemble(ffmpeg, ffprobe, cache, output, plan, parts, audio, inputs):
    # The list lives in the cache folder, so all entries are safe relative
    # names even when the user's source or cache path contains punctuation.
    with tempfile.NamedTemporaryFile(mode="w", encoding="ascii", suffix=".ffconcat",
                                     prefix=".chapters-", dir=cache, delete=False) as concat:
        concat.write("ffconcat version 1.0\n")
        for part, chapter in zip(parts, plan["chapters"], strict=True):
            start = microseconds(chapter["start_frame"], plan["fps"])
            end = microseconds(chapter["end_frame"], plan["fps"])
            # Round absolute boundaries before subtraction so concat's AV_TIME_BASE
            # rounding does not accumulate across chapters.
            duration = end - start
            concat.write(f"file {part.name}\nduration {duration // 1_000_000}.{duration % 1_000_000:06d}\n")
    try:
        with tempfile.TemporaryDirectory(prefix=".assemble-", dir=output.parent) as folder:
            staged = Path(folder) / "video.mp4"
            metadata = Path(folder) / "chapters.ffmeta"
            metadata.write_text(chapter_metadata(plan), encoding="utf-8")
            command = [ffmpeg, "-hide_banner", "-loglevel", "warning", "-nostdin",
                       "-f", "concat", "-safe", "1", "-i", concat.name]
            if audio is not None:
                command.extend(("-i", str(audio)))
            command.extend(("-f", "ffmetadata", "-i", str(metadata),
                            "-map_chapters", str(1 + (audio is not None))))
            command.extend(("-map", "0:v:0"))
            if audio is not None:
                command.extend(("-map", "1:a:0"))
            command.extend(("-c", "copy", "-movflags", "+faststart", str(staged)))
            subprocess.run(command, check=True)
            validate_video(ffprobe, staged, plan, plan["chapters"][-1]["end_frame"],
                           audio is not None, plan["chapters"])
            inputs.verify()
            os.replace(staged, output)
    finally:
        Path(concat.name).unlink(missing_ok=True)


def run(args):
    if args.output.suffix.lower() != ".mp4":
        raise ValueError("Chapter export output must have an .mp4 extension")
    input_file = args.input.absolute()
    output = args.output.absolute()
    cache = (args.cache_dir or input_file.parent / ".animatix-chapters").absolute()
    binary = command_path(args.animatix)
    ffmpeg, ffprobe = command_path(args.ffmpeg), command_path(args.ffprobe)
    settings = flags(args, ("width", "height", "fps", "duration", "hold", "codec", "preset", "export_preset"))
    inputs = Inputs(time.time_ns())
    inputs.digest(input_file)
    render_base = {
        "version": 1,
        "animatix": inputs.digest(binary),
        "environment": {name: value for name, value in os.environ.items()
                        if name.startswith(("ANIMATIX_", "WGPU_", "VK_", "FONTCONFIG_"))},
        "extra": {str(path): inputs.digest(path) for path in args.dependency},
    }
    plan = run_json([binary, "--no-color", "chapter-plan", str(input_file), *settings])
    validate_plan(plan)
    for path, expected in plan["files"].items():
        if inputs.digest(path) != expected:
            raise ValueError(f"Source changed while preparing the export: {path}. Run again.")
    scene_keys = {}
    for name in sorted({name for chapter in plan["chapters"] for name in chapter["scenes"]}):
        scene = plan["scenes"][name]
        scene_keys[name] = [scene["source"], {path: inputs.digest(path) for path in scene["assets"]}]
    audio_segments = [{**segment, "sha256": inputs.digest(segment["source"])}
                      for segment in plan["audio"]]
    if output.resolve() in {path.resolve() for path in inputs.files}:
        raise ValueError("The output path is also an input")
    cache.mkdir(parents=True, exist_ok=True)
    output.parent.mkdir(parents=True, exist_ok=True)

    render_settings = {
        **render_base,
        **{name: plan[name] for name in ("width", "height", "fps", "codec", "preset")},
        "threads": args.threads,
        "debug_bounds": args.debug_bounds,
    }
    video_flags = [*settings, *flags(args, ("threads",))]
    if args.debug_bounds:
        video_flags.append("--debug-bounds")
    parts = []
    for chapter in plan["chapters"]:
        name = chapter["name"]
        frames = chapter["end_frame"] - chapter["start_frame"]
        key = chapter_key(plan, chapter, scene_keys, render_settings)
        entry = CacheEntry(cache / f"chapter-{key}.mp4")
        parts.append(cached_export(
            entry,
            [binary, "--no-color", "video", str(input_file), "--scene", name, "--no-audio", *video_flags],
            lambda path: validate_video(ffprobe, path, plan, frames),
            inputs,
            f"chapter {name}",
        ))

    audio = None
    if audio_segments:
        duration = plan["duration_s"]
        key = canonical_hash({"render": render_base, "duration": duration, "segments": audio_segments})
        audio = cached_export(
            CacheEntry(cache / f"audio-{key}.m4a"),
            [binary, "--no-color", "audio", str(input_file), "--duration", str(duration)],
            lambda path: validate_audio(ffprobe, path),
            inputs,
            "composition audio",
        )
    assemble(ffmpeg, ffprobe, cache, output, plan, parts, audio, inputs)
    print(f"Wrote {output}")


def main():
    try:
        run(parser().parse_args())
    except (KeyError, OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Chapter export failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
