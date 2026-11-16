#!/usr/bin/env python3
"""Manual native renderer measurement; no CI performance acceptance gate."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import uuid

ROOT = Path(__file__).resolve().parents[3]
INPUTS = ["Cargo.lock", "Cargo.toml", "flake.lock", "rust-toolchain.toml",
          "crates/animatix-render/Cargo.toml",
          "crates/animatix-render/examples/offscreen_blur_area.rs",
          "crates/animatix-render/examples/offscreen_blur_area/workload.rs",
          "crates/animatix-render/tools/offscreen-blur-area.py",
          "crates/animatix-render/examples/offscreen_blur_area/manifest.json"]


def bounded(command, timeout, stdout, stderr):
    # Separate process group means a stuck poll/map and Cargo descendants are
    # terminated together. Never impose this policy on normal renderer calls.
    with open(stdout, "xb") as out, open(stderr, "xb") as err:
        process = subprocess.Popen(command, cwd=ROOT, stdout=out, stderr=err, start_new_session=True)
        try:
            return process.wait(timeout=timeout)
        except BaseException:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
            raise


def require(condition, message):
    if not condition:
        raise ValueError(message)


def validate(result, frames, allow_software):
    require(result["schema"] == 1 and result["warmup"] == 4, "wrong version/warmup")
    require(result["columns"] == ["host_evaluate_encode_submit_ns", "completion_map_cpu_copy_ns", "total_completed_frame_ns"], "wrong timing columns")
    require(result["workload"] == "offscreen_blur_area_v1", "wrong workload")
    require((result["width"], result["height"], result["queue_depth"], result["radius"]) == (512, 512, 1, 6), "wrong configuration")
    kind = result["adapter"]["device_type"]
    expected_class = {"Cpu": "software", "IntegratedGpu": "hardware_reported", "DiscreteGpu": "hardware_reported", "VirtualGpu": "virtual_unqualified"}.get(kind, "unknown_unqualified")
    require(result["class"] == expected_class, "device class mismatch")
    require(result["class"] != "software" or allow_software, "software requires opt-in")
    require(len(result["cases"]) == 6, "missing cases")
    require({(c["size"], c["mode"]) for c in result["cases"]} == {(s, m) for s in (64, 128, 256) for m in ("plain", "blur")}, "wrong cases")
    manifest = json.loads((ROOT / "crates/animatix-render/examples/offscreen_blur_area/manifest.json").read_text())
    hashes = {(c["size"], c["mode"]): c["source_sha256"] for c in manifest["scenes"]}
    for case in result["cases"]:
        require(len(case["samples_ns"]) == frames, "missing samples")
        for row in case["samples_ns"]:
            require(len(row) == 3 and all(type(v) is int and v >= 0 for v in row), "invalid duration")
            require(row[0] + row[1] == row[2] and row[2] > 0, "inconsistent duration")
        case["source_sha256"] = hashlib.sha256(case["source"].encode()).hexdigest()
        require(case["source_sha256"] == hashes[(case["size"], case["mode"])], "fixture drift")
    require(bool(kind) and bool(result["adapter"]["backend"]), "missing adapter")


def main():
    def interrupted(signum, _frame):
        raise KeyboardInterrupt(f"signal {signum}")
    signal.signal(signal.SIGTERM, interrupted)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--frames", type=int, default=30, choices=range(1, 121), metavar="1..120")
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("--allow-software", action="store_true")
    parser.add_argument("--output-root", type=Path, default=ROOT / "target/perf/offscreen-blur-area-v1")
    args = parser.parse_args()
    if not 1 <= args.timeout <= 1800:
        parser.error("timeout must be 1..1800 seconds")
    run = args.output_root.resolve() / (datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ") + "-" + uuid.uuid4().hex)
    run.mkdir(parents=True, exist_ok=False)
    tracked = subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT).decode().split("\0")
    # Relevant complete source trees, including dirty bytes. No credentials,
    # full environment dump, usernames or arbitrary absolute path hashes.
    files = set(INPUTS) | {p for p in tracked if p.startswith(("crates/animatix/", "crates/animatix-core/", "crates/animatix-std/", "crates/animatix-syntax/", "crates/animatix-render/", "crates/vendor/"))}
    for path in INPUTS:
        if not (ROOT / path).is_file():
            raise RuntimeError(f"required provenance input missing: {path}")
    metadata = {"schema": 1, "workload": "offscreen_blur_area_v1", "status": "pending", "frames": args.frames,
                "allow_software": args.allow_software, "os": platform.system(), "arch": platform.machine(),
                "kernel": platform.release(), "logical_cpus": os.cpu_count(), "load": list(os.getloadavg()),
                "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                "dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT)),
                "inputs_sha256": {p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest() for p in sorted(files) if (ROOT / p).is_file()},
                "environment_value_sha256": {k: hashlib.sha256(os.environ[k].encode()).hexdigest() for k in ("WGPU_BACKEND", "WGPU_POWER_PREF", "RUSTFLAGS", "CARGO_BUILD_TARGET") if k in os.environ},
                "rustc": subprocess.check_output(["rustc", "--version", "--verbose"], cwd=ROOT, text=True).strip(),
                "historical_comparison": False, "performance_threshold": None}
    def save():
        (run / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    save()
    try:
        command = ["cargo", "build", "--release", "--locked", "-p", "animatix-render", "--example", "offscreen_blur_area", "--message-format=json"]
        metadata["build_command"] = command
        if bounded(command, 1800, run / "build.jsonl", run / "build.stderr"):
            raise RuntimeError("build failed")
        binaries = [x["executable"] for line in (run / "build.jsonl").read_text().splitlines() if (x := json.loads(line)).get("reason") == "compiler-artifact" and x.get("target", {}).get("name") == "offscreen_blur_area" and x.get("executable")]
        if len(set(binaries)) != 1:
            raise RuntimeError("expected exactly one compiled example executable")
        command = [binaries[0], str(run / "raw.json"), str(args.frames)]
        if args.allow_software:
            command.append("--allow-software")
        if bounded(command, args.timeout, run / "run.stdout", run / "run.stderr"):
            raise RuntimeError("driver failed (including adapter policy or correctness guard)")
        result = json.loads((run / "raw.json").read_text())
        validate(result, args.frames, args.allow_software)
        with (run / "results.json").open("x") as out:
            json.dump(result, out, indent=2)
        metadata["status"] = "passed_correctness_and_collected_samples"
    except BaseException as error:
        metadata["status"] = "interrupted" if isinstance(error, KeyboardInterrupt) else "failed"
        metadata["error"] = str(error)
        raise
    finally:
        save()
        print(run)


if __name__ == "__main__":
    main()
