#!/usr/bin/env python3
"""Classify Criterion comparisons by statistical signal and workload impact."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import sys
from pathlib import Path


def load_policy(path: Path) -> dict:
    try:
        policy = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        raise ValueError(f"cannot load policy {path}: {exc}") from exc
    if policy.get("version") != 1 or policy.get("unknown_policy") != "fail":
        raise ValueError("unsupported policy version or unknown-row behavior")
    budgets = policy.get("budgets_ns")
    if not isinstance(budgets, dict) or any(
        not isinstance(value, (int, float)) or not math.isfinite(value) or value <= 0
        for value in budgets.values()
    ):
        raise ValueError("policy budgets must be finite positive numbers")
    for rule in policy.get("rules", []):
        try:
            re.compile(rule["pattern"])
        except (KeyError, re.error) as exc:
            raise ValueError(f"invalid policy rule {rule!r}: {exc}") from exc
    return policy


def role_for(policy: dict, bench_id: str) -> dict:
    matches = [
        rule for rule in policy["rules"] if re.fullmatch(rule["pattern"], bench_id)
    ]
    if len(matches) != 1:
        raise ValueError(
            f"benchmark {bench_id!r} has {len(matches)} policy matches; expected exactly one"
        )
    return matches[0]


def read_estimate(path: Path) -> tuple[float, float, float, float]:
    try:
        estimate = json.loads(path.read_text())
        mean = float(estimate["mean"]["point_estimate"])
        interval = estimate["mean"]["confidence_interval"]
        confidence_level = float(interval["confidence_level"])
        lower = float(interval["lower_bound"])
        upper = float(interval["upper_bound"])
    except (OSError, json.JSONDecodeError, KeyError, TypeError, ValueError) as exc:
        raise ValueError(f"invalid Criterion estimate {path}: {exc}") from exc
    if not math.isfinite(mean) or mean <= 0:
        raise ValueError(f"estimate {path} has a nonpositive or nonfinite mean")
    if not math.isfinite(lower) or lower <= 0 or not math.isfinite(upper) or upper < lower:
        raise ValueError(f"estimate {path} has an invalid mean confidence interval")
    if not math.isfinite(confidence_level) or not 0 < confidence_level < 1:
        raise ValueError(f"estimate {path} has an invalid confidence level")
    if not lower <= mean <= upper:
        raise ValueError(f"estimate {path} mean is outside its confidence interval")
    return mean, lower, upper, confidence_level


def classify(policy: dict, bench_id: str, baseline: Path, current: Path,
             threshold_pct: float) -> dict:
    if not math.isfinite(threshold_pct) or threshold_pct < 0:
        raise ValueError("percentage threshold must be finite and nonnegative")
    rule = role_for(policy, bench_id)
    base_mean, base_lower, base_upper, base_level = read_estimate(baseline)
    current_mean, current_lower, current_upper, current_level = read_estimate(current)
    if base_level != current_level:
        raise ValueError(
            f"Criterion confidence levels differ: baseline {base_level}, current {current_level}"
        )
    delta = current_mean - base_mean
    change_pct = delta / base_mean * 100.0
    ratio_lower_pct = (current_lower / base_upper - 1.0) * 100.0
    ratio_upper_pct = (current_upper / base_lower - 1.0) * 100.0
    delta_lower = current_lower - base_upper
    delta_upper = current_upper - base_lower
    if ratio_lower_pct > threshold_pct:
        statistical_status = "CLEAR_SIGNAL"
    elif ratio_upper_pct <= threshold_pct:
        statistical_status = "NO_SIGNAL"
    else:
        statistical_status = "INCONCLUSIVE"

    role = rule["role"]
    if role == "sentinel":
        budget = float(rule["absolute_limit_ns"])
        impact = delta
        verdict = "BLOCK" if current_mean > budget else (
            "INFO" if statistical_status == "CLEAR_SIGNAL" and delta > 0 else "OK"
        )
    elif role == "signal_only":
        budget = None
        impact = None
        verdict = "INFO" if statistical_status != "NO_SIGNAL" and delta > 0 else "OK"
    elif role in ("action", "frame", "frame_batch"):
        frames = 1
        if role == "frame_batch":
            frames = rule.get("frames")
            if not isinstance(frames, int) or frames <= 0:
                raise ValueError(f"frame_batch rule for {bench_id!r} needs positive frames")
        budget = float(policy["budgets_ns"]["action"])
        if role in ("frame", "frame_batch"):
            budget = float(policy["budgets_ns"]["frame"])
        impact = delta / frames
        impact_lower = delta_lower / frames
        impact_upper = delta_upper / frames
        if statistical_status == "CLEAR_SIGNAL" and impact_lower >= budget:
            verdict = "BLOCK"
        elif ratio_upper_pct <= threshold_pct or impact_upper < budget:
            verdict = "OK"
        else:
            verdict = "INCONCLUSIVE"
    else:
        raise ValueError(f"unsupported role {role!r} for {bench_id!r}")

    return {
        "id": bench_id,
        "baseline_ns": base_mean,
        "current_ns": current_mean,
        "delta_ns": delta,
        "change_pct": change_pct,
        "ratio_lower_pct": ratio_lower_pct,
        "ratio_upper_pct": ratio_upper_pct,
        "delta_lower_ns": delta_lower,
        "delta_upper_ns": delta_upper,
        "statistical_status": statistical_status,
        "role": role,
        "impact_ns": impact,
        "impact_lower_ns": None if impact is None else (impact_lower if role in ("action", "frame", "frame_batch") else impact),
        "impact_upper_ns": None if impact is None else (impact_upper if role in ("action", "frame", "frame_batch") else impact),
        "budget_ns": budget,
        "verdict": verdict,
    }


def estimate_files(directory: Path) -> list[Path]:
    return sorted(path for path in directory.glob("*.json") if path.name != "META.json")


def validate_dirs(policy: dict, baseline_dir: Path, current_dir: Path) -> None:
    for directory in (baseline_dir, current_dir):
        if not directory.is_dir():
            raise ValueError(f"estimate directory does not exist: {directory}")
        files = estimate_files(directory)
        if not files:
            raise ValueError(f"no estimate JSON files in {directory}")
        for path in files:
            bench_id = path.stem
            role_for(policy, bench_id)
            read_estimate(path)
    baseline_ids = {path.stem for path in estimate_files(baseline_dir)}
    current_ids = {path.stem for path in estimate_files(current_dir)}
    missing = sorted(baseline_ids - current_ids)
    unexpected = sorted(current_ids - baseline_ids)
    if missing:
        preview = ", ".join(missing[:8])
        suffix = " …" if len(missing) > 8 else ""
        raise ValueError(f"current estimates are missing {len(missing)} baseline row(s): {preview}{suffix}")
    if unexpected:
        preview = ", ".join(unexpected[:8])
        suffix = " …" if len(unexpected) > 8 else ""
        raise ValueError(f"current estimates contain {len(unexpected)} row(s) without a baseline: {preview}{suffix}")


def evaluate_dirs(policy: dict, baseline_dir: Path, current_dir: Path,
                  threshold_pct: float, min_coverage: int) -> dict:
    validate_dirs(policy, baseline_dir, current_dir)
    baseline_files = {path.stem: path for path in estimate_files(baseline_dir)}
    current_files = estimate_files(current_dir)
    results = []
    for current in current_files:
        base = baseline_files.get(current.stem)
        if base is None:
            raise ValueError(f"current benchmark {current.stem!r} has no baseline")
        results.append(classify(policy, current.stem, base, current, threshold_pct))
    coverage = len(results) * 100 // len(baseline_files)
    if coverage < min_coverage:
        raise ValueError(
            f"baseline coverage {coverage}% is below required {min_coverage}%"
        )
    return {
        "baseline_count": len(baseline_files),
        "compared_count": len(results),
        "coverage_pct": coverage,
        "material_regressions": sum(item["verdict"] == "BLOCK" for item in results),
        "inconclusive": sum(item["verdict"] == "INCONCLUSIVE" for item in results),
        "policy": str(policy.get("source", "scripts/perf-policy.json")),
        "results": results,
    }


def validate_report_path(report_path: Path, *estimate_dirs: Path) -> None:
    report_resolved = report_path.resolve()
    for directory in estimate_dirs:
        if not directory.is_dir():
            raise ValueError(f"estimate directory does not exist: {directory}")
        for estimate in directory.glob("*.json"):
            aliases_existing = report_path.exists() and os.path.samefile(report_path, estimate)
            if report_resolved == estimate.resolve() or aliases_existing:
                raise ValueError(
                    f"policy report path aliases raw estimate input {estimate}; choose a separate file"
                )


def table_line(result: dict) -> str:
    def ns(value):
        return "-" if value is None else f"{value:.1f}"

    def pct(value):
        return f"{value:.2f}%"

    impact = "-" if result["impact_lower_ns"] is None else (
        f"[{ns(result['impact_lower_ns'])},{ns(result['impact_upper_ns'])}]"
    )
    budget = ns(result["budget_ns"])
    ratio = f"[{pct(result['ratio_lower_pct'])},{pct(result['ratio_upper_pct'])}]"
    delta_ci = f"[{ns(result['delta_lower_ns'])},{ns(result['delta_upper_ns'])}]"
    return (
        f"{result['id']:<46} {result['baseline_ns']:>12.1f} {result['current_ns']:>12.1f} "
        f"{result['delta_ns']:>11.1f} {result['change_pct']:>8.2f}% {ratio:>19} "
        f"{delta_ci:>21} {result['role']:<14} {result['statistical_status']:<12} "
        f"{impact:>21} {budget:>12} {result['verdict']}"
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    validate = subparsers.add_parser("validate")
    validate.add_argument("--policy", type=Path, required=True)
    validate.add_argument("--baseline-dir", type=Path, required=True)
    validate.add_argument("--current-dir", type=Path, required=True)
    evaluate = subparsers.add_parser("evaluate")
    evaluate.add_argument("--policy", type=Path, required=True)
    evaluate.add_argument("--baseline-dir", type=Path, required=True)
    evaluate.add_argument("--current-dir", type=Path, required=True)
    evaluate.add_argument("--thresh", type=float, default=5.0)
    evaluate.add_argument("--format", choices=("table", "json"), default="table")
    report_check = subparsers.add_parser("validate-report")
    report_check.add_argument("--report", type=Path, required=True)
    report_check.add_argument("--baseline-dir", type=Path, required=True)
    report_check.add_argument("--current-dir", type=Path, required=True)
    compare = subparsers.add_parser("classify")
    compare.add_argument("--policy", type=Path, required=True)
    compare.add_argument("--id", required=True)
    compare.add_argument("--baseline", type=Path, required=True)
    compare.add_argument("--current", type=Path, required=True)
    compare.add_argument("--thresh", type=float, required=True)
    args = parser.parse_args()
    try:
        if args.command == "validate-report":
            validate_report_path(args.report, args.baseline_dir, args.current_dir)
            return 0
        policy = load_policy(args.policy)
        if args.command == "validate":
            validate_dirs(policy, args.baseline_dir, args.current_dir)
            return 0
        if args.command == "evaluate":
            result = evaluate_dirs(
                policy, args.baseline_dir, args.current_dir, args.thresh, 100
            )
            result["policy"] = str(args.policy)
            result["policy_version"] = policy["version"]
            result["policy_sha256"] = hashlib.sha256(args.policy.read_bytes()).hexdigest()
            result["interval_scope"] = (
                "Criterion per-run mean confidence-interval screen; excludes cross-process/session variance"
            )
            if args.format == "json":
                print(json.dumps(result, indent=2, sort_keys=True))
            else:
                print(
                    f"Policy v{result['policy_version']} SHA256 {result['policy_sha256']}\n"
                    f"Interval scope: {result['interval_scope']}"
                )
                print(
                    f"{'bench':<46} {'baseline(ns)':>12} {'current(ns)':>12} {'delta(ns)':>11} "
                    f"{'mean%':>9} {'ratio-CI(%)':>19} {'delta-CI(ns)':>21} {'role':<14} "
                    f"{'screen':<12} {'impact-CI(ns)':>21} {'budget(ns)':>12} verdict"
                )
                for item in result["results"]:
                    print(table_line(item))
                print(
                    f"{result['compared_count']}/{result['baseline_count']} rows; "
                    f"{result['coverage_pct']}% coverage; "
                    f"{result['material_regressions']} material regressions; "
                    f"{result['inconclusive']} inconclusive"
                )
            if result["material_regressions"]:
                return 1
            if result["inconclusive"]:
                return 2
            return 0
        result = classify(policy, args.id, args.baseline, args.current, args.thresh)
    except ValueError as exc:
        print(f"perf-policy: {exc}", file=sys.stderr)
        return 2
    fields = (
        result["baseline_ns"], result["current_ns"], result["delta_ns"],
        result["change_pct"], result["ratio_lower_pct"], result["ratio_upper_pct"],
        result["delta_lower_ns"], result["delta_upper_ns"], result["statistical_status"], result["role"],
        "-" if result["impact_lower_ns"] is None else result["impact_lower_ns"],
        "-" if result["impact_upper_ns"] is None else result["impact_upper_ns"],
        "-" if result["budget_ns"] is None else result["budget_ns"],
        result["verdict"],
    )
    print(" ".join(str(value) for value in fields))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
