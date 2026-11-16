#!/usr/bin/env python3
"""Validate fresh Criterion 0.5.x health samples, never historical regressions."""

import argparse
import json
import math
from pathlib import Path
import sys

EXPECTED = (
    "property_plan_lookup_and_sample",
    "timeline_evaluate_0s",
    "timeline_evaluate_1s",
    "timeline_evaluate_2s",
)


def number(value, label, *, positive=False):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        raise ValueError(f"{label}: expected finite number")
    if positive and value <= 0:
        raise ValueError(f"{label}: expected positive number")
    return value


def read_json(path):
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"missing/nonregular result: {path}")
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise ValueError(f"invalid result {path}: {error}") from error
    if not isinstance(data, dict):
        raise ValueError(f"{path}: expected JSON object")
    return data


def estimate(data, label, *, positive=False):
    if not isinstance(data, dict) or not isinstance(data.get("confidence_interval"), dict):
        raise ValueError(f"{label}: missing estimate/confidence interval")
    point = number(data.get("point_estimate"), f"{label}.point", positive=positive)
    error = number(data.get("standard_error"), f"{label}.standard_error")
    ci = data["confidence_interval"]
    lower = number(ci.get("lower_bound"), f"{label}.lower")
    upper = number(ci.get("upper_bound"), f"{label}.upper")
    confidence = number(ci.get("confidence_level"), f"{label}.confidence")
    if point < 0 or error < 0 or lower > upper or not 0 < confidence < 1:
        raise ValueError(f"{label}: invalid uncertainty bounds")
    # Bootstrap intervals need not contain their point estimate. Negative lower
    # bounds can occur for noisy slopes and retain the existing sentinel semantics.
    return {"point_ns": point, "lower_ns": lower, "upper_ns": upper,
            "confidence_level": confidence}


def validate(home, max_plan_ns=None):
    home = Path(home)
    if not home.is_dir() or home.is_symlink():
        raise ValueError("Criterion result directory is missing or not a real directory")
    expected_dirs = {home / name / "new" for name in EXPECTED}
    actual_dirs = set(home.rglob("new"))
    if actual_dirs != expected_dirs:
        missing = sorted(str(p.relative_to(home)) for p in expected_dirs - actual_dirs)
        extra = sorted(str(p.relative_to(home)) for p in actual_dirs - expected_dirs)
        raise ValueError(f"fresh sample inventory mismatch: missing={missing}, extra={extra}")
    rows = {}
    for name in EXPECTED:
        directory = home / name / "new"
        if directory.is_symlink() or directory.parent.is_symlink():
            raise ValueError(f"{name}: symlinked sample directory")
        identity = read_json(directory / "benchmark.json")
        if (identity.get("full_id") != name or identity.get("group_id") != name
                or identity.get("directory_name") != name
                or identity.get("function_id") is not None
                or identity.get("value_str") is not None):
            raise ValueError(f"{name}: benchmark identity mismatch")
        sample = read_json(directory / "sample.json")
        iters, times = sample.get("iters"), sample.get("times")
        if (not isinstance(sample.get("sampling_mode"), str)
                or sample["sampling_mode"] not in {"Linear", "Flat"}
                or not isinstance(iters, list) or not isinstance(times, list)
                or len(iters) < 2 or len(iters) != len(times)):
            raise ValueError(f"{name}: invalid sample arrays/mode")
        for index, (iterations, elapsed) in enumerate(zip(iters, times)):
            iterations = number(iterations, f"{name}.iters[{index}]", positive=True)
            number(elapsed, f"{name}.times[{index}]", positive=True)
            if iterations != int(iterations):
                raise ValueError(f"{name}: non-integral iteration count")
        estimates = read_json(directory / "estimates.json")
        for statistic in ("mean", "median", "median_abs_dev", "std_dev"):
            estimate(estimates.get(statistic), f"{name}.{statistic}",
                     positive=statistic in {"mean", "median"})
        if "slope" not in estimates or ((estimates["slope"] is not None)
                                        != (sample["sampling_mode"] == "Linear")):
            raise ValueError(f"{name}: slope/sampling mode mismatch")
        # Match Criterion 0.5.1 Estimates::typical() and the CLI's first time value.
        statistic = "slope" if estimates.get("slope") is not None else "mean"
        row = estimate(estimates.get(statistic), f"{name}.{statistic}", positive=True)
        row.update({"statistic": statistic, "sample_count": len(iters)})
        rows[name] = row
    threshold = None
    passed = True
    if max_plan_ns is not None:
        limit = number(max_plan_ns, "max_plan_ns")
        if limit < 0:
            raise ValueError("max_plan_ns must be nonnegative")
        observed = rows[EXPECTED[0]]["lower_ns"]
        passed = observed <= limit
        threshold = {"metric": "typical.confidence_interval.lower_bound", "limit_ns": limit,
                     "observed_ns": observed}
    return {"kind": "fresh_health_samples", "historical_comparison": False,
            "passed": passed, "rows": rows, "property_plan_threshold": threshold}


def stage_evidence(run_dir, exit_code):
    """Only this invocation's allowlisted files; at most 16 x 64 KiB + manifest."""
    run_dir = Path(run_dir)
    evidence = run_dir / "evidence"
    evidence.mkdir()  # Fresh, never reuse a cached directory.
    allowed = [Path(name) for name in ("build.log", "property-plan.log", "timeline.log", "summary.json")]
    allowed += [Path("criterion") / name / "new" / filename
                for name in EXPECTED for filename in ("benchmark.json", "sample.json", "estimates.json")]
    records = []
    cap = 64 * 1024
    for relative in allowed:
        source = run_dir / relative
        if not source.exists():
            continue
        if source.is_symlink() or any(p.is_symlink() for p in source.parents if p != run_dir.parent):
            records.append({"path": str(relative), "skipped": "symlink"})
            continue
        if not source.is_file():
            records.append({"path": str(relative), "skipped": "not a regular file"})
            continue
        size = source.stat().st_size
        with source.open("rb") as stream:
            if relative.suffix == ".log" and size > cap:
                stream.seek(size - cap)  # Last diagnostics, bounded read.
            data = stream.read(cap)
        truncated = size > cap
        output = relative if not truncated or relative.suffix == ".log" else Path(str(relative) + ".truncated.txt")
        destination = evidence / output
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
        records.append({"path": str(output), "bytes": len(data), "original_bytes": size,
                        "truncated": truncated})
    (evidence / "evidence.json").write_text(json.dumps({
        "exit_code": exit_code, "historical_comparison": False,
        "per_file_limit_bytes": cap, "files": records,
    }, indent=2) + "\n")
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("home", type=Path)
    parser.add_argument("--max-plan-ns", type=float)
    parser.add_argument("--stage-evidence", type=int, metavar="EXIT_CODE")
    args = parser.parse_args()
    try:
        if args.stage_evidence is not None:
            print(f"extension-bench evidence: {stage_evidence(args.home, args.stage_evidence)}")
            return 0
        result = validate(args.home, args.max_plan_ns)
    except (ValueError, OSError) as error:
        print(json.dumps({"kind": "fresh_health_samples", "passed": False,
                          "historical_comparison": False, "error": str(error)}, indent=2))
        print(f"extension-bench: invalid fresh samples: {error}", file=sys.stderr)
        return 2
    print(json.dumps(result, indent=2, sort_keys=True, allow_nan=False))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
