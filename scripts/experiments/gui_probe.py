"""Launch the GUI on a real runner display and save a screenshot for evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time


def _image_record(path: Path) -> dict:
    if not path.is_file() or path.stat().st_size < 4096:
        raise RuntimeError(f"GUI screenshot is missing or implausibly small: {path}")
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": digest}


def _terminate_group(process: subprocess.Popen, timeout: float = 5):
    if process.poll() is not None:
        return
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        process.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()


def _linux_probe(binary: Path, scene: Path, screenshot: Path, timeout: float) -> dict:
    for tool in ("xdotool", "import"):
        if not shutil_which(tool):
            raise RuntimeError(f"required Linux GUI probe tool is missing: {tool}")
    screenshot.parent.mkdir(parents=True, exist_ok=True)
    process = subprocess.Popen([str(binary), str(scene)], start_new_session=True)
    deadline = time.monotonic() + timeout
    windows = []
    visible_since = None
    try:
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError(f"GUI exited before opening a visible window: {process.returncode}")
            found = subprocess.run(
                ["xdotool", "search", "--onlyvisible", "--name", "Animatix"],
                capture_output=True, text=True, check=False,
            )
            windows = [item for item in found.stdout.splitlines() if item.strip()]
            if found.returncode == 0 and windows:
                if visible_since is None:
                    visible_since = time.monotonic()
                if time.monotonic() - visible_since >= 3:
                    capture = subprocess.run(
                        ["import", "-window", "root", str(screenshot)], capture_output=True,
                        text=True, check=False, timeout=30,
                    )
                    if capture.returncode:
                        raise RuntimeError(f"ImageMagick screenshot failed: {capture.stderr.strip()}")
                    record = _image_record(screenshot)
                    record.update({"platform": "linux", "window_ids": windows, "process_pid": process.pid})
                    return record
            else:
                visible_since = None
            time.sleep(0.25)
        raise RuntimeError(f"GUI window did not appear within {timeout}s")
    finally:
        _terminate_group(process)


def _pids_named(name: str) -> set[int]:
    result = subprocess.run(["pgrep", "-x", name], capture_output=True, text=True, check=False)
    if result.returncode not in (0, 1):
        raise RuntimeError(f"pgrep failed: {result.stderr.strip()}")
    return {int(line) for line in result.stdout.splitlines() if line.strip().isdigit()}


def _mac_capture(screenshot: Path) -> dict:
    screenshot.parent.mkdir(parents=True, exist_ok=True)
    capture = subprocess.run(
        ["screencapture", "-x", str(screenshot)], capture_output=True,
        text=True, check=False, timeout=30,
    )
    if capture.returncode:
        raise RuntimeError(f"screencapture failed: {capture.stderr.strip()}")
    record = _image_record(screenshot)
    dimensions = subprocess.run(
        ["sips", "-g", "pixelWidth", "-g", "pixelHeight", str(screenshot)],
        capture_output=True, text=True, check=False,
    )
    record["dimensions_probe"] = dimensions.stdout.strip()
    if dimensions.returncode or "pixelWidth" not in dimensions.stdout or "pixelHeight" not in dimensions.stdout:
        raise RuntimeError("captured macOS screen image dimensions are unavailable")
    return record


def _mac_window_names(process_name: str) -> list[str]:
    result = subprocess.run(
        ["osascript", "-e",
         f'tell application "System Events" to tell process "{process_name}" to get name of every window'],
        capture_output=True, text=True, check=False, timeout=10,
    )
    if result.returncode:
        raise RuntimeError(f"could not inspect GUI windows with System Events: {result.stderr.strip()}")
    value = result.stdout.strip()
    if not value or value == "missing value":
        return []
    return [name.strip() for name in value.split(",") if name.strip()]


def _mac_binary_probe(binary: Path, scene: Path, screenshot: Path, timeout: float) -> dict:
    process = subprocess.Popen([str(binary), str(scene)], start_new_session=True)
    try:
        started = time.monotonic()
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError(f"GUI exited before screenshot: {process.returncode}")
            windows = _mac_window_names("animatix-gui")
            if windows and time.monotonic() - started >= 3:
                record = _mac_capture(screenshot)
                record.update({"platform": "macos", "process_pid": process.pid,
                               "window_names": windows})
                return record
            time.sleep(0.25)
        raise RuntimeError(f"GUI did not stay open for {timeout}s")
    finally:
        _terminate_group(process)


def _mac_bundle_probe(app: Path, scene: Path, screenshot: Path, timeout: float) -> dict:
    before = _pids_named("animatix-gui")
    launch = subprocess.run(
        ["open", "-a", str(app), "--args", str(scene)], capture_output=True,
        text=True, check=False, timeout=30,
    )
    if launch.returncode:
        raise RuntimeError(f"LaunchServices failed to open app bundle: {launch.stderr.strip()}")
    new_pids = set()
    visible_since = None
    deadline = time.monotonic() + timeout
    try:
        while time.monotonic() < deadline:
            new_pids = _pids_named("animatix-gui") - before
            if new_pids:
                windows = _mac_window_names("animatix-gui")
                if windows:
                    if visible_since is None:
                        visible_since = time.monotonic()
                    if time.monotonic() - visible_since >= 3:
                        record = _mac_capture(screenshot)
                        record.update({"platform": "macos", "app_bundle": str(app),
                                       "new_process_pids": sorted(new_pids),
                                       "window_names": windows})
                        return record
                else:
                    visible_since = None
            time.sleep(0.25)
        raise RuntimeError(f"packaged GUI process did not appear within {timeout}s")
    finally:
        if new_pids:
            subprocess.run(
                ["osascript", "-e", 'tell application id "io.github.j4ger.animatix" to quit'],
                capture_output=True, text=True, check=False, timeout=10,
            )
            for pid in new_pids:
                try:
                    os.kill(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass


def shutil_which(program: str) -> str | None:
    from shutil import which
    return which(program)


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("linux", "macos", "macos-bundle"))
    parser.add_argument("binary_or_app", type=Path)
    parser.add_argument("scene", type=Path)
    parser.add_argument("screenshot", type=Path)
    parser.add_argument("--timeout", type=float, default=90)
    args = parser.parse_args(argv)
    try:
        if args.mode == "linux":
            evidence = _linux_probe(args.binary_or_app, args.scene, args.screenshot, args.timeout)
        elif args.mode == "macos":
            evidence = _mac_binary_probe(args.binary_or_app, args.scene, args.screenshot, args.timeout)
        else:
            evidence = _mac_bundle_probe(args.binary_or_app, args.scene, args.screenshot, args.timeout)
        print(json.dumps(evidence, sort_keys=True))
        return 0
    except (OSError, RuntimeError, subprocess.SubprocessError, ValueError) as error:
        print(f"GUI launch probe failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
