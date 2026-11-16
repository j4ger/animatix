#!/usr/bin/env python3
"""Build the workspace test graph once, then schedule its test harnesses."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess
import sys
import uuid

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from tools.ci.lib.workspace_tests import (
    WorkspaceTestError,
    build_test_inventory,
    execute_harness_plan,
    package_maps,
    parse_cargo_test_artifacts,
    run_logged_command,
)


BUILD_COMMAND = ["cargo", "test", "--workspace", "--no-run", "--locked", "--message-format=json"]
DOC_COMMAND = ["cargo", "test", "--workspace", "--doc", "--locked", "--", "--test-threads=1"]
SERIAL_PACKAGES = ["animatix", "animatix-render"]
CPU_WORKER_CAP = 2


def _available_cpus() -> int:
    try:
        return max(1, len(os.sched_getaffinity(0)))
    except (AttributeError, OSError):
        return max(1, os.cpu_count() or 1)


def _write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def _make_run_dir(target_dir: Path) -> tuple[Path, str]:
    run_id = f"{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')}-{os.getpid()}-{uuid.uuid4().hex[:10]}"
    run_dir = target_dir / "ci" / "workspace-tests" / run_id
    run_dir.mkdir(parents=True, exist_ok=False)
    marker = {"owner": "animatix-workspace-tests", "run_id": run_id}
    _write_json(run_dir / ".animatix-ci-owned.json", marker)
    return run_dir, run_id


def _stage(name: str, record: dict) -> bool:
    status = "PASS" if record["exit_code"] == 0 and not record.get("timed_out") else "FAIL"
    print(f"[workspace-tests] {status} {name} ({record['elapsed_seconds']:.2f}s)", flush=True)
    print(f"[workspace-tests] logs: {record['stdout_path']} / {record['stderr_path']}", flush=True)
    return status == "PASS"


def _run_workspace(repo: Path) -> int:
    repo = repo.resolve()
    env = os.environ.copy()
    env["ANIMATIX_REQUIRE_GPU"] = "1"
    env["CARGO_TERM_COLOR"] = "never"
    target_dir = Path(env.get("CARGO_TARGET_DIR", repo / "target"))
    if not target_dir.is_absolute():
        target_dir = repo / target_dir
    target_dir = target_dir.resolve()
    env["CARGO_TARGET_DIR"] = str(target_dir)
    run_dir, run_id = _make_run_dir(target_dir)
    summary = {
        "owner": "animatix-workspace-tests", "run_id": run_id, "repo": str(repo),
        "target_dir": str(target_dir), "started_at_utc": datetime.now(timezone.utc).isoformat(),
        "environment": {
            "ANIMATIX_REQUIRE_GPU": env["ANIMATIX_REQUIRE_GPU"],
            "CARGO_INCREMENTAL": env.get("CARGO_INCREMENTAL", "cargo-default"),
            "CARGO_TERM_COLOR": env["CARGO_TERM_COLOR"],
            "CARGO_TARGET_DIR": env["CARGO_TARGET_DIR"],
        },
        "cpu_worker_cap": CPU_WORKER_CAP, "serial_packages": SERIAL_PACKAGES,
        "commands": {"build": BUILD_COMMAND, "doctests": DOC_COMMAND},
        "stages": [],
    }
    _write_json(run_dir / "metadata.json", summary)

    metadata_run = run_logged_command(
        ["cargo", "metadata", "--no-deps", "--format-version=1", "--locked"], repo, env,
        run_dir / "cargo-metadata.json", run_dir / "cargo-metadata.stderr.log", timeout_seconds=120,
    )
    summary["stages"].append({"name": "metadata", **metadata_run})
    if not _stage("metadata", metadata_run):
        summary["exit_code"] = 1
        summary["finished_at_utc"] = datetime.now(timezone.utc).isoformat()
        _write_json(run_dir / "summary.json", summary)
        return 1
    try:
        metadata = json.loads(Path(metadata_run["stdout_path"]).read_text(encoding="utf-8"))
        if Path(metadata["target_directory"]).resolve() != target_dir:
            raise WorkspaceTestError(
                f"Cargo selected target directory {metadata['target_directory']}, expected {target_dir}"
            )
        _write_json(run_dir / "cargo-metadata-parsed.json", metadata)
    except (KeyError, json.JSONDecodeError, WorkspaceTestError) as error:
        summary["error"] = str(error)
        summary["exit_code"] = 1
        summary["finished_at_utc"] = datetime.now(timezone.utc).isoformat()
        _write_json(run_dir / "summary.json", summary)
        print(f"[workspace-tests] ERROR {error}", file=sys.stderr, flush=True)
        return 1

    build = run_logged_command(
        BUILD_COMMAND, repo, env, run_dir / "cargo-test-no-run.jsonl",
        run_dir / "cargo-test-no-run.stderr.log", timeout_seconds=7200,
    )
    summary["stages"].append({"name": "build-test-targets", **build})
    if not _stage("build-test-targets", build):
        summary["exit_code"] = 1
        summary["finished_at_utc"] = datetime.now(timezone.utc).isoformat()
        _write_json(run_dir / "summary.json", summary)
        return 1

    try:
        members, package_by_id = package_maps(metadata)
        artifacts = parse_cargo_test_artifacts(
            Path(build["stdout_path"]), members, target_dir=target_dir,
        )
        harnesses, cases = build_test_inventory(artifacts, package_by_id, target_dir)
    except (WorkspaceTestError, OSError, subprocess.TimeoutExpired) as error:
        summary["error"] = str(error)
        summary["exit_code"] = 1
        summary["finished_at_utc"] = datetime.now(timezone.utc).isoformat()
        _write_json(run_dir / "summary.json", summary)
        print(f"[workspace-tests] ERROR inventory: {error}", file=sys.stderr, flush=True)
        return 1

    inventory = {
        "workspace_members": sorted(members), "artifacts": artifacts,
        "harnesses": harnesses, "cases": cases,
        "active_case_count": sum(not case["ignored"] for case in cases),
        "ignored_case_count": sum(case["ignored"] for case in cases),
    }
    _write_json(run_dir / "inventory.json", inventory)
    workers = min(CPU_WORKER_CAP, _available_cpus())
    summary["effective_cpu_workers"] = workers
    plan = {
        # The executor inherits the parent environment. Persist only our public
        # overrides because CI uploads this directory as diagnostic evidence.
        "env": {key: env[key] for key in
                ("CARGO_TARGET_DIR", "ANIMATIX_REQUIRE_GPU", "CARGO_TERM_COLOR")},
        "items": harnesses, "max_workers": workers,
        "serial_packages": SERIAL_PACKAGES, "run_tag": run_id,
        "log_dir": str(run_dir / "harnesses"), "progress": True,
    }
    plan_path = run_dir / "harness-plan.json"
    result_path = run_dir / "harness-results.json"
    _write_json(plan_path, plan)
    try:
        status = execute_harness_plan(plan_path, result_path)
        harness_summary = json.loads(result_path.read_text(encoding="utf-8"))
    except (WorkspaceTestError, OSError, json.JSONDecodeError) as error:
        summary["error"] = str(error)
        summary["exit_code"] = 1
        summary["finished_at_utc"] = datetime.now(timezone.utc).isoformat()
        _write_json(run_dir / "summary.json", summary)
        print(f"[workspace-tests] ERROR harness dispatch: {error}", file=sys.stderr, flush=True)
        return 1
    expected_ids = sorted(case["id"] for case in cases if not case["ignored"])
    actual_ids = sorted(case_id for harness in harness_summary.get("harnesses", [])
                        for case_id in harness.get("executed_ids", []))
    expected_set, actual_set = set(expected_ids), set(actual_ids)
    coverage = {
        "equal": expected_set == actual_set and len(actual_ids) == len(actual_set),
        "expected_count": len(expected_ids), "actual_count": len(actual_ids),
        "missing": sorted(expected_set - actual_set),
        "unexpected": sorted(actual_set - expected_set),
    }
    if not coverage["equal"]:
        status = 1
        print(f"[workspace-tests] FAIL test inventory mismatch: {coverage}", file=sys.stderr, flush=True)
    summary["stages"].append({"name": "test-harnesses", "exit_code": status,
                              "harness_count": len(harness_summary.get("harnesses", [])),
                              "active_case_count": inventory["active_case_count"],
                              "coverage": coverage, "results_path": str(result_path)})

    doctests = run_logged_command(
        DOC_COMMAND, repo, env, run_dir / "doctests.stdout.log",
        run_dir / "doctests.stderr.log", timeout_seconds=7200,
    )
    summary["stages"].append({"name": "doctests", **doctests})
    doctests_ok = _stage("doctests", doctests)
    summary["finished_at_utc"] = datetime.now(timezone.utc).isoformat()
    summary["exit_code"] = 0 if status == 0 and doctests_ok else 1
    _write_json(run_dir / "summary.json", summary)
    print(f"[workspace-tests] summary: {run_dir / 'summary.json'}", flush=True)
    return summary["exit_code"]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=ROOT,
                        help="workspace root (defaults to the directory containing this script)")
    parser.add_argument("--execute-plan", type=Path,
                        help=argparse.SUPPRESS)
    parser.add_argument("--result", type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    if args.execute_plan:
        if not args.result:
            parser.error("--result is required with --execute-plan")
        try:
            return execute_harness_plan(args.execute_plan, args.result)
        except WorkspaceTestError as error:
            print(f"workspace test dispatch failed: {error}", file=sys.stderr)
            return 1
    if args.result:
        parser.error("--result is only valid with --execute-plan")
    return _run_workspace(args.repo)


if __name__ == "__main__":
    raise SystemExit(main())
