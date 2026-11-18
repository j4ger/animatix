#!/usr/bin/env python3
"""Validate the narrated starter using the packaged tools from outside the checkout."""

from array import array
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys
from tempfile import TemporaryDirectory


def verify(package, project):
    suffix = ".exe" if sys.platform == "win32" else ""

    def run(tool, *args):
        return subprocess.check_output([str(package / (tool + suffix)), *args], cwd=project)

    for tool, option in [("animatix", "doctor"), ("animatix-gui", "--doctor")]:
        report = subprocess.run(
            [str(package / (tool + suffix)), option], cwd=project,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False,
        )
        (project / f"{tool}-doctor.txt").write_bytes(report.stdout)
        report.check_returncode()

    # A path with spaces also exercises quoting in asset resolution and FFmpeg.
    starter = "Narrated Project"
    scene = f"{starter}/main.amx"
    run("animatix", "init", starter)
    report = json.loads(run("animatix", "check", scene, "--format", "json"))
    (project / "check.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    problems = [item for item in report["errors"] if item["severity"] in {"warning", "error"}]
    if not report["passed"] or problems:
        raise ValueError(f"starter build diagnostics: {problems}")

    for time, image in [("4", "preview.png"), ("8.5", "preview-last-cue.png")]:
        run("animatix", "image", scene, "--time", time, "--output", image)
        pixels = run("ffmpeg", "-v", "error", "-i", image, "-pix_fmt", "rgb24", "-f", "rawvideo", "-")
        if len(set(zip(pixels[::3], pixels[1::3], pixels[2::3]))) < 16:
            raise ValueError(f"starter preview did not render at {time}s")
        row_width = 640 * 3
        background = pixels[:3]
        for row in range(0, len(pixels), row_width):
            if pixels[row:row + 3] != background or pixels[row + row_width - 3:row + row_width] != background:
                raise ValueError(f"starter content touches the canvas edge at {time}s")

    run("animatix", "video", scene, "--fps", "10", "--threads", "1", "--hold", "0", "--codec", "libx264", "--output", "narrated.mp4")
    probe = run("ffprobe", "-v", "error", "-count_frames", "-show_streams", "-show_format", "-of", "json", "narrated.mp4")
    (project / "mp4-probe.json").write_bytes(probe)
    metadata = json.loads(probe)
    streams = {row["codec_type"]: row for row in metadata["streams"]}
    if streams["video"]["codec_name"] != "h264" or streams["video"]["nb_read_frames"] != "300":
        raise ValueError("expected 300 decodable H.264 frames")
    if streams["audio"]["codec_name"] != "aac" or abs(float(metadata["format"]["duration"]) - 30.0) > 0.05:
        raise ValueError("expected an AAC audio stream and a 30-second MP4")
    samples = array("h")
    samples.frombytes(run("ffmpeg", "-v", "error", "-xerror", "-i", "narrated.mp4", "-map", "0:a:0", "-ac", "1", "-ar", "16000", "-f", "s16le", "-"))
    if sys.byteorder == "big":
        samples.byteswap()
    speech = samples[: 10 * 16000]
    if len(speech) != 10 * 16000 or sum(sample * sample for sample in speech) / len(speech) < 200**2:
        raise ValueError("the AAC stream is missing audible narration")
    print("Starter preview, 300 decoded H.264 frames, and audible AAC narration verified.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bin_dir", type=Path)
    parser.add_argument("evidence_dir", type=Path)
    args = parser.parse_args()
    with TemporaryDirectory(prefix="animatix-first-run-") as directory:
        project = Path(directory)
        try:
            verify(args.bin_dir.resolve(), project)
        finally:
            shutil.copytree(project, args.evidence_dir, dirs_exist_ok=True)


if __name__ == "__main__":
    main()
