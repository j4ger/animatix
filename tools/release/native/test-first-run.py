#!/usr/bin/env python3
"""Validate the narrated starter using the packaged tools from outside the checkout."""

from array import array
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from tempfile import TemporaryDirectory
import time
from zipfile import ZipFile

from windows_sdk_dxc import stage_sdk_dxc


def verify_workbench(gui, scene, project, env):
    frames = project / "gui-frames.jsonl"
    log_path = project / "gui.log"
    with log_path.open("wb") as log:
        process = subprocess.Popen([str(gui), "--perf-log", str(frames), scene], cwd=project,
                                   env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 60
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise ValueError(f"workbench exited before rendering a preview ({process.returncode})")
                if frames.exists():
                    # A concurrent writer can leave the newest line incomplete.
                    lines = frames.read_text(encoding="utf-8").split("\n")[:-1]
                    if any(row["actors"] > 0 and row["render_ms"] > 0 and not row["stale"]
                           for row in map(json.loads, lines)):
                        print("Packaged workbench rendered the narrated project.")
                        return
                time.sleep(0.25)
            raise ValueError("workbench did not render the project within 60 seconds")
        except Exception:
            print(log_path.read_text(encoding="utf-8", errors="replace")[-8000:], file=sys.stderr)
            raise
        finally:
            if process.poll() is None:
                process.kill()
            process.wait()


def verify(package, project, version, env):
    suffix = ".exe" if sys.platform == "win32" else ""
    gui = package / ("animatix-gui" + suffix)
    bundle = package / "Animatix Studio.app/Contents/MacOS/animatix-gui"
    if version and sys.platform == "darwin":
        gui = bundle

    def run(tool, *args):
        return subprocess.check_output([str(package / (tool + suffix)), *args], cwd=project, env=env)

    for binary, option in [(package / ("animatix" + suffix), "doctor"), (gui, "--doctor")]:
        if version:
            output = subprocess.check_output([str(binary), "--version"], cwd=project, env=env, text=True)
            if output.split()[-1:] != [version]:
                raise ValueError(f"{binary.name} reports {output.strip()!r}; expected version {version}")
        report = subprocess.run(
            [str(binary), option], cwd=project, env=env,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False,
        )
        (project / f"{binary.stem}-doctor.txt").write_bytes(report.stdout)
        if report.returncode:
            print(report.stdout.decode(errors="replace"), file=sys.stderr)
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
    if version:
        verify_workbench(gui, scene, project, env)

    # Backdrop deliberately paints a full-canvas gradient. Render that same
    # layer alone so edge checks distinguish authored background from stray ink.
    starter_dir = project / starter
    reference = starter_dir / "backdrop-reference.amx"
    template = (starter_dir / "main.amx").read_text(encoding="utf-8")
    config = next(line for line in template.splitlines() if line.startswith("config "))
    reference.write_text(
        f'import "./layout.amx"\n{config}\n#0s\nbackdrop: Backdrop\n', encoding="utf-8",
    )

    def rgb(image):
        return run("ffmpeg", "-v", "error", "-i", image, "-pix_fmt", "rgb24", "-f", "rawvideo", "-")

    run("animatix", "image", str(reference), "--time", "4", "--output", "backdrop-reference.png")
    background = rgb("backdrop-reference.png")
    for time, image in [("4", "preview.png"), ("8.5", "preview-last-cue.png")]:
        run("animatix", "image", scene, "--time", time, "--output", image)
        pixels = rgb(image)
        if len(pixels) != 640 * 360 * 3 or len(background) != len(pixels):
            raise ValueError(f"starter preview has unexpected dimensions at {time}s")
        if sum(before != after for before, after in zip(background, pixels)) < 300:
            raise ValueError(f"starter preview has no foreground at {time}s")
        row_width = 640 * 3
        for row in range(0, len(pixels), row_width):
            left, right = slice(row, row + 3), slice(row + row_width - 3, row + row_width)
            if pixels[left] != background[left] or pixels[right] != background[right]:
                raise ValueError(f"starter content touches the canvas edge at {time}s")

    run("animatix", "video", scene, "--fps", "10", "--threads", "1", "--hold", "0", "--codec", "libx264", "--output", "narrated.mp4")
    probe = run("ffprobe", "-v", "error", "-count_frames", "-show_streams", "-show_format", "-of", "json", "narrated.mp4")
    (project / "mp4-probe.json").write_bytes(probe)
    metadata = json.loads(probe)
    streams = {row["codec_type"]: row for row in metadata["streams"]}
    if streams["video"]["codec_name"] != "h264" or streams["video"]["nb_read_frames"] != "110":
        raise ValueError("expected 110 decodable H.264 frames")
    if streams["audio"]["codec_name"] != "aac" or abs(float(metadata["format"]["duration"]) - 11.0) > 0.05:
        raise ValueError("expected an AAC audio stream and an 11-second MP4")
    samples = array("h")
    samples.frombytes(run("ffmpeg", "-v", "error", "-xerror", "-i", "narrated.mp4", "-map", "0:a:0", "-ac", "1", "-ar", "16000", "-f", "s16le", "-"))
    if sys.byteorder == "big":
        samples.byteswap()
    # The template's final sentence is late in the clip; it must not
    # accidentally regress to a short clip plus a long silent hold.
    ending = samples[8 * 16000: 10 * 16000]
    if len(ending) != 2 * 16000 or sum(sample * sample for sample in ending) / len(ending) < 200**2:
        raise ValueError("the final section of the starter is missing audible narration")
    speech = samples[: 10 * 16000]
    if len(speech) != 10 * 16000 or sum(sample * sample for sample in speech) / len(speech) < 200**2:
        raise ValueError("the AAC stream is missing audible narration")
    print("Starter preview, 110 decoded H.264 frames, and audible AAC narration verified.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package", type=Path)
    parser.add_argument("evidence_dir", type=Path)
    parser.add_argument("--version", help="Expected version in an assembled release ZIP")
    args = parser.parse_args()
    env = os.environ.copy()
    with TemporaryDirectory(prefix="Animatix first run ") as directory:
        temporary = Path(directory)
        project = temporary / "project"
        project.mkdir()
        try:
            package = args.package.resolve()
            if args.version:
                with package.open("rb") as stream:
                    sha = hashlib.file_digest(stream, "sha256").hexdigest()
                (project / "archive-sha256.txt").write_text(f"{sha}  {package.name}\n", encoding="ascii")
                extracted = temporary / "installed package"
                with ZipFile(package) as archive:
                    for member in archive.infolist():
                        relative = Path(member.filename)
                        if relative.anchor or ".." in relative.parts or "\\" in member.filename or ":" in member.filename:
                            raise ValueError(f"unsafe release ZIP entry: {member.filename}")
                    archive.extractall(extracted)
                    for member in archive.infolist():
                        if member.external_attr & (0o111 << 16):
                            (extracted / member.filename).chmod(0o755)
                package = extracted
                env["PATH"] = os.defpath if sys.platform != "win32" else str(Path(os.environ["SystemRoot"]) / "System32")
                env.pop("DYLD_LIBRARY_PATH", None)
                env.pop("DYLD_FALLBACK_LIBRARY_PATH", None)
                if sys.platform == "win32" and env.get("GITHUB_ACTIONS") == "true":
                    # Hosted WARP needs DXC. Keep its SDK DLLs outside the tested ZIP.
                    sdk = temporary / "runner graphics"
                    sdk.mkdir()
                    stage_sdk_dxc(sdk)
                    env["PATH"] += os.pathsep + str(sdk)
            verify(package, project, args.version, env)
        finally:
            shutil.copytree(project, args.evidence_dir, dirs_exist_ok=True)


if __name__ == "__main__":
    main()
