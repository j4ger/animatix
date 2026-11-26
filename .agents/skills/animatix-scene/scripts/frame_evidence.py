#!/usr/bin/env python3
"""Plan exact frame times, capture a render receipt, or check that it is current."""
import argparse
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

MAX_FRAMES = 600


def read(path):
    return json.loads(path.read_text(encoding="utf-8"))


def write(path, data):
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def sha(path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1048576), b""):
            h.update(block)
    return h.hexdigest()


def samples(plan):
    spec = json.loads(plan)
    if not isinstance(spec["fps"], str):
        raise ValueError("fps must be a string to preserve its exact value")
    fps = Fraction(spec["fps"])
    if fps <= 0:
        raise ValueError("fps must be a positive string, e.g. '24' or '30000/1001'")
    frames = set()
    for rule in spec["checks"]:
        if not rule["expect"].strip():
            raise ValueError("Each check needs an expected visible state")
        for token in rule["frames"].split(","):
            m = re.fullmatch(r"(\d+)(?:-(\d+))?", token.strip())
            if not m:
                raise ValueError(f"Invalid zero-based frame or inclusive range: {token}")
            lo, hi = int(m[1]), int(m[2] or m[1])
            if hi < lo or hi - lo >= MAX_FRAMES:
                raise ValueError(f"Invalid or oversized range: {token}")
            frames.update(range(lo, hi + 1))
            if len(frames) > MAX_FRAMES:
                raise ValueError(f"Select at most {MAX_FRAMES} frames in one run")
    if not frames:
        raise ValueError("No checks supplied")
    rows = []
    for frame in sorted(frames):
        t = Fraction(frame) / fps
        rows.append(dict(frame=frame, exact_seconds=str(t), runner_seconds=float(t), file=f"frame-{frame:06}.png"))
    if any(a["runner_seconds"] >= b["runner_seconds"] for a, b in zip(rows, rows[1:])):
        raise ValueError("Renderer numbers cannot distinguish the requested frames")
    return rows


def input_hashes(source, inputs):
    source = source.resolve(strict=True)
    if any(Path(name).is_absolute() for name in inputs):
        raise ValueError("Additional inputs must be relative to the scene")
    return {
        "source": sha(source),
        "additional": {name: sha(source.parent / name) for name in inputs},
    }


def capture(a):
    locked = a.expectations.read_bytes()
    rows = samples(locked)
    before = input_hashes(a.source, a.input)
    a.output.mkdir()
    frozen = a.output / "expected.json"
    frozen.write_bytes(locked)
    times = [r["runner_seconds"] for r in rows]
    subprocess.run(
        [str(a.runner.resolve(strict=True)), str(a.source.resolve(strict=True)), str(a.output.resolve()),
         ",".join(map(repr, times))],
        check=True,
    )
    report = read(a.output / "samples.json")
    if report["times"] != times or report["result"]["ok"] is not True:
        raise ValueError("Runner did not confirm a successful render at the requested times")
    if before != input_hashes(a.source, a.input) or locked != a.expectations.read_bytes():
        raise ValueError("Inputs or expectations changed during capture; discard this run")
    for i, row in enumerate(rows):
        raw = a.output / f"f{i:03}.png"
        with raw.open("rb") as f:
            if f.read(8) != b"\x89PNG\r\n\x1a\n":
                raise ValueError(f"Runner output is not PNG: {raw.name}")
        img = raw.rename(a.output / row["file"])
        row["sha256"] = sha(img)
    write(a.output / "receipt.json", dict(
        schema=1, inputs=before, expectations_sha256=sha(frozen),
        runner_report_sha256=sha(a.output / "samples.json"), samples=rows,
    ))
    print(f"Captured {len(rows)} PNG samples; inspect the actual visible claims.")


def verify(a):
    receipt = read(a.output / "receipt.json")
    frozen = a.output / "expected.json"
    if receipt["schema"] != 1 or sha(frozen) != receipt["expectations_sha256"]:
        raise ValueError("Expectations or receipt format changed")
    if sha(a.output / "samples.json") != receipt["runner_report_sha256"]:
        raise ValueError("Runner report changed")
    if input_hashes(a.source, receipt["inputs"]["additional"]) != receipt["inputs"]:
        raise ValueError("Scene or listed dependency changed after capture")
    rows = samples(frozen.read_bytes())
    saved = receipt["samples"]
    if len(rows) != len(saved):
        raise ValueError("Receipt sample count changed")
    for requested, observed in zip(rows, saved):
        if any(observed.get(k) != v for k, v in requested.items()):
            raise ValueError("Receipt doesn't map to the locked frame numbers")
        if sha(a.output / requested["file"]) != observed["sha256"]:
            raise ValueError(f"Image changed: {requested['file']}")
    print(f"Verified {len(rows)} current samples and their receipt; no visual verdict inferred.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_subparsers(dest="mode", required=True)
    plan = modes.add_parser("plan", help="print frame-to-seconds mapping")
    plan.add_argument("expectations", type=Path)
    cap = modes.add_parser("capture", help="run a compatible renderer and write a receipt")
    cap.add_argument("expectations", type=Path)
    cap.add_argument("source", type=Path)
    cap.add_argument("output", type=Path, help="a new directory")
    cap.add_argument("--runner", type=Path, required=True)
    cap.add_argument("--input", action="append", default=[], help="additional input path relative to source (repeatable)")
    check = modes.add_parser("verify", help="check source, listed dependencies, and saved images")
    check.add_argument("source", type=Path)
    check.add_argument("output", type=Path)
    a = parser.parse_args()
    try:
        if a.mode == "plan":
            print(json.dumps(samples(a.expectations.read_bytes()), indent=2))
        elif a.mode == "capture":
            capture(a)
        else:
            verify(a)
    except (OSError, ValueError, KeyError, TypeError, AttributeError, ZeroDivisionError, OverflowError,
            subprocess.CalledProcessError) as e:
        print(f"Frame evidence: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
