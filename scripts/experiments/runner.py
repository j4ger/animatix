"""Measure workspace test execution strategies without changing Cargo features.

This harness deliberately builds the workspace once with its normal feature
union, then compares Cargo's serial test orchestration, direct harness dispatch
with bounded target parallelism, and nextest. It never cleans a caller-owned
target directory.
"""

from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import sys
import threading
import time
import tomllib

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from scripts.experiments.common import ExperimentFailure, ExperimentOutput, run_timed
from scripts.experiments.coverage import (
    CASE_ALIAS, canonical_ids as _canonical_ids, coverage_diff as _coverage_diff,
    historical_canonical_ids as _historical_canonical_ids, ignored_ids as _ignored_ids,
    parse_artifacts as _parse_artifacts, parse_cargo_executed_ids as _parse_cargo_executed_ids,
    parse_doc_ids as _parse_doc_ids, parse_libtest_results as _parse_libtest_results,
    parse_nextest_junit_ids as _parse_nextest_junit_ids,
    parse_nextest_listing as _parse_nextest_listing, parse_test_list as _parse_test_list,
)
from scripts.experiments.runner_identity import (
    recheck_binary_hashes as _recheck_nextest_binary_hashes,
    verify_nextest_listing as _verify_nextest_listing,
)


BUILD_COMMAND = ["cargo", "test", "--workspace", "--no-run", "--locked", "--message-format=json"]
BASELINE_COMMAND = ["cargo", "test", "--workspace", "--locked", "--", "--test-threads=1"]
DOC_COMMAND = ["cargo", "test", "--workspace", "--doc", "--locked", "--", "--test-threads=1"]
EXAMPLES_COMMAND = ["cargo", "check", "--workspace", "--examples", "--locked"]
RUNNER_ARM_IDS = ("baseline", "cargo2", "cargo4", "nextest2", "nextest4")


def _select_runner_arm_ids(selection: str | None) -> list[str]:
    """Parse an optional comma-separated arm cohort in canonical order."""
    if selection is None:
        return list(RUNNER_ARM_IDS)
    requested = [part.strip() for part in selection.split(",")]
    if not selection.strip() or any(not arm for arm in requested):
        raise ExperimentFailure("ANIMATIX_RUNNER_ARMS must be a comma-separated list of runner arms")
    duplicates = sorted({arm for arm in requested if requested.count(arm) > 1})
    if duplicates:
        raise ExperimentFailure(f"ANIMATIX_RUNNER_ARMS contains duplicate arms: {duplicates}")
    unknown = sorted(set(requested) - set(RUNNER_ARM_IDS))
    if unknown:
        raise ExperimentFailure(f"ANIMATIX_RUNNER_ARMS contains unknown arms: {unknown}")
    if "baseline" not in requested:
        raise ExperimentFailure("ANIMATIX_RUNNER_ARMS must include baseline")
    if len(requested) < 2:
        raise ExperimentFailure("ANIMATIX_RUNNER_ARMS must include baseline and at least one candidate")
    return [arm for arm in RUNNER_ARM_IDS if arm in requested]

_ACTIVE_HARNESS_GROUPS: dict[int, subprocess.Popen] = {}
_ACTIVE_HARNESS_LOCK = threading.RLock()
_DISPATCH_SHUTTING_DOWN = False
def _metadata(repo: Path) -> dict:
    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version=1"], cwd=repo,
        capture_output=True, text=True, check=False, timeout=60,
    )
    if result.returncode:
        raise ExperimentFailure(f"cargo metadata failed: {result.stderr[-2000:]}")
    return json.loads(result.stdout)


def _available_cpus() -> int:
    try:
        return max(1, len(os.sched_getaffinity(0)))
    except (AttributeError, OSError):
        return max(1, os.cpu_count() or 1)


def _prepare_artifacts(repo: Path, output: ExperimentOutput, workspace_members: set[str],
                       target_dir: Path, artifact_json: Path | None, label: str) -> tuple[list[dict], dict | None]:
    if artifact_json:
        artifacts_path = output.logs_dir / f"{label}-precompiled-artifacts.jsonl"
        shutil.copyfile(artifact_json.resolve(), artifacts_path)
        build_record = None
    else:
        build_log = output.logs_dir / f"{label}-test-no-run.jsonl"
        build_stderr = output.logs_dir / f"{label}-test-no-run.stderr.log"
        env = {"CARGO_TARGET_DIR": str(target_dir), "CARGO_INCREMENTAL": "0", "CARGO_TERM_COLOR": "never",
               "ANIMATIX_REQUIRE_GPU": "1"}
        build_record = run_timed(
            f"runner-{label}-prepare", BUILD_COMMAND, repo, env, build_log,
            timeout_seconds=7200, stderr_path=build_stderr,
        )
        output.append_record({"type": "prepare-build", "label": label, **build_record})
        if build_record["exit_code"] or build_record["timed_out"]:
            raise ExperimentFailure(f"workspace test artifact build failed; see {build_log}")
        artifacts_path = build_log
    artifacts = _parse_artifacts(artifacts_path, workspace_members, target_dir)
    return artifacts, build_record


def _package_maps(metadata: dict) -> tuple[set[str], dict[str, dict]]:
    members = set(metadata["workspace_members"])
    return members, {package["id"]: package for package in metadata["packages"] if package["id"] in members}


def _cargo_cases(artifacts: list[dict], package_by_id: dict[str, dict], target_dir: Path | None = None) -> tuple[list[dict], list[dict]]:
    cases: list[dict] = []
    harnesses = []
    for artifact in artifacts:
        package = package_by_id[artifact["package_id"]]
        executable = Path(artifact["executable"])
        list_env = {**os.environ, "ANIMATIX_REQUIRE_GPU": "1", "CARGO_INCREMENTAL": "0"}
        if target_dir:
            list_env["CARGO_TARGET_DIR"] = str(target_dir)
        listed = subprocess.run(
            [str(executable), "--list", "--format", "terse"], cwd=Path(package["manifest_path"]).parent,
            env=list_env, capture_output=True, text=True, check=False, timeout=60,
        )
        if listed.returncode:
            raise ExperimentFailure(f"test listing failed for {executable}: {listed.stderr[-1000:]}")
        ignored = subprocess.run(
            [str(executable), "--ignored", "--list", "--format", "terse"], cwd=Path(package["manifest_path"]).parent,
            env=list_env, capture_output=True, text=True, check=False, timeout=60,
        )
        if ignored.returncode:
            raise ExperimentFailure(f"ignored-test listing failed for {executable}: {ignored.stderr[-1000:]}")
        names = _parse_test_list(listed.stdout)
        ignored_names = set(_parse_test_list(ignored.stdout))
        pkg = package["name"]
        target_name = artifact["target_name"]
        active = []
        for name in names:
            case = {"id": f"{pkg}::{target_name}::{name}", "package": pkg,
                    "binary": target_name, "name": name, "ignored": name in ignored_names}
            cases.append(case)
            if not case["ignored"]:
                active.append(case)
        harnesses.append({
            **artifact, "package_name": pkg, "manifest_dir": str(Path(package["manifest_path"]).parent),
            "package_env": {
                "CARGO_MANIFEST_DIR": str(Path(package["manifest_path"]).parent),
                "CARGO_PKG_NAME": package.get("name", ""),
                "CARGO_PKG_VERSION": package.get("version", ""),
                "CARGO_PKG_AUTHORS": ":".join(package.get("authors", [])),
                "CARGO_PKG_DESCRIPTION": package.get("description") or "",
                "CARGO_PKG_HOMEPAGE": package.get("homepage") or "",
                "CARGO_PKG_REPOSITORY": package.get("repository") or "",
                "CARGO_PKG_LICENSE": package.get("license") or "",
            },
            "cases": active,
        })
    # Cargo occasionally emits duplicate artifact records for a target. Keep IDs
    # strict and make that visible instead of silently deduplicating coverage.
    ids = [case["id"] for case in cases]
    if len(ids) != len(set(ids)):
        duplicates = sorted({case_id for case_id in ids if ids.count(case_id) > 1})
        raise ExperimentFailure(f"duplicate canonical test IDs: {duplicates[:12]}")
    return harnesses, cases


def _nextest_cases(repo: Path, target_dir: Path, output: ExperimentOutput,
                   artifacts: list[dict], filter_expr: str | None = None,
                   label: str = "all") -> tuple[list[dict], dict, dict]:
    command = ["cargo", "nextest", "list", "--workspace", "--locked", "--message-format", "json"]
    if filter_expr:
        command.extend(["-E", filter_expr])
    result = subprocess.run(
        command, cwd=repo,
        env={**os.environ, "CARGO_TARGET_DIR": str(target_dir), "CARGO_TERM_COLOR": "never",
             "CARGO_INCREMENTAL": "0", "ANIMATIX_REQUIRE_GPU": "1"},
        capture_output=True, text=True, check=False, timeout=300,
    )
    log_path = output.logs_dir / f"nextest-list-{label}.json"
    stderr_path = output.logs_dir / f"nextest-list-{label}.stderr.log"
    log_path.write_text(result.stdout, encoding="utf-8")
    stderr_path.write_text(result.stderr, encoding="utf-8")
    if result.returncode:
        raise ExperimentFailure(f"cargo nextest list failed: {result.stderr[-2000:]}")
    try:
        data = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise ExperimentFailure(f"nextest list returned invalid JSON: {error}") from error
    cases, suite_count = _parse_nextest_listing(data)
    try:
        identity = _verify_nextest_listing(
            data, artifacts, target_dir, exact_suite_set=filter_expr is None,
        )
    except ExperimentFailure as error:
        output.append_record({"type": "nextest-binary-identity-listing", "label": label,
                              "verified": False, "json_path": str(log_path),
                              "stderr_path": str(stderr_path), "error": str(error)})
        raise
    identity_path = f"nextest-binary-identity-{label}.json"
    output.write_json(identity_path, identity)
    output.append_record({"type": "nextest-binary-identity-listing", "label": label,
                          "verified": True, "identity_sha256": identity["identity_sha256"],
                          "target_directory": identity["target_directory"],
                          "listed_suite_count": identity["listed_suite_count"],
                          "prepared_artifact_count": identity["prepared_artifact_count"],
                          "identity_path": str(output.directory / identity_path)})
    return cases, {"command": command, "json_path": str(log_path),
                   "stderr_path": str(stderr_path), "suite_count": suite_count,
                   "identity_sha256": identity["identity_sha256"],
                   "identity_path": str(output.directory / identity_path)}, identity


def _record_nextest_identity_check(output: ExperimentOutput, identity: dict,
                                   target_dir: Path, arm_id: str, round_number: int,
                                   phase: str) -> dict:
    try:
        checked = _recheck_nextest_binary_hashes(identity, target_dir)
    except ExperimentFailure as error:
        output.append_record({"type": "nextest-binary-identity-check", "arm": arm_id,
                              "round": round_number, "phase": phase,
                              "passed": False, "error": str(error)})
        raise
    record = {"type": "nextest-binary-identity-check", "arm": arm_id,
              "round": round_number, "phase": phase, "passed": True, **checked}
    output.append_record(record)
    return record


def _nextest_filter_expression(groups: list[dict]) -> str:
    """Make a compact exact-ID filter, grouping testcase names by binary."""
    by_binary: dict[tuple[str, str], set[str]] = {}
    for group in groups:
        for case in group["cases"]:
            binary_id = case.get("binary_id")
            if not binary_id:
                raise ExperimentFailure(f"nextest binary ID is missing for selected case {case['id']}")
            by_binary.setdefault((case["package"], binary_id), set()).add(case["name"])
    terms = []
    for (package, binary_id), names in sorted(by_binary.items()):
        tests = " | ".join(f"test(={name})" for name in sorted(names))
        terms.append(f"(package(={package}) & binary_id(={binary_id}) & ({tests}))")
    if not terms:
        raise ExperimentFailure("sensitive test selection produced no nextest filter terms")
    return " | ".join(terms)


def _write_nextest_config(repo: Path, output: ExperimentOutput, arm_id: str,
                          round_number: int) -> tuple[Path, Path]:
    """Preserve the repository nextest policy and append per-run JUnit output."""
    source_path = Path(repo) / ".config" / "nextest.toml"
    if not source_path.is_file():
        raise ExperimentFailure(f"repository nextest configuration is missing: {source_path}")
    base_source = source_path.read_text(encoding="utf-8")
    try:
        base = tomllib.loads(base_source)
    except tomllib.TOMLDecodeError as error:
        raise ExperimentFailure(f"invalid repository nextest config {source_path}: {error}") from error
    renderer_group = base.get("test-groups", {}).get("renderer-gpu", {})
    renderer_override = any(
        row.get("filter") == "package(animatix-render)" and row.get("test-group") == "renderer-gpu"
        for row in base.get("profile", {}).get("default", {}).get("overrides", [])
    )
    if renderer_group.get("max-threads") != 1 or not renderer_override:
        raise ExperimentFailure("repository nextest config does not keep the renderer package in renderer-gpu group=1")
    if "store" in base or "junit" in base.get("profile", {}).get("default", {}):
        raise ExperimentFailure("repository nextest config defines store/JUnit settings that conflict with per-run reporting")
    safe = re.sub(r"[^A-Za-z0-9_.-]+", "_", arm_id)
    store_dir = output.logs_dir / "nextest-store" / safe / f"round-{round_number}"
    config_path = output.logs_dir / f"nextest-config-{safe}-{round_number}.toml"
    report_path = store_dir / "default" / "junit.xml"
    suffix = (
        "\n\n[store]\n"
        f"dir = {json.dumps(str(store_dir))}\n\n"
        "[profile.default.junit]\n"
        'path = "junit.xml"\n'
        "store-success-output = false\n"
        "store-failure-output = true\n"
    )
    merged = base_source.rstrip() + suffix
    try:
        tomllib.loads(merged)
    except tomllib.TOMLDecodeError as error:
        raise ExperimentFailure(f"generated nextest config conflicts with repository policy: {error}") from error
    config_path.write_text(merged, encoding="utf-8")
    return config_path, report_path


def _case_groups(harnesses: list[dict]) -> dict[str, dict]:
    groups: dict[str, dict] = {}
    for harness in harnesses:
        pkg = harness["package_name"]
        binary = harness["target_name"]
        pathish = f"{pkg} {binary}".lower()
        for case in harness["cases"]:
            text = f"{pathish} {case['name']}".lower()
            categories = set()
            if pkg == "animatix-render":
                categories.add("gpu-renderer")
            if pkg == "animatix-text" or any(word in text for word in ("text_compile", "font", "cache")):
                categories.add("text-cache")
            if any(word in text for word in ("media", "assets", "svg", "plugin", "source_edit", "document", "handler", "temp", "path")):
                categories.add("filesystem")
            if not categories:
                continue
            # Libtest filters are substrings. Group at module granularity where
            # possible, then record every case that that filter will execute.
            pieces = case["name"].split("::")
            filter_text = "::".join(pieces[:-1]) if len(pieces) > 1 else case["name"]
            key = f"{pkg}::{binary}::{filter_text}"
            group = groups.setdefault(key, {"package": pkg, "binary": binary,
                                            "filter": filter_text, "categories": set(),
                                            "cases": [], "harness": harness})
            group["categories"].update(categories)
    # A parent module's substring filter already includes nested modules.
    # Keep a disjoint invocation plan so Cargo and nextest stress each ID once.
    disjoint = {}
    for key, group in sorted(groups.items(), key=lambda item: len(item[1]["filter"])):
        parent = next((existing for existing in disjoint.values()
                       if existing["package"] == group["package"]
                       and existing["binary"] == group["binary"]
                       and existing["filter"] in group["filter"]), None)
        if parent is not None:
            parent["categories"].update(group["categories"])
        else:
            disjoint[key] = group
    groups = disjoint
    for group in groups.values():
        matching = [case for case in group["harness"]["cases"] if group["filter"] in case["name"]]
        group["cases"] = matching
    return groups


def _write_plan(path: Path, plan: dict) -> None:
    path.write_text(json.dumps(plan, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def _signal_process_group(process_group: int, sig: int) -> None:
    try:
        os.killpg(process_group, sig)
    except ProcessLookupError:
        pass


def _terminate_harness_group(process: subprocess.Popen, grace_seconds: float = 1.0) -> None:
    """Terminate a harness and every child that inherited its private group."""
    _signal_process_group(process.pid, signal.SIGTERM)
    try:
        process.wait(timeout=grace_seconds)
    except subprocess.TimeoutExpired:
        pass
    _signal_process_group(process.pid, signal.SIGKILL)
    try:
        process.wait(timeout=1)
    except subprocess.TimeoutExpired:
        pass


def _stop_active_harness_groups(signum: int, _frame) -> None:
    """Clean nested child sessions when the outer run_timed deadline fires."""
    global _DISPATCH_SHUTTING_DOWN
    with _ACTIVE_HARNESS_LOCK:
        _DISPATCH_SHUTTING_DOWN = True
        processes = list(_ACTIVE_HARNESS_GROUPS.values())
    for process in processes:
        _signal_process_group(process.pid, signal.SIGTERM)
    time.sleep(0.1)
    for process in processes:
        _signal_process_group(process.pid, signal.SIGKILL)
    raise SystemExit(128 + signum)


def _execute_harness_item(item: dict, env: dict[str, str], log_dir: Path, run_tag: str,
                          filter_text: str | None = None, timeout_seconds: float = 3600) -> dict:
    executable = item["executable"]
    command = [executable]
    if filter_text is not None:
        command.append(filter_text)
    if item.get("skip_tests") is not None:
        command.append("--exact")
        for name in item["skip_tests"]:
            command.extend(["--skip", name])
    command.append("--test-threads=1")
    safe_binary = re.sub(r"[^A-Za-z0-9_.-]+", "_",
                         f"{item['package_name']}--{item['target_name']}")
    suffix = re.sub(r"[^A-Za-z0-9_.-]+", "_", filter_text or "all")[:80]
    log_path = log_dir / f"{run_tag}-{safe_binary}-{suffix}.log"
    stderr_log_path = log_dir / f"{run_tag}-{safe_binary}-{suffix}.stderr.log"
    started = time.monotonic()
    process = None
    status = 127
    timed_out = False
    error = None
    try:
        harness_env = {**env, **item.get("package_env", {})}
        with log_path.open("wb") as log, stderr_log_path.open("wb") as stderr_log:
            with _ACTIVE_HARNESS_LOCK:
                if _DISPATCH_SHUTTING_DOWN:
                    raise RuntimeError("dispatch is shutting down")
                process = subprocess.Popen(
                    command, cwd=item["manifest_dir"], env=harness_env,
                    stdout=log, stderr=stderr_log, start_new_session=True,
                )
                _ACTIVE_HARNESS_GROUPS[process.pid] = process
            try:
                status = process.wait(timeout=timeout_seconds)
            except subprocess.TimeoutExpired:
                timed_out = True
                error = f"timed out after {timeout_seconds:g} seconds"
                _terminate_harness_group(process)
                status = 124
            else:
                # A test can exit after leaving a helper behind; don't leak it
                # into the following targets or into the next measured arm.
                _signal_process_group(process.pid, signal.SIGTERM)
                time.sleep(0.02)
                _signal_process_group(process.pid, signal.SIGKILL)
        if process is not None:
            with _ACTIVE_HARNESS_LOCK:
                _ACTIVE_HARNESS_GROUPS.pop(process.pid, None)
    except OSError as exception:
        error = str(exception)
        status = 127
    except RuntimeError as exception:
        error = str(exception)
        status = 125
    finally:
        if process is not None:
            with _ACTIVE_HARNESS_LOCK:
                _ACTIVE_HARNESS_GROUPS.pop(process.pid, None)
    if status < 0:
        status = 128 - status
    by_name = {case["name"]: case["id"] for case in item.get("cases", [])}
    observed = _parse_libtest_results(log_path) if log_path.exists() else []
    executed_ids = sorted(by_name[row["name"]] for row in observed
                          if row["status"] in {"ok", "FAILED"} and row["name"] in by_name)
    ignored_ids = sorted(by_name[row["name"]] for row in observed
                         if row["status"] == "ignored" and row["name"] in by_name)
    return {"package": item["package_name"], "binary": item["target_name"],
            "filter": filter_text, "command": command, "log_path": str(log_path),
            "stderr_log_path": str(stderr_log_path),
            "exit_code": status, "elapsed_seconds": time.monotonic() - started,
            "timed_out": timed_out, "error": error,
            "executed_ids": executed_ids, "ignored_ids": ignored_ids,
            "observed_test_results": observed}


def _execute_plan(plan_path: Path, result_path: Path) -> int:
    plan = json.loads(plan_path.read_text(encoding="utf-8"))
    env = {**os.environ, **plan.get("env", {}), "ANIMATIX_REQUIRE_GPU": "1", "CARGO_INCREMENTAL": "0"}
    target = Path(env.get("CARGO_TARGET_DIR", "target")).resolve()
    lib_dirs = [str(target / "debug" / "deps"), str(target / "debug")]
    for variable in ("LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH"):
        existing = env.get(variable)
        env[variable] = os.pathsep.join([*lib_dirs, *([existing] if existing else [])])
    items = plan["items"]
    log_dir = Path(plan["log_dir"])
    log_dir.mkdir(parents=True, exist_ok=True)
    serial_packages = set(plan.get("serial_packages", []))
    serial = [item for item in items if item["package_name"] in serial_packages]
    parallel = [item for item in items if item["package_name"] not in serial_packages]
    results = []
    global _DISPATCH_SHUTTING_DOWN
    _DISPATCH_SHUTTING_DOWN = False
    prior_handlers = {sig: signal.getsignal(sig) for sig in (signal.SIGTERM, signal.SIGINT)}
    for sig in prior_handlers:
        signal.signal(sig, _stop_active_harness_groups)
    try:
        for item in serial:
            filters = item.get("filters") or [None]
            for filter_text in filters:
                result = _execute_harness_item(item, env, log_dir, plan["run_tag"], filter_text)
                results.append(result)
        max_workers = max(1, int(plan.get("max_workers", 1)))
        jobs = [(item, filter_text) for item in parallel for filter_text in (item.get("filters") or [None])]
        if jobs:
            with ThreadPoolExecutor(max_workers=max_workers) as pool:
                futures = {pool.submit(_execute_harness_item, item, env, log_dir,
                                       plan["run_tag"], filter_text): (item, filter_text)
                           for item, filter_text in jobs}
                for future in as_completed(futures):
                    results.append(future.result())
    finally:
        for sig, handler in prior_handlers.items():
            signal.signal(sig, handler)
    status = 1 if any(result["exit_code"] for result in results) else 0
    result_path.write_text(json.dumps({"exit_code": status, "harnesses": results},
                                      indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return status


def _run_dispatch(output: ExperimentOutput, target_dir: Path, arm_id: str, round_number: int,
                  harnesses: list[dict], mode: str, workers: int, selected_groups: list[dict] | None = None,
                  docs: bool = True, nextest_cases: list[dict] | None = None,
                  nextest_identity: dict | None = None) -> dict:
    log_dir = output.logs_dir / "harnesses" / arm_id / f"round-{round_number}"
    log_dir.mkdir(parents=True, exist_ok=True)
    selected_ids = None
    items = []
    for harness in harnesses:
        cases = harness["cases"]
        filters = None
        skip_tests = None
        if selected_groups is not None:
            matching = [group for group in selected_groups
                        if group["package"] == harness["package_name"] and group["binary"] == harness["target_name"]]
            if not matching:
                continue
            selected_names = {case["name"] for group in matching for case in group["cases"]}
            # Libtest has no include-union option. Run this target once and
            # exclude the complement with exact skips, avoiding overlapping
            # module filters and repeated process/font initialization.
            skip_tests = sorted(case["name"] for case in cases if case["name"] not in selected_names)
        items.append({"package_name": harness["package_name"], "target_name": harness["target_name"],
                      "manifest_dir": harness["manifest_dir"], "executable": harness["executable"],
                      "package_env": harness.get("package_env", {}), "filters": filters,
                      "skip_tests": skip_tests,
                      "cases": harness["cases"]})
    if selected_groups is not None:
        selected_ids = sorted({case["id"] for group in selected_groups for case in group["cases"]})
    else:
        selected_ids = sorted(case["id"] for harness in harnesses for case in harness["cases"])

    command_env = {"CARGO_TARGET_DIR": str(target_dir), "CARGO_INCREMENTAL": "0",
                   "CARGO_TERM_COLOR": "never", "ANIMATIX_REQUIRE_GPU": "1"}
    if mode == "nextest":
        if nextest_identity is None:
            raise ExperimentFailure("nextest dispatch has no verified binary identity snapshot")
        config_path, junit_path = _write_nextest_config(output.repo, output, arm_id, round_number)
        if selected_groups is None:
            command = ["cargo", "nextest", "run", "--workspace", "--locked", "--no-fail-fast",
                       "--test-threads", str(workers)]
        else:
            # Exact binary/test filters keep the sensitive subset auditable.
            command = ["cargo", "nextest", "run", "--workspace", "--locked", "--no-fail-fast",
                       "--test-threads", str(workers), "-E",
                       _nextest_filter_expression(selected_groups)]
        command.extend(["--config-file", str(config_path)])
        identity_before = _record_nextest_identity_check(
            output, nextest_identity, target_dir, arm_id, round_number, "before",
        )
        run = run_timed(f"runner-{arm_id}-{round_number}", command, output.repo, command_env,
                        output.logs_dir / f"{arm_id}-round-{round_number}.log", timeout_seconds=7200)
        try:
            identity_after = _record_nextest_identity_check(
                output, nextest_identity, target_dir, arm_id, round_number, "after",
            )
        except ExperimentFailure:
            output.append_record({"type": "runner-suite", "arm": arm_id, "round": round_number,
                                  "mode": mode, "workers": workers,
                                  "binary_identity_sha256": nextest_identity["identity_sha256"],
                                  "binary_identity_verified": False, **run})
            raise
        actual_ids = []
        suite_details = []
        coverage = {"equal": False, "expected_count": len(selected_ids), "actual_count": 0,
                    "missing": selected_ids, "unexpected": []}
        report_error = None
        if not run["exit_code"] and not run["timed_out"]:
            try:
                actual_ids, suite_details = _parse_nextest_junit_ids(junit_path, nextest_cases or [])
                coverage = _coverage_diff(set(selected_ids), set(actual_ids))
                if not coverage["equal"]:
                    report_error = "JUnit executed IDs differ from expected filter/listing"
                    run["exit_code"] = 1
            except ExperimentFailure as error:
                report_error = str(error)
                run["exit_code"] = 1
        output.append_record({"type": "runner-suite", "arm": arm_id, "round": round_number,
                              "mode": mode, "workers": workers, "selected_ids": selected_ids,
                              "binary_identity_sha256": nextest_identity["identity_sha256"],
                              "binary_identity_verified": True,
                              "binary_identity_checks": [identity_before, identity_after],
                              "actual_ids": actual_ids, "test_id_coverage": coverage,
                              "junit_path": str(junit_path), "junit_suites": suite_details,
                              "report_error": report_error, **run})
        if run["exit_code"] or run["timed_out"]:
            return {**run, "selected_ids": selected_ids, "actual_ids": actual_ids,
                    "binary_identity_sha256": nextest_identity["identity_sha256"],
                    "test_id_coverage": coverage, "junit_path": str(junit_path),
                    "report_error": report_error, "doc_ids": [], "dispatch_details": []}
        details = []
        if docs:
            doc = run_timed(f"runner-{arm_id}-docs-{round_number}", DOC_COMMAND, output.repo,
                            command_env, output.logs_dir / f"{arm_id}-docs-round-{round_number}.log",
                            timeout_seconds=7200,
                            stderr_path=output.logs_dir / f"{arm_id}-docs-round-{round_number}.stderr.log")
            output.append_record({"type": "runner-docs", "arm": arm_id, "round": round_number, **doc})
            details.append(doc)
            run = {**run, "exit_code": doc["exit_code"], "timed_out": doc["timed_out"],
                   "elapsed_seconds": run["elapsed_seconds"] + doc["elapsed_seconds"],
                   "peak_tree_rss_bytes": max(run["peak_tree_rss_bytes"], doc["peak_tree_rss_bytes"])}
        return {**run, "selected_ids": selected_ids, "actual_ids": actual_ids,
                "binary_identity_sha256": nextest_identity["identity_sha256"],
                "test_id_coverage": coverage, "junit_path": str(junit_path),
                "doc_ids": _parse_doc_ids(details), "dispatch_details": details}

    plan_path = output.logs_dir / f"dispatch-{arm_id}-round-{round_number}.json"
    result_path = output.logs_dir / f"dispatch-{arm_id}-round-{round_number}-result.json"
    plan = {"env": command_env, "items": items, "max_workers": workers,
            "serial_packages": ["animatix", "animatix-render"], "run_tag": f"{arm_id}-{round_number}",
            "log_dir": str(log_dir)}
    _write_plan(plan_path, plan)
    dispatch = [sys.executable, str(Path(__file__).resolve()), "--execute-plan", str(plan_path),
                "--result", str(result_path)]
    run = run_timed(f"runner-{arm_id}-{round_number}", dispatch, output.repo, command_env,
                    output.logs_dir / f"{arm_id}-round-{round_number}.log", timeout_seconds=7200)
    output.append_record({"type": "runner-suite", "arm": arm_id, "round": round_number,
                          "mode": mode, "workers": workers, "selected_ids": selected_ids, **run})
    details = []
    if result_path.exists():
        details = json.loads(result_path.read_text(encoding="utf-8")).get("harnesses", [])
    actual_ids = sorted({case_id for item in details for case_id in item.get("executed_ids", [])})
    expected_ids = (selected_ids if selected_ids is not None else
                    sorted(case["id"] for harness in harnesses for case in harness["cases"]))
    test_coverage = _coverage_diff(set(expected_ids or []), set(actual_ids))
    if run["exit_code"] or run["timed_out"]:
        return {**run, "selected_ids": expected_ids, "actual_ids": actual_ids,
                "test_id_coverage": test_coverage, "doc_ids": [], "dispatch_details": details}
    if not test_coverage["equal"]:
        run["exit_code"] = 1
    if docs:
        doc = run_timed(f"runner-{arm_id}-docs-{round_number}", DOC_COMMAND, output.repo,
                        command_env, output.logs_dir / f"{arm_id}-docs-round-{round_number}.log",
                        timeout_seconds=7200,
                        stderr_path=output.logs_dir / f"{arm_id}-docs-round-{round_number}.stderr.log")
        output.append_record({"type": "runner-docs", "arm": arm_id, "round": round_number, **doc})
        details.append(doc)
        run = {**run, "exit_code": doc["exit_code"], "timed_out": doc["timed_out"],
               "elapsed_seconds": run["elapsed_seconds"] + doc["elapsed_seconds"],
               "peak_tree_rss_bytes": max(run["peak_tree_rss_bytes"], doc["peak_tree_rss_bytes"])}
    if not test_coverage["equal"] or any(detail.get("exit_code") for detail in details if "exit_code" in detail):
        run["exit_code"] = 1
    return {**run, "selected_ids": expected_ids, "actual_ids": actual_ids,
            "test_id_coverage": test_coverage, "doc_ids": _parse_doc_ids(details),
            "dispatch_details": details}


def _run_full_baseline(output: ExperimentOutput, target_dir: Path, round_number: int,
                       harnesses: list[dict]) -> dict:
    env = {"CARGO_TARGET_DIR": str(target_dir), "CARGO_INCREMENTAL": "0",
           "CARGO_TERM_COLOR": "never", "ANIMATIX_REQUIRE_GPU": "1"}
    run = run_timed(f"runner-baseline-{round_number}", BASELINE_COMMAND, output.repo, env,
                    output.logs_dir / f"baseline-round-{round_number}.log", timeout_seconds=7200,
                    stderr_path=output.logs_dir / f"baseline-round-{round_number}.stderr.log")
    output.append_record({"type": "runner-suite", "arm": "baseline", "round": round_number,
                          "mode": "cargo-workspace-serial", "workers": 1, **run})
    actual_ids, unresolved = _parse_cargo_executed_ids(
        Path(run["log_path"]), harnesses, Path(run["stderr_log_path"]),
    )
    expected_ids = sorted(case["id"] for harness in harnesses for case in harness["cases"])
    run["selected_ids"] = expected_ids
    run["actual_ids"] = actual_ids
    run["test_id_coverage"] = _coverage_diff(set(expected_ids), set(actual_ids))
    run["unresolved_test_harnesses"] = unresolved
    run["doc_ids"] = _parse_doc_ids([run])
    return run


def _run_cold(output: ExperimentOutput, rounds: int) -> list[dict]:
    measured = []
    for round_number in range(1, rounds + 1):
        target = output.owned_target("cold-target", reset=True)
        env = {"CARGO_TARGET_DIR": str(target), "CARGO_INCREMENTAL": "0",
               "CARGO_TERM_COLOR": "never", "ANIMATIX_REQUIRE_GPU": "1"}
        record = run_timed(f"runner-cold-{round_number}", BUILD_COMMAND, output.repo, env,
                           output.logs_dir / f"cold-{round_number}.log", timeout_seconds=7200)
        record.update({"type": "runner-cold-build", "round": round_number, "target_dir": str(target)})
        output.append_record(record)
        measured.append(record)
        if record["exit_code"] or record["timed_out"]:
            # A cold build failure is evidence; do not waste repeated rounds.
            break
    return measured


def _median(values: list[float]) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    middle = len(ordered) // 2
    if len(ordered) % 2:
        return ordered[middle]
    return (ordered[middle - 1] + ordered[middle]) / 2


def run_runner_experiment(repo: Path, output: ExperimentOutput, warm_rounds: int,
                          cold_rounds: int, existing_target_dir: Path | None = None,
                          build_artifact_json: Path | None = None) -> dict:
    """Run paired warm test-dispatch samples and independent cold compile samples.

    The caller owns ``output.machine_lock()``. Optional target and artifact
    arguments allow CI to reuse an already-built workspace without cleaning it.
    """
    if warm_rounds < 1 or cold_rounds < 0:
        raise ExperimentFailure("warm_rounds must be at least 1 and cold_rounds nonnegative")
    repo = Path(repo).resolve()
    runner_arm_selection = os.environ.get("ANIMATIX_RUNNER_ARMS")
    selected_arm_ids = _select_runner_arm_ids(runner_arm_selection)
    target_env = os.environ.get("CARGO_TARGET_DIR")
    target_dir = Path(existing_target_dir or target_env or (repo / "target")).resolve()
    if build_artifact_json is None:
        configured = os.environ.get("ANIMATIX_EXPERIMENT_BUILD_JSON")
        build_artifact_json = Path(configured) if configured else None
    metadata = _metadata(repo)
    workspace_members, package_by_id = _package_maps(metadata)
    baseline_target = Path(os.environ.get("ANIMATIX_BASELINE_TARGET_DIR", str(target_dir))).resolve()
    baseline_artifact_json = os.environ.get("ANIMATIX_BASELINE_BUILD_JSON")

    output.metadata.update({
        "runner_experiment": {
            "workspace_target_dir": str(target_dir),
            "baseline_target_dir": str(baseline_target),
            "precompiled_artifact_json": str(build_artifact_json.resolve()) if build_artifact_json else None,
            "baseline_artifact_json": baseline_artifact_json,
            "arm_selection_source": "ANIMATIX_RUNNER_ARMS" if runner_arm_selection is not None else "default",
            "selected_arms": selected_arm_ids,
            "available_arms": list(RUNNER_ARM_IDS),
            "partial_cohort": selected_arm_ids != list(RUNNER_ARM_IDS),
            "feature_union": "one cargo test --workspace --no-run build per target; no cargo -p splitting",
            "coverage_identity": "package::target::test-name; ignored tests listed but excluded from execution coverage",
            "case_aliases": CASE_ALIAS,
            "timeout_seconds": 7200,
            "cold_target_policy": "fresh ExperimentOutput-owned target subdirectories only",
            "recorded_at_utc": datetime.now(timezone.utc).isoformat(),
            "platform": platform.platform(),
            "test_environment": {
                "ANIMATIX_REQUIRE_GPU": "1",
                "CARGO_INCREMENTAL": "0",
                **{
                    key: os.environ[key]
                    for key in (
                        "ANIMATIX_DUMP_STAGES", "ANIMATIX_DUMP_FRAMES", "ANIMATIX_PROBE",
                        "WGPU_BACKEND", "WGPU_POWER_PREF", "XDG_RUNTIME_DIR", "DISPLAY",
                        "WAYLAND_DISPLAY", "VK_ICD_FILENAMES", "VK_LAYER_PATH",
                        "FONTCONFIG_FILE", "FONTCONFIG_PATH", "XDG_CACHE_HOME",
                        "RUST_TEST_THREADS", "CARGO_TARGET_DIR", "ANIMATIX_RUNNER_ARMS",
                    ) if key in os.environ
                },
            },
            "process_global_isolation": {
                "cargo_fallback": "engine and renderer harnesses are serial; other test harness processes are bounded by worker count",
                "nextest_renderer": "renderer-gpu group max-threads=1 from .config/nextest.toml",
                "sensitive_repeats": "module-filtered/exact tests start fresh harness processes each repetition",
            },
        }
    })
    output.write_metadata()

    # Candidate workspace build/listing preserves the original workspace feature
    # graph. If a caller provided compiler JSON, do not rerun cargo to generate it.
    harness_artifacts, candidate_prepare = _prepare_artifacts(
        repo, output, workspace_members, target_dir, build_artifact_json, "candidate",
    )
    for artifact in harness_artifacts:
        artifact["package_name"] = package_by_id[artifact["package_id"]]["name"]
        with Path(artifact["executable"]).open("rb") as executable:
            artifact["sha256"] = hashlib.file_digest(executable, "sha256").hexdigest()
    harnesses, cargo_cases = _cargo_cases(harness_artifacts, package_by_id, target_dir)
    output.write_json("test-artifacts.json", {"artifacts": harness_artifacts})
    if baseline_target != target_dir:
        baseline_artifacts, baseline_prepare = _prepare_artifacts(
            repo, output, workspace_members, baseline_target,
            Path(baseline_artifact_json) if baseline_artifact_json else None, "baseline",
        )
        for artifact in baseline_artifacts:
            artifact["package_name"] = package_by_id[artifact["package_id"]]["name"]
        baseline_harnesses, baseline_cases = _cargo_cases(baseline_artifacts, package_by_id, baseline_target)
        baseline_diff = _coverage_diff(_canonical_ids(cargo_cases), _canonical_ids(baseline_cases))
        if not baseline_diff["equal"]:
            raise ExperimentFailure(f"baseline/candidate test coverage differs: {baseline_diff}")
    else:
        baseline_harnesses, baseline_cases, baseline_prepare = harnesses, cargo_cases, candidate_prepare

    preflight_failures = []
    preflight_failed_arms = set()
    selected_nextest_ids = [arm for arm in selected_arm_ids if arm.startswith("nextest")]
    if selected_nextest_ids:
        try:
            nextest_cases, nextest_list, nextest_identity = _nextest_cases(
                repo, target_dir, output, harness_artifacts,
            )
            nextest_available = True
        except (ExperimentFailure, OSError, subprocess.SubprocessError) as error:
            nextest_cases, nextest_list, nextest_identity = [], {"error": str(error)}, None
            nextest_available = False
            preflight_failed_arms.update(selected_nextest_ids)
            preflight_failures.append({"reason": "nextest list unavailable", "details": str(error)})
        coverage = _coverage_diff(_canonical_ids(cargo_cases), _canonical_ids(nextest_cases))
        ignored_coverage = _coverage_diff(_ignored_ids(cargo_cases), _ignored_ids(nextest_cases))
    else:
        nextest_cases, nextest_list, nextest_identity = [], {"not_selected": True}, None
        nextest_available = False
        coverage = {"equal": True, "not_applicable": True, "expected_count": len(_canonical_ids(cargo_cases)),
                    "actual_count": 0, "missing": [], "unexpected": []}
        ignored_coverage = {"equal": True, "not_applicable": True,
                            "expected_count": len(_ignored_ids(cargo_cases)),
                            "actual_count": 0, "missing": [], "unexpected": []}
    output.write_json("test-coverage.json", {
        "cargo_active_ids": sorted(_canonical_ids(cargo_cases)),
        "cargo_ignored_ids": sorted(_ignored_ids(cargo_cases)),
        "nextest_ids": sorted(_canonical_ids(nextest_cases)),
        "nextest_ignored_ids": sorted(_ignored_ids(nextest_cases)),
        "nextest_listing": nextest_list,
        "nextest_selected": bool(selected_nextest_ids),
        "nextest_binary_identity_verified": nextest_identity is not None,
        "nextest_binary_identity_sha256": (nextest_identity["identity_sha256"]
                                            if nextest_identity else None),
        "coverage": coverage,
        "ignored_coverage": ignored_coverage,
    })
    if selected_nextest_ids and (not coverage["equal"] or not ignored_coverage["equal"]):
        output.append_record({"type": "coverage-mismatch", "active": coverage,
                              "ignored": ignored_coverage})
        preflight_failed_arms.update(selected_nextest_ids)
        preflight_failures.append({"reason": "Cargo and nextest active/ignored test ID sets differ",
                                   "coverage": coverage, "ignored_coverage": ignored_coverage})

    # Copy nextest binary IDs into matching harness cases for exact subset filters.
    nextest_by_id = {case["id"]: case for case in nextest_cases}
    for harness in harnesses:
        for case in harness["cases"]:
            if case["id"] in nextest_by_id:
                case["binary_id"] = nextest_by_id[case["id"]].get("binary_id")
    sensitive = _case_groups(harnesses)
    sensitive_cases = sorted({case["id"] for group in sensitive.values() for case in group["cases"]})
    category_counts = {}
    for group in sensitive.values():
        for category in group["categories"]:
            category_counts[category] = category_counts.get(category, 0) + len(group["cases"])
    required_categories = {"gpu-renderer", "text-cache", "filesystem"}
    missing_categories = sorted(required_categories - set(category_counts))
    output.write_json("sensitive-test-selection.json", {
        "policy": "renderer package plus text/cache and filesystem-sensitive module groups; exact IDs expanded from cargo --list",
        "category_case_counts": category_counts,
        "missing_required_categories": missing_categories,
        "groups": [{"package": group["package"], "binary": group["binary"], "filter": group["filter"],
                    "categories": sorted(group["categories"]), "ids": [case["id"] for case in group["cases"]]}
                   for group in sensitive.values()],
        "ids": sensitive_cases,
    })
    sensitive_expr = None
    if sensitive and nextest_available and not (preflight_failed_arms & {"nextest2", "nextest4"}):
        try:
            sensitive_expr = _nextest_filter_expression(list(sensitive.values()))
        except ExperimentFailure as error:
            preflight_failed_arms.update(selected_nextest_ids)
            preflight_failures.append({"reason": "nextest sensitive filter construction failed", "details": str(error)})
    sensitive_nextest = []
    if sensitive_expr and nextest_available and not (preflight_failed_arms & {"nextest2", "nextest4"}):
        try:
            sensitive_nextest, sensitive_listing, _sensitive_identity = _nextest_cases(
                repo, target_dir, output, harness_artifacts, sensitive_expr, "sensitive",
            )
            sensitive_diff = _coverage_diff(set(sensitive_cases), _canonical_ids(sensitive_nextest))
            output.write_json("sensitive-nextest-coverage.json", {
                "filter_expression": sensitive_expr,
                "listing": sensitive_listing,
                "coverage": sensitive_diff,
                "selected_ids": sorted(_canonical_ids(sensitive_nextest)),
            })
            if not sensitive_diff["equal"]:
                raise ExperimentFailure("nextest sensitive filter does not select the exact intended case set")
        except (ExperimentFailure, OSError, subprocess.SubprocessError) as error:
            preflight_failed_arms.update(selected_nextest_ids)
            preflight_failures.append({"reason": "nextest sensitive filter validation failed", "details": str(error)})

    cpu_cap = _available_cpus()
    requested_arms = [
        {"id": "baseline", "kind": "baseline", "requested_workers": 1},
        {"id": "cargo2", "kind": "cargo", "requested_workers": 2},
        {"id": "cargo4", "kind": "cargo", "requested_workers": 4},
        {"id": "nextest2", "kind": "nextest", "requested_workers": 2},
        {"id": "nextest4", "kind": "nextest", "requested_workers": 4},
    ]
    requested_arms = [arm for arm in requested_arms if arm["id"] in selected_arm_ids]
    effective_by_kind_and_cap = {}
    arms = []
    for arm in requested_arms:
        effective = min(arm["requested_workers"], cpu_cap)
        duplicate = effective_by_kind_and_cap.get((arm["kind"], effective))
        effective_by_kind_and_cap.setdefault((arm["kind"], effective), arm["id"])
        arms.append({**arm, "workers": effective, "duplicate_cap_of": duplicate})
    measurements = {arm["id"]: [] for arm in arms}
    failures = list(preflight_failures)
    if missing_categories:
        failures.append({"reason": "sensitive categories not discovered", "categories": missing_categories})
    failed_arms = set(preflight_failed_arms)
    baseline_docs_expected = None
    for round_number in range(1, warm_rounds + 1):
        order = arms if round_number % 2 == 1 else list(reversed(arms))
        for arm in order:
            arm_id = arm["id"]
            if arm_id in failed_arms:
                continue
            try:
                if arm["kind"] == "baseline":
                    result = _run_full_baseline(output, baseline_target, round_number, baseline_harnesses)
                else:
                    result = _run_dispatch(output, target_dir, arm_id, round_number, harnesses,
                                           arm["kind"], arm["workers"], nextest_cases=nextest_cases,
                                           nextest_identity=nextest_identity)
            except (ExperimentFailure, OSError, subprocess.SubprocessError, ValueError) as error:
                failed_arms.add(arm_id)
                failures.append({"arm": arm_id, "round": round_number,
                                 "reason": "candidate setup/dispatch failed", "details": str(error)})
                failed_record = {"arm": arm_id, "round": round_number, "exit_code": 1,
                                 "timed_out": False, "elapsed_seconds": 0.0,
                                 "peak_tree_rss_bytes": 0, "error": str(error)}
                output.append_record({"type": "runner-suite-failure", **failed_record})
                measurements[arm_id].append(failed_record)
                continue
            measurements[arm_id].append(result)
            if result.get("exit_code") or result.get("timed_out"):
                failed_arms.add(arm_id)
                failures.append({"arm": arm_id, "round": round_number,
                                 "reason": "timeout" if result.get("timed_out") else "test command failed",
                                 "log_path": result.get("log_path")})
                continue
            id_coverage = result.get("test_id_coverage")
            if not id_coverage or not id_coverage.get("equal"):
                failed_arms.add(arm_id)
                failures.append({"arm": arm_id, "round": round_number,
                                 "reason": "executed test ID set differs from expected", "coverage": id_coverage})
                continue
            if arm_id == "baseline":
                baseline_docs_expected = result.get("doc_ids", [])
            elif baseline_docs_expected is not None:
                doc_diff = _coverage_diff(set(baseline_docs_expected), set(result.get("doc_ids", [])))
                if not doc_diff["equal"]:
                    failed_arms.add(arm_id)
                    failures.append({"arm": arm_id, "round": round_number,
                                     "reason": "doctest coverage differs from baseline", "coverage": doc_diff})
            if result.get("peak_tree_rss_bytes", 0) <= 0:
                failures.append({"arm": arm_id, "round": round_number,
                                 "reason": "RSS sampler returned no measurement"})

    sensitive_rounds = []
    if sensitive_cases:
        selected_groups = list(sensitive.values())
        for arm in arms:
            arm_id = arm["id"]
            if arm_id in failed_arms:
                continue
            for repetition in range(1, 21):
                try:
                    if arm["kind"] == "baseline":
                        # Run the same focused module groups through bounded direct
                        # harnesses; whole-workspace Cargo has no union-of-filters API.
                        result = _run_dispatch(output, baseline_target, arm_id + "-sensitive", repetition,
                                               baseline_harnesses, "cargo", 1, selected_groups, docs=False)
                    else:
                        result = _run_dispatch(output, target_dir, arm_id + "-sensitive", repetition,
                                               harnesses, arm["kind"], arm["workers"], selected_groups,
                                               docs=False, nextest_cases=nextest_cases,
                                               nextest_identity=nextest_identity)
                except (ExperimentFailure, OSError, subprocess.SubprocessError, ValueError) as error:
                    result = {"exit_code": 1, "timed_out": False, "elapsed_seconds": 0.0,
                              "peak_tree_rss_bytes": 0, "error": str(error), "selected_ids": sensitive_cases}
                record = {"arm": arm_id, "round": repetition, **result}
                sensitive_rounds.append(record)
                output.append_record({"type": "runner-sensitive", **record})
                if result.get("exit_code") or result.get("timed_out"):
                    failed_arms.add(arm_id)
                    failures.append({"arm": arm_id, "sensitive_round": repetition,
                                     "reason": "sensitive subset failed or timed out",
                                     "log_path": result.get("log_path")})
                    break
                if set(result.get("selected_ids") or []) != set(sensitive_cases):
                    failed_arms.add(arm_id)
                    failures.append({"arm": arm_id, "sensitive_round": repetition,
                                     "reason": "sensitive selected-ID set mismatch"})
                    break

    cold = _run_cold(output, cold_rounds)
    summary = {}
    for arm in arms:
        values = measurements[arm["id"]]
        elapsed = [value["elapsed_seconds"] for value in values if not value.get("exit_code") and not value.get("timed_out")]
        rss = [value["peak_tree_rss_bytes"] for value in values if not value.get("exit_code") and not value.get("timed_out")]
        summary[arm["id"]] = {"successful_warm_samples": len(elapsed), "median_seconds": _median(elapsed),
                               "median_peak_tree_rss_bytes": _median(rss),
                               "max_peak_tree_rss_bytes": max(rss) if rss else None,
                               "failed_or_inconclusive": arm["id"] in failed_arms}
    base_time = summary["baseline"]["median_seconds"]
    base_rss = summary["baseline"]["max_peak_tree_rss_bytes"]
    qualified = []
    for arm in arms[1:]:
        candidate = summary[arm["id"]]
        if (not candidate["failed_or_inconclusive"] and base_time and base_rss
                and candidate["median_seconds"] is not None
                and candidate["max_peak_tree_rss_bytes"] is not None
                and candidate["median_seconds"] <= base_time * 0.90
                and candidate["max_peak_tree_rss_bytes"] <= base_rss * 1.10
                and len(measurements[arm["id"]]) == warm_rounds):
            qualified.append(arm["id"])
    sensitive_count_by_arm = {arm["id"]: sum(1 for row in sensitive_rounds if row["arm"] == arm["id"]
                                               and not row.get("exit_code") and not row.get("timed_out"))
                              for arm in arms}
    examples = run_timed(
        "runner-example-compile-coverage", EXAMPLES_COMMAND, output.repo,
        {"CARGO_TARGET_DIR": str(target_dir), "CARGO_INCREMENTAL": "0",
         "CARGO_TERM_COLOR": "never", "ANIMATIX_REQUIRE_GPU": "1"},
        output.logs_dir / "examples-compile-coverage.log", timeout_seconds=7200,
    )
    output.append_record({"type": "runner-examples-coverage", **examples})
    for artifact in harness_artifacts:
        with Path(artifact["executable"]).open("rb") as executable:
            actual_hash = hashlib.file_digest(executable, "sha256").hexdigest()
        if actual_hash != artifact["sha256"]:
            failures.append({"reason": "test executable changed during measurement",
                             "executable": artifact["executable"]})
    minimum_samples_met = warm_rounds >= 5 and cold_rounds >= 3
    sensitive_samples_met = all(sensitive_count_by_arm.get(arm_id, 0) >= 20 for arm_id in measurements)
    cold_coverage_met = len(cold) >= cold_rounds and all(
        not record["exit_code"] and not record["timed_out"] for record in cold
    )
    if len(cold) < cold_rounds or not cold_coverage_met:
        failures.append({"reason": "cold compile samples incomplete", "requested": cold_rounds,
                        "completed": len(cold)})
    if examples["exit_code"] or examples["timed_out"]:
        failures.append({"reason": "workspace examples compilation failed",
                         "exit_code": examples["exit_code"], "timed_out": examples["timed_out"],
                         "log_path": examples["log_path"]})
    if not minimum_samples_met:
        failures.append({"reason": "minimum warm/cold sample count not met",
                         "warm_rounds": warm_rounds, "cold_rounds": cold_rounds,
                         "required_warm": 5, "required_cold": 3})
    if not sensitive_samples_met:
        failures.append({"reason": "20-round sensitive subset requirement not met",
                         "successful_rounds_by_arm": sensitive_count_by_arm})
    successful = (not failures and not failed_arms and coverage["equal"]
                  and ignored_coverage["equal"] and len(measurements["baseline"]) == warm_rounds
                  and minimum_samples_met and sensitive_samples_met and cold_coverage_met
                  and examples["exit_code"] == 0 and not examples["timed_out"])
    coverage_complete = (coverage["equal"] and ignored_coverage["equal"]
                         and len(measurements["baseline"]) == warm_rounds
                         and all(len(results) == warm_rounds and all(
                             result.get("exit_code") == 0 and result.get("test_id_coverage", {}).get("equal")
                             for result in results
                         ) for results in measurements.values()))
    if not successful:
        # Do not surface any candidate as qualified unless all required coverage,
        # sample-count, RSS, documentation, and examples gates completed.
        qualified = []
    all_strategies_selected = selected_arm_ids == list(RUNNER_ARM_IDS)
    if successful:
        status = "complete" if all_strategies_selected else "complete-selected-cohort"
    else:
        status = "failed-or-inconclusive"
    if qualified and all_strategies_selected:
        decision = "candidate-meets-10-percent-and-rss-gates"
    elif qualified:
        decision = "selected-cohort-candidate-meets-10-percent-and-rss-gates"
    elif all_strategies_selected:
        decision = "retain-cargo-default"
    else:
        decision = "selected-cohort-has-no-qualifying-candidate"
    result = {
        "exit_code": 0 if successful else 1,
        "status": status,
        "selected_arms": selected_arm_ids,
        "available_arms": list(RUNNER_ARM_IDS),
        "partial_cohort": not all_strategies_selected,
        "selected_cohort_complete": successful,
        "all_strategies_complete": successful and all_strategies_selected,
        "coverage_complete": coverage_complete,
        "all_strategy_coverage_complete": coverage_complete and all_strategies_selected,
        "arms": arms,
        "warm_rounds_requested": warm_rounds,
        "cold_rounds_requested": cold_rounds,
        "cpu_cap": cpu_cap,
        "minimum_sample_requirements_met": minimum_samples_met,
        "sensitive_samples_by_arm": sensitive_count_by_arm,
        "sensitive_sample_requirements_met": sensitive_samples_met,
        "coverage": coverage,
        "ignored_coverage": ignored_coverage,
        "sensitive_case_count": len(sensitive_cases),
        "sensitive_category_case_counts": category_counts,
        "sensitive_rounds_completed": len(sensitive_rounds),
        "measurements": measurements,
        "cold_builds": cold,
        "examples_compile_coverage": examples,
        "summary": summary,
        "qualified_candidates": qualified,
        "decision": decision,
        "failures": failures,
        "limitations": [
            "Doctest coverage is compared by parsed rustdoc result IDs; if rustdoc emits no IDs, zero IDs are reported and that limitation is visible in records.",
            "The direct-harness fallback carries CARGO_TARGET_DIR and workspace target package metadata only through the compiled test binary; Cargo-specific runtime variables should be added if tests prove they are required.",
        ],
    }
    output.write_json("runner-summary.json", result)
    output.metadata["runner_experiment"]["summary"] = {
        "status": result["status"], "qualified_candidates": qualified,
        "selected_arms": selected_arm_ids,
        "all_strategies_complete": result["all_strategies_complete"],
        "coverage_equal": coverage["equal"], "failures": failures,
    }
    output.write_metadata()
    return result


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--execute-plan", type=Path)
    parser.add_argument("--result", type=Path)
    args = parser.parse_args(argv)
    if args.execute_plan:
        if not args.result:
            parser.error("--result is required with --execute-plan")
        return _execute_plan(args.execute_plan, args.result)
    parser.error("runner.py is invoked through scripts/experiments/run.py")
    return 2


if __name__ == "__main__":
    sys.exit(main())
