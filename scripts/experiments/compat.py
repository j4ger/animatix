"""Functional CLI, renderer, GUI, and macOS package checks for linker arms."""

from __future__ import annotations

import json
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tomllib
import zlib

from scripts.experiments.common import ExperimentFailure, ExperimentOutput, run_timed
from scripts.experiments.linkers import (
    collect_elf_linker_evidence, collect_wrapper_evidence,
)


SMOKE_SCENE = Path("examples/basics/00_hello.amx")


def _run(output: ExperimentOutput, repo: Path, platform: str, arm: dict,
         target: Path | None, step: str, command: list[str], timeout: float = 300) -> dict:
    name = f"{platform}-compat-{arm['id']}-{step}"
    env = {
        **arm["env"],
        "CARGO_TARGET_DIR": str(target) if target else None,
        "CARGO_INCREMENTAL": "0",
        "CARGO_TERM_COLOR": "never",
        "ANIMATIX_LINKER_AUDIT_LOG": str(output.logs_dir / f"{name}.linker-audit.jsonl")
        if arm.get("audit") else None,
    }
    result = run_timed(name, command, repo, env, output.logs_dir / f"{name}.log",
                       timeout_seconds=timeout)
    result.update({"type": "compatibility-step", "platform": platform,
                   "arm": arm["id"], "step": step, "command": command})
    output.append_record(result)
    if result["exit_code"] or result["timed_out"]:
        raise ExperimentFailure(
            f"{platform} {arm['id']} compatibility step {step} failed; see {result['log_path']}"
        )
    return result


def _assert_log_contains(output: ExperimentOutput, run: dict, expected: str, label: str):
    content = Path(run["log_path"]).read_text(encoding="utf-8", errors="replace")
    if expected not in content:
        raise ExperimentFailure(f"{label} output did not contain {expected!r}")


def _png_rgba_rows(data: bytes) -> tuple[int, int, list[bytearray]]:
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        raise ExperimentFailure("CLI render output is not a PNG")
    offset = 8
    width = height = bit_depth = color_type = interlace = None
    compressed = []
    while offset + 12 <= len(data):
        size = int.from_bytes(data[offset:offset + 4], "big")
        kind = data[offset + 4:offset + 8]
        start = offset + 8
        end = start + size
        if end + 4 > len(data):
            raise ExperimentFailure("CLI render PNG has a truncated chunk")
        chunk = data[start:end]
        if kind == b"IHDR":
            if size != 13:
                raise ExperimentFailure("CLI render PNG has an invalid IHDR")
            width, height, bit_depth, color_type, _compression, _filter, interlace = struct.unpack(
                ">IIBBBBB", chunk,
            )
        elif kind == b"IDAT":
            compressed.append(chunk)
        elif kind == b"IEND":
            break
        offset = end + 4
    if not width or not height or bit_depth != 8 or color_type != 6 or interlace != 0:
        raise ExperimentFailure(
            "CLI render PNG must use noninterlaced 8-bit RGBA for pixel validation"
        )
    stride = width * 4
    try:
        raw = zlib.decompress(b"".join(compressed))
    except zlib.error as error:
        raise ExperimentFailure(f"CLI render PNG image data is invalid: {error}") from error
    if len(raw) != height * (stride + 1):
        raise ExperimentFailure("CLI render PNG decoded data has an unexpected size")
    rows = []
    previous = bytearray(stride)
    cursor = 0
    for _ in range(height):
        filter_type = raw[cursor]
        source = raw[cursor + 1:cursor + 1 + stride]
        cursor += stride + 1
        row = bytearray(stride)
        for index, value in enumerate(source):
            left = row[index - 4] if index >= 4 else 0
            above = previous[index]
            upper_left = previous[index - 4] if index >= 4 else 0
            if filter_type == 0:
                predictor = 0
            elif filter_type == 1:
                predictor = left
            elif filter_type == 2:
                predictor = above
            elif filter_type == 3:
                predictor = (left + above) // 2
            elif filter_type == 4:
                base = left + above - upper_left
                left_distance = abs(base - left)
                above_distance = abs(base - above)
                upper_left_distance = abs(base - upper_left)
                predictor = (left if left_distance <= above_distance and left_distance <= upper_left_distance
                             else above if above_distance <= upper_left_distance else upper_left)
            else:
                raise ExperimentFailure(f"CLI render PNG uses unknown filter {filter_type}")
            row[index] = (value + predictor) & 0xFF
        rows.append(row)
        previous = row
    return width, height, rows


def _assert_png(path: Path) -> dict:
    if not path.is_file():
        raise ExperimentFailure(f"CLI render did not create {path}")
    data = path.read_bytes()
    width, height, rows = _png_rgba_rows(data)
    if width != 320 or height != 180:
        raise ExperimentFailure(f"CLI render dimensions were {width}x{height}, expected 320x180")
    visible_colors = set()
    visible_pixels = 0
    for row in rows:
        for index in range(0, len(row), 4):
            pixel = tuple(row[index:index + 4])
            if pixel[3]:
                visible_pixels += 1
                visible_colors.add(pixel)
    if visible_pixels < 10 or len(visible_colors) < 2:
        raise ExperimentFailure(
            f"CLI render PNG is visually blank or uniform ({visible_pixels} visible pixels, "
            f"{len(visible_colors)} visible colors)"
        )
    return {"path": str(path), "bytes": len(data), "sha256": __import__("hashlib").sha256(data).hexdigest(),
            "width": width, "height": height, "visible_pixels": visible_pixels,
            "visible_color_count": len(visible_colors)}


def _linker_binary_evidence(platform: str, arm: dict,
                            binaries: list[tuple[Path, dict]], output: ExperimentOutput) -> dict | list[dict]:
    paths = [binary for binary, _ in binaries]
    if arm.get("audit"):
        invocation_evidence = []
        artifact_evidence = []
        for binary, info in binaries:
            audit_log = Path(info["audit_log_path"]) if info.get("audit_log_path") else None
            if not audit_log:
                raise ExperimentFailure(f"{arm['id']} {binary.name} build has no linker audit path")
            invocation_evidence.extend(collect_wrapper_evidence(
                audit_log, arm["audit"],
                [Path(info.get("link_output_path", str(binary)))],
            ))
            if platform == "linux":
                artifact_evidence.extend(collect_elf_linker_evidence(
                    [binary], arm["artifact_identity"], allow_unknown=True,
                ))
        return {"artifacts": artifact_evidence, "actual_linker_invocations": invocation_evidence}
    if platform == "linux":
        return collect_elf_linker_evidence(paths, arm["expected_linker"])
    expected_path = arm.get("expected_linker_path")
    if not expected_path:
        raise ExperimentFailure("Apple linker arm lacks its resolved ld path")
    # Apple's -v trace is recorded in the Cargo build log. This is a diagnostic
    # emitted by the invoked clang driver, not a configured-linker string.
    traces = []
    for binary, info in binaries:
        log = Path(info["build_log"])
        text = log.read_text(encoding="utf-8", errors="replace")
        matching = [line for line in text.splitlines() if expected_path in line]
        if not matching:
            raise ExperimentFailure(
                f"Apple ld path {expected_path!r} was not observed while linking {binary.name}"
            )
        traces.extend({"artifact": str(binary), "observed_driver_trace": line} for line in matching)
    return traces


def _mac_version(repo: Path) -> str:
    with (repo / "Cargo.toml").open("rb") as source:
        manifest = tomllib.load(source)
    version = manifest.get("workspace", {}).get("package", {}).get("version")
    if not isinstance(version, str):
        raise ExperimentFailure("workspace Cargo.toml has no workspace.package.version")
    return version


def _run_mac_package(output: ExperimentOutput, repo: Path, platform: str, arm: dict,
                     target: Path | None, cli: Path, gui: Path, record: dict) -> dict:
    if platform != "macos":
        return record
    package_root = output.directory / "compatibility" / platform / arm["id"] / "package"
    app = package_root / "Animatix.app"
    if package_root.exists() and any(package_root.iterdir()):
        # This is inside the explicitly-owned experiment output, never a user
        # target or the release workflow's target/native-package-macos path.
        import shutil
        shutil.rmtree(package_root)
    package_root.mkdir(parents=True, exist_ok=True)
    bin_dir = package_root.parent / "package-binaries"
    bin_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(cli, bin_dir / "animatix")
    shutil.copy2(gui, bin_dir / "animatix-gui")

    native = _run(output, repo, platform, arm, target, "native-package-check",
                  [sys.executable, "scripts/native-package-check.py", "macos", str(bin_dir)])
    icon = _run(output, repo, platform, arm, target, "package-app",
                [sys.executable, "scripts/package-macos.py", str(bin_dir / "animatix-gui"),
                 str(app), _mac_version(repo)])
    icon_check = _run(output, repo, platform, arm, target, "package-icon-check",
                      [sys.executable, "scripts/platform-icon-check.py", "macos", str(app)])
    zip_path = package_root / "Animatix-macos-experiment.zip"
    zip_script = (
        "import pathlib,sys,zipfile; app=pathlib.Path(sys.argv[1]); out=pathlib.Path(sys.argv[2]); "
        "z=zipfile.ZipFile(out,'w',zipfile.ZIP_DEFLATED); "
        "[z.write(p,p.relative_to(app.parent)) for p in app.rglob('*') if p.is_file()]; z.close()"
    )
    zipped = _run(output, repo, platform, arm, target, "package-zip",
                  [sys.executable, "-c", zip_script, str(app), str(zip_path)])
    if not zip_path.is_file() or zip_path.stat().st_size < 4096:
        raise ExperimentFailure("macOS experiment app ZIP was not produced or is implausibly small")
    record["macos_package"] = {
        "native_package_check_log": native["log_path"],
        "package_log": icon["log_path"],
        "icon_check_log": icon_check["log_path"],
        "zip_log": zipped["log_path"],
        "zip_path": str(zip_path),
        "zip_bytes": zip_path.stat().st_size,
    }
    return record


def run_linker_compatibility(output: ExperimentOutput, repo: Path, platform: str,
                             arm: dict, targets: dict) -> dict:
    """Exercise the measured CLI/GUI binaries from one linker arm."""
    if not SMOKE_SCENE.is_file():
        raise ExperimentFailure(f"compatibility smoke scene is missing: {SMOKE_SCENE}")
    cli = Path(targets["cli"]["binary_path"])
    gui = Path(targets["gui"]["binary_path"])
    if not cli.is_file() or not gui.is_file():
        raise ExperimentFailure("measured CLI/GUI workload executables are missing")

    help_run = _run(output, repo, platform, arm, None, "cli-help", [str(cli), "--help"])
    _assert_log_contains(output, help_run, "Usage:", "CLI --help")
    version_run = _run(output, repo, platform, arm, None, "cli-version", [str(cli), "--version"])
    _assert_log_contains(output, version_run, "animatix", "CLI --version")
    gui_help = _run(output, repo, platform, arm, None, "gui-help", [str(gui), "--help"])
    _assert_log_contains(output, gui_help, "Usage: animatix-gui", "GUI --help")
    gui_version = _run(output, repo, platform, arm, None, "gui-version", [str(gui), "--version"])
    _assert_log_contains(output, gui_version, "animatix-gui", "GUI --version")

    smoke_root = output.directory / "compatibility" / platform / arm["id"]
    smoke_root.mkdir(parents=True, exist_ok=True)
    png = smoke_root / "hello.png"
    render = _run(
        output, repo, platform, arm, None, "cli-image-render",
        [str(cli), "image", str(SMOKE_SCENE), "--time", "1.0", "--width", "320", "--height", "180",
         "--output", str(png)],
        timeout=1800,
    )
    image = _assert_png(png)

    if platform == "linux":
        gui_command = ["xvfb-run", "-a", sys.executable,
                       "scripts/experiments/gui_probe.py", "linux", str(gui),
                       str(SMOKE_SCENE), str(smoke_root / "gui.png"), "--timeout", "90"]
        gui_probe = _run(output, repo, platform, arm, None, "gui-launch", gui_command, timeout=150)
    elif platform == "macos":
        gui_command = [sys.executable, "scripts/experiments/gui_probe.py", "macos-bundle",
                       str(smoke_root / "package/Animatix.app"), str(SMOKE_SCENE),
                       str(smoke_root / "gui.png"), "--timeout", "90"]
        # Construct and check the application package before attempting GUI launch.
        package_record = _run_mac_package(output, repo, platform, arm, None, cli, gui, {})
        gui_probe = _run(output, repo, platform, arm, None, "gui-launch", gui_command, timeout=150)
        gui_probe["package_evidence"] = package_record.get("macos_package")
    else:
        raise ExperimentFailure(f"unsupported compatibility platform: {platform}")

    linker_evidence = _linker_binary_evidence(
        platform, arm,
        [(cli, targets["cli"]), (gui, targets["gui"])], output,
    )
    result = {
        "status": "passed",
        "platform": platform,
        "arm": arm["id"],
        "build_logs": {name: targets[name]["build_log"] for name in ("cli", "gui")},
        "binary_builds": {
            name: {
                "measured_build": targets[name].get("measured_build", False),
                "link_output_path": targets[name].get("link_output_path"),
                "staged_path": targets[name]["binary_path"],
                "sha256": targets[name].get("sha256"),
                "staging_method": targets[name].get("staging_method"),
                "audit_log_path": targets[name].get("audit_log_path"),
            }
            for name in ("cli", "gui")
        },
        "cli_path": str(cli),
        "gui_path": str(gui),
        "cli_help_log": help_run["log_path"],
        "cli_version_log": version_run["log_path"],
        "gui_help_log": gui_help["log_path"],
        "gui_version_log": gui_version["log_path"],
        "render_log": render["log_path"],
        "rendered_image": image,
        "gui_launch_log": gui_probe["log_path"],
        "gui_probe_output": Path(gui_probe["log_path"]).read_text(encoding="utf-8", errors="replace").strip(),
        "linker_evidence": linker_evidence,
        "executable_sha256": [
            {"path": str(path), "sha256": __import__("hashlib").sha256(path.read_bytes()).hexdigest()}
            for path in (cli, gui)
        ],
    }
    if platform == "macos":
        result["macos_package"] = gui_probe.get("package_evidence")
    output.append_record({"type": "compatibility-result", **result})
    output.write_json(f"compatibility/{platform}/{arm['id']}/result.json", result)
    return result
