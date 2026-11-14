#!/usr/bin/env python3
"""Summarize real GitHub Actions jobs JSON for CI topology experiments."""

from __future__ import annotations

import argparse
from datetime import datetime
import json
from pathlib import Path
import statistics
import sys

FAILURES = {"failure", "timed_out", "cancelled", "startup_failure", "action_required"}
WORKFLOW_PREFIXES = {
    "baseline": "ci_baseline / ",
    "grouped": "ci_grouped / ",
    "shared-cli": "ci_shared_cli / ",
}
SETUP_PREFIXES = (
    "set up job", "run actions/checkout", "free disk space", "install nix",
    "set up cachix", "rust (", "rust cache", "provision", "download artifact",
)


def instant(value):
    if not value:
        return None
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


def elapsed(start, end):
    begin, finish = instant(start), instant(end)
    return max(0.0, (finish - begin).total_seconds()) if begin and finish else None


def read_jobs(path):
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    jobs = data.get("jobs") if isinstance(data, dict) else data
    if not isinstance(jobs, list) or not jobs or any(not isinstance(job, dict) for job in jobs):
        raise ValueError(f"{path}: expected GitHub Actions jobs JSON with a non-empty jobs array")
    return jobs


def _topology_name(job):
    return job.get("_topology_name", job.get("name", ""))


def _name_key(job):
    name = _topology_name(job).lower().replace("_", " ").replace("-", " ")
    return " ".join(name.split())


def select_topology_jobs(jobs, topology):
    """Keep only the selected reusable workflow subtree in a Nightly run."""
    expected = WORKFLOW_PREFIXES[topology]
    prefixes_present = [prefix for prefix in WORKFLOW_PREFIXES.values()
                        if any(job.get("name", "").startswith(prefix) for job in jobs)]
    if not prefixes_present:
        return jobs
    selected = [dict(job, _topology_name=job["name"][len(expected):])
                for job in jobs if job.get("name", "").startswith(expected)]
    if not selected:
        raise ValueError(f"jobs JSON has reusable workflow prefixes but no {expected} subtree")
    return selected


def _roles(jobs):
    roles = {}
    for job in jobs:
        name = _topology_name(job)
        lowered = name.lower()
        role = ("plan" if lowered == "plan" else
                "required" if lowered in {"required", "ci required"} else
                "producer" if "producer" in lowered else
                "consumer" if "consumer" in lowered else "matrix")
        roles[job["id"]] = role
    return roles


def _critical_path(jobs):
    """Infer the active dependency path without counting pre-dependency queue time twice."""
    roles = _roles(jobs)
    by_id = {job["id"]: job for job in jobs}
    plan = next((key for key, role in roles.items() if role == "plan"), None)
    producers = [key for key, role in roles.items() if role == "producer"]
    parents = {}
    for key, role in roles.items():
        if role == "plan":
            parents[key] = []
        elif role == "required":
            parents[key] = [other for other, kind in roles.items() if kind != "required"]
        else:
            parents[key] = [plan] if plan is not None else []
            if role == "consumer":
                parents[key].extend(producers)
    memo = {}
    def path_to(key, visiting=frozenset()):
        if key in memo:
            return memo[key]
        if key in visiting:
            raise ValueError("cycle in inferred workflow dependency graph")
        created = instant(by_id[key].get("created_at") or by_id[key].get("started_at"))
        completed = instant(by_id[key].get("completed_at"))
        parent_finishes = [
            instant(by_id[parent].get("completed_at")) for parent in parents[key]
        ]
        if created is None or completed is None or any(value is None for value in parent_finishes):
            return None
        ready = max([created, *parent_finishes])
        duration = max(0.0, (completed - ready).total_seconds())
        candidates = [path_to(parent, visiting | {key}) for parent in parents[key]]
        candidates = [candidate for candidate in candidates if candidate is not None]
        best = max(candidates, key=lambda candidate: candidate[0]) if candidates else (0.0, [])
        memo[key] = (best[0] + duration, best[1] + [by_id[key].get("name", str(key))])
        return memo[key]
    paths = [path_to(key) for key in by_id]
    paths = [path for path in paths if path is not None]
    return max(paths, key=lambda path: path[0]) if paths else (None, [])


def _observed_critical_path(jobs):
    """Measure the actual pipeline boundary from Plan creation to CI Required completion."""
    plan = next((job for job in jobs if _topology_name(job).lower() == "plan"), None)
    required = next((job for job in jobs
                     if _topology_name(job).lower() in {"required", "ci required"}), None)
    if plan is None or required is None:
        return None, []
    seconds = elapsed(
        plan.get("created_at") or plan.get("started_at"),
        required.get("completed_at"),
    )
    if seconds is None:
        return None, []
    return seconds, [plan.get("name", "Plan"), required.get("name", "CI Required")]


def analyze_jobs(jobs, topology, source=""):
    if topology not in WORKFLOW_PREFIXES:
        raise ValueError(f"unknown topology: {topology}")
    jobs = select_topology_jobs(jobs, topology)
    unselected_shared_cli = []
    if topology != "shared-cli":
        optional_names = {"experimental cli producer", "experimental shared cli consumer"}
        unselected_shared_cli = [job for job in jobs if _name_key(job) in optional_names]
        jobs = [job for job in jobs if _name_key(job) not in optional_names]
    ids = [job.get("id") for job in jobs]
    if any(identifier is None for identifier in ids) or len(ids) != len(set(ids)):
        raise ValueError("jobs must have unique GitHub job ids")
    names = [_topology_name(job) for job in jobs]
    lowered = [name.lower() for name in names]
    issues = []
    unexpected_unselected = any(
        job.get("conclusion") != "skipped" for job in unselected_shared_cli
    )
    if unexpected_unselected:
        issues.append("unselected shared CLI producer/consumer jobs did not skip")
    if not any(name == "plan" for name in lowered):
        issues.append("Plan job missing")
    if not any(name in {"required", "ci required"} for name in lowered):
        issues.append("CI Required job missing")
    if topology == "grouped" and not any("grouped" in name for name in lowered):
        issues.append("grouped native job missing")
    if topology == "shared-cli":
        producers = [job for job, name in zip(jobs, lowered) if "producer" in name]
        consumers = [job for job, name in zip(jobs, lowered) if "consumer" in name]
        if not producers or not consumers:
            issues.append("shared CLI producer/consumer job missing")
        if any(job.get("conclusion") != "success" for job in producers):
            issues.append("shared CLI producer job did not succeed")
        if any(job.get("conclusion") != "success" for job in consumers):
            issues.append("shared CLI consumer job did not succeed")

    run_ids = {job.get("run_id") for job in jobs if job.get("run_id") is not None}
    source_shas = {job.get("head_sha") for job in jobs if job.get("head_sha")}
    if len(run_ids) > 1:
        issues.append("jobs JSON contains multiple workflow runs")
    if len(source_shas) != 1:
        issues.append("source commit is missing or inconsistent")

    created = [job.get("created_at") or job.get("started_at") for job in jobs]
    completed = [job.get("completed_at") for job in jobs]
    wall = None
    if all(created) and all(completed):
        wall = elapsed(min(created), max(completed))
    else:
        issues.append("one or more jobs have no completion timestamp")
    runner_seconds = [elapsed(job.get("started_at"), job.get("completed_at")) for job in jobs]
    runner_minutes = sum(runner_seconds) / 60 if all(value is not None for value in runner_seconds) else None
    if runner_minutes is None:
        issues.append("runner duration is incomplete")

    step_seconds = {}
    setup_seconds = build_seconds = gate_seconds = 0.0
    for job in jobs:
        for step in job.get("steps", []):
            duration = elapsed(step.get("started_at"), step.get("completed_at"))
            if duration is None:
                continue
            name = step.get("name", "")
            key = name.lower()
            step_seconds[name] = step_seconds.get(name, 0.0) + duration
            if key.startswith(SETUP_PREFIXES):
                setup_seconds += duration
            if key == "gate" or key.startswith("verify and consume"):
                gate_seconds += duration
            if "build" in key:
                build_seconds += duration

    failure_jobs = [job for job in jobs if job.get("conclusion") in FAILURES and job.get("completed_at")]
    first_failure = min(failure_jobs, key=lambda job: job["completed_at"]) if failure_jobs else None
    first_time = elapsed(min(created), first_failure["completed_at"]) if first_failure and all(created) else None
    critical, path = _observed_critical_path(jobs)
    inferred_active, inferred_path = _critical_path(jobs)
    return {
        "source_file": source,
        "topology": topology,
        "run_id": next(iter(run_ids)) if len(run_ids) == 1 else None,
        "head_sha": next(iter(source_shas)) if len(source_shas) == 1 else None,
        "run_url": jobs[0].get("run_url"),
        "job_count": len(jobs),
        "complete": not issues,
        "successful": (all(job.get("conclusion") == "success" for job in jobs)
                       and not unexpected_unselected),
        "issues": issues,
        "wall_seconds": wall,
        "critical_path_seconds": critical,
        "critical_path_jobs": path,
        "inferred_active_path_seconds": inferred_active,
        "inferred_active_path_jobs": inferred_path,
        "first_failure_job": first_failure.get("name") if first_failure else None,
        "first_failure_seconds": first_time,
        "runner_minutes": runner_minutes,
        "setup_step_seconds": setup_seconds,
        "gate_step_seconds": gate_seconds,
        "named_build_step_seconds": build_seconds,
        "step_seconds_by_name": dict(sorted(step_seconds.items())),
    }


def median(records, field):
    values = [record[field] for record in records if record.get(field) is not None]
    return statistics.median(values) if values else None


def compare(batches, min_runs=3, min_wall_improvement=0.10,
            max_runner_regression=0.05, max_failure_regression=0.10):
    baseline = batches.get("baseline", [])
    all_shas = {record.get("head_sha") for runs in batches.values() for record in runs}
    common_source = len(all_shas) == 1 and None not in all_shas
    comparisons = {}
    for candidate in ("grouped", "shared-cli"):
        runs = batches.get(candidate, [])
        reasons = []
        if len(baseline) < min_runs or len(runs) < min_runs:
            reasons.append(f"need at least {min_runs} measured runs for baseline and candidate")
        if not all(record.get("complete") for record in baseline + runs):
            reasons.append("one or more jobs records are incomplete")
        if any(not record.get("successful") for record in baseline + runs):
            reasons.append("one or more workflow runs failed")
        if not common_source:
            reasons.append("baseline and candidates must use the same verified source commit")
        base_path = median(baseline, "critical_path_seconds")
        candidate_path = median(runs, "critical_path_seconds")
        base_minutes = median(baseline, "runner_minutes")
        candidate_minutes = median(runs, "runner_minutes")
        improvement = (1 - candidate_path / base_path) if base_path and candidate_path is not None else None
        runner_change = (candidate_minutes / base_minutes - 1) if base_minutes and candidate_minutes is not None else None
        failures_base = [record["first_failure_seconds"] for record in baseline if record.get("first_failure_seconds") is not None]
        failures_candidate = [record["first_failure_seconds"] for record in runs if record.get("first_failure_seconds") is not None]
        failure_change = None
        if failures_base and failures_candidate:
            base_failure = statistics.median(failures_base)
            candidate_failure = statistics.median(failures_candidate)
            failure_change = candidate_failure / base_failure - 1 if base_failure else None
            if failure_change is not None and failure_change > max_failure_regression:
                reasons.append("first-failure feedback regressed beyond threshold")
        if improvement is None or improvement < min_wall_improvement:
            reasons.append("critical-path improvement threshold not met")
        if runner_change is None or runner_change > max_runner_regression:
            reasons.append("runner-minute regression threshold exceeded or unavailable")
        status = "INCOMPLETE" if len(reasons) and any(token in " ".join(reasons) for token in ("need at least", "incomplete", "same verified")) else ("REJECT" if reasons else "PASS_THRESHOLDS")
        comparisons[candidate] = {
            "status": status,
            "adoption": "reject" if status != "PASS_THRESHOLDS" else "manual review required",
            "critical_path_improvement": improvement,
            "runner_minute_change": runner_change,
            "first_failure_feedback_change": failure_change,
            "failure_feedback_observed": bool(failures_base and failures_candidate),
            "reasons": reasons,
        }
    return comparisons


def build_report(paths, thresholds):
    batches = {}
    for topology, filenames in paths.items():
        batches[topology] = [analyze_jobs(read_jobs(path), topology, str(path)) for path in filenames]
    summary = {
        topology: {
            "runs": runs,
            "medians": {field: median(runs, field) for field in (
                "wall_seconds", "critical_path_seconds", "inferred_active_path_seconds",
                "runner_minutes",
                "setup_step_seconds", "gate_step_seconds", "named_build_step_seconds",
            )},
        }
        for topology, runs in batches.items()
    }
    return {
        "schema": 1,
        "evidence": "GitHub Actions jobs API JSON; durations are observed, never locally estimated",
        "critical_path_model": "adoption uses observed Plan creation to CI Required completion; inferred_active_path_seconds uses the declared dependency graph and per-job ready=max(created_at, parent completion)",
        "runner_minutes_basis": "sum of job started_at-to-completed_at minutes; not GitHub billing-rounded minutes",
        "thresholds": thresholds,
        "topologies": summary,
        "comparisons": compare(batches, **thresholds),
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    for topology in ("baseline", "grouped", "shared-cli"):
        parser.add_argument(f"--{topology}", action="append", type=Path, default=[])
    parser.add_argument("--min-runs", type=int, default=3)
    parser.add_argument("--min-wall-improvement", type=float, default=0.10)
    parser.add_argument("--max-runner-regression", type=float, default=0.05)
    parser.add_argument("--max-failure-regression", type=float, default=0.10)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args(argv)
    if args.min_runs < 1:
        parser.error("--min-runs must be positive")
    paths = {name: getattr(args, name.replace("-", "_")) for name in ("baseline", "grouped", "shared-cli")}
    thresholds = {
        "min_runs": args.min_runs,
        "min_wall_improvement": args.min_wall_improvement,
        "max_runner_regression": args.max_runner_regression,
        "max_failure_regression": args.max_failure_regression,
    }
    try:
        report = build_report(paths, thresholds)
    except (OSError, json.JSONDecodeError, ValueError, KeyError, TypeError) as error:
        print(f"topology: {error}", file=sys.stderr)
        return 2
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 0


if __name__ == "__main__":
    sys.exit(main())
