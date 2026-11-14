"""Cargo test artifact inventory and bounded libtest process scheduling."""

from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor, as_completed
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import threading
import time


class WorkspaceTestError(RuntimeError):
    """The Cargo test inventory or its execution plan is invalid."""


TEST_LIST_RE = re.compile(r"^(.*?)\s*:\s*test\s*$")
LIBTEST_RESULT_RE = re.compile(r"^test\s+(.+?)\s+\.\.\.\s+(ok|FAILED|ignored)\s*$")

_ACTIVE_HARNESS_GROUPS: dict[int, subprocess.Popen] = {}
_ACTIVE_HARNESS_LOCK = threading.RLock()
_DISPATCH_SHUTTING_DOWN = False


def parse_cargo_test_artifacts(path: Path, workspace_members: set[str],
                               target_dir: Path | None = None) -> list[dict]:
    """Read Cargo compiler JSON and return every executable workspace test target."""
    artifacts = []
    by_identity = {}
    target_root = Path(target_dir).resolve() if target_dir is not None else None
    for line_number, line in enumerate(Path(path).read_text(encoding="utf-8", errors="replace").splitlines(), 1):
        if not line.strip():
            continue
        try:
            record = json.loads(line)
        except json.JSONDecodeError as error:
            raise WorkspaceTestError(
                f"invalid Cargo JSON at {path}:{line_number}: {error.msg}"
            ) from error
        if not isinstance(record, dict) or record.get("reason") != "compiler-artifact":
            continue
        if record.get("package_id") not in workspace_members:
            continue
        target = record.get("target", {})
        profile = record.get("profile", {})
        if not target.get("test") or not profile.get("test"):
            continue
        if not record.get("executable"):
            raise WorkspaceTestError(
                f"Cargo test target has no executable at {path}:{line_number}: "
                f"{record.get('package_id')}::{target.get('name')}"
            )
        executable = Path(record["executable"]).resolve()
        if not executable.is_file():
            raise WorkspaceTestError(f"Cargo test executable is missing: {executable}")
        if target_root is not None and not executable.is_relative_to(target_root):
            raise WorkspaceTestError(f"test executable is outside the selected target directory: {executable}")
        artifact = {
            "package_id": record["package_id"], "package_name": None,
            "target_name": target.get("name", executable.name),
            "features": record.get("features", []), "profile": profile,
            "target_kind": target.get("kind", []),
            "manifest_path": record.get("manifest_path"),
            "executable": str(executable),
        }
        key = (artifact["package_id"], artifact["target_name"], str(executable))
        prior = by_identity.get(key)
        if prior is not None:
            compared = ("features", "profile", "target_kind", "manifest_path")
            if any(prior[field] != artifact[field] for field in compared):
                raise WorkspaceTestError(f"conflicting duplicate Cargo test artifact: {key}")
            continue
        by_identity[key] = artifact
        artifacts.append(artifact)
    if not artifacts:
        raise WorkspaceTestError(f"no executable workspace test artifacts found in {path}")
    return artifacts


def parse_test_list(text: str) -> list[str]:
    names = []
    for line in text.splitlines():
        match = TEST_LIST_RE.match(line.strip())
        if match:
            names.append(match.group(1))
    return names


def parse_libtest_results(log_path: Path) -> list[dict]:
    results = []
    for line in Path(log_path).read_text(encoding="utf-8", errors="replace").splitlines():
        match = LIBTEST_RESULT_RE.match(line.strip())
        if match:
            results.append({"name": match.group(1), "status": match.group(2)})
    return results


def package_maps(metadata: dict) -> tuple[set[str], dict[str, dict]]:
    members = set(metadata["workspace_members"])
    packages = {package["id"]: package for package in metadata["packages"] if package["id"] in members}
    if members - packages.keys():
        raise WorkspaceTestError("Cargo metadata omits one or more workspace member package records")
    return members, packages


def build_test_inventory(artifacts: list[dict], package_by_id: dict[str, dict],
                         target_dir: Path | None = None) -> tuple[list[dict], list[dict]]:
    """List each compiled harness once and map cases to package::target::test IDs."""
    cases: list[dict] = []
    harnesses = []
    for artifact in artifacts:
        package = package_by_id[artifact["package_id"]]
        executable = Path(artifact["executable"])
        list_env = {**os.environ, "ANIMATIX_REQUIRE_GPU": "1"}
        if target_dir:
            list_env["CARGO_TARGET_DIR"] = str(target_dir)
        listed = subprocess.run(
            [str(executable), "--list", "--format", "terse"], cwd=Path(package["manifest_path"]).parent,
            env=list_env, capture_output=True, text=True, check=False, timeout=60,
        )
        if listed.returncode:
            raise WorkspaceTestError(f"test listing failed for {executable}: {listed.stderr[-1000:]}")
        ignored = subprocess.run(
            [str(executable), "--ignored", "--list", "--format", "terse"],
            cwd=Path(package["manifest_path"]).parent,
            env=list_env, capture_output=True, text=True, check=False, timeout=60,
        )
        if ignored.returncode:
            raise WorkspaceTestError(f"ignored-test listing failed for {executable}: {ignored.stderr[-1000:]}")
        names = parse_test_list(listed.stdout)
        ignored_names = set(parse_test_list(ignored.stdout))
        package_name = package["name"]
        target_name = artifact["target_name"]
        active = []
        for name in names:
            case = {"id": f"{package_name}::{target_name}::{name}", "package": package_name,
                    "binary": target_name, "name": name, "ignored": name in ignored_names}
            cases.append(case)
            if not case["ignored"]:
                active.append(case)
        harnesses.append({
            **artifact, "package_name": package_name,
            "manifest_dir": str(Path(package["manifest_path"]).parent),
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
    ids = [case["id"] for case in cases]
    if len(ids) != len(set(ids)):
        duplicates = sorted({case_id for case_id in ids if ids.count(case_id) > 1})
        raise WorkspaceTestError(f"duplicate canonical test IDs: {duplicates[:12]}")
    return harnesses, cases


def _signal_process_group(process_group: int, sig: int) -> None:
    try:
        os.killpg(process_group, sig)
    except ProcessLookupError:
        pass


def _terminate_process_group(process: subprocess.Popen, grace_seconds: float = 1.0) -> None:
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
    """Terminate all nested child sessions when the parent is interrupted."""
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


def run_logged_command(command: list[str], cwd: Path, env: dict[str, str],
                       stdout_path: Path, stderr_path: Path,
                       timeout_seconds: float = 7200) -> dict:
    """Run a Cargo phase with separate raw logs and process-group cleanup."""
    stdout_path.parent.mkdir(parents=True, exist_ok=True)
    stderr_path.parent.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    process = None
    timed_out = False
    error = None
    status = 127

    def interrupted(signum, _frame):
        if process is not None:
            _terminate_process_group(process)
        raise SystemExit(128 + signum)

    previous = {}
    for sig in (signal.SIGTERM, signal.SIGINT):
        previous[sig] = signal.getsignal(sig)
        signal.signal(sig, interrupted)
    try:
        with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
            process = subprocess.Popen(command, cwd=cwd, env=env, stdout=stdout, stderr=stderr,
                                       start_new_session=True)
            try:
                status = process.wait(timeout=timeout_seconds)
            except subprocess.TimeoutExpired:
                timed_out = True
                error = f"timed out after {timeout_seconds:g} seconds"
                _terminate_process_group(process)
                status = 124
            else:
                # Cargo may leave rustc helpers behind after its own exit.
                _signal_process_group(process.pid, signal.SIGTERM)
                time.sleep(0.02)
                _signal_process_group(process.pid, signal.SIGKILL)
    except OSError as exception:
        error = str(exception)
        status = 127
    finally:
        if process is not None and process.poll() is None:
            _terminate_process_group(process)
        for sig, handler in previous.items():
            signal.signal(sig, handler)

    return {"command": command, "exit_code": status, "timed_out": timed_out,
            "elapsed_seconds": time.monotonic() - started, "error": error,
            "stdout_path": str(stdout_path), "stderr_path": str(stderr_path)}


def execute_harness_item(item: dict, env: dict[str, str], log_dir: Path, run_tag: str,
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
                _terminate_process_group(process)
                status = 124
            else:
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
    observed = parse_libtest_results(log_path) if log_path.exists() else []
    executed_ids = sorted(by_name[row["name"]] for row in observed
                          if row["status"] in {"ok", "FAILED"} and row["name"] in by_name)
    ignored_ids = sorted(by_name[row["name"]] for row in observed
                         if row["status"] == "ignored" and row["name"] in by_name)
    return {"package": item["package_name"], "binary": item["target_name"],
            "filter": filter_text, "command": command, "log_path": str(log_path),
            "stderr_log_path": str(stderr_log_path), "exit_code": status,
            "elapsed_seconds": time.monotonic() - started, "timed_out": timed_out,
            "error": error, "executed_ids": executed_ids, "ignored_ids": ignored_ids,
            "observed_test_results": observed}


def execute_harness_plan(plan_path: Path, result_path: Path) -> int:
    plan = json.loads(plan_path.read_text(encoding="utf-8"))
    env = {**os.environ, **plan.get("env", {}), "ANIMATIX_REQUIRE_GPU": "1"}
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
            for filter_text in item.get("filters") or [None]:
                results.append(execute_harness_item(item, env, log_dir, plan["run_tag"], filter_text))
        max_workers = max(1, int(plan.get("max_workers", 1)))
        jobs = [(item, filter_text) for item in parallel
                for filter_text in (item.get("filters") or [None])]
        if plan.get("progress"):
            for item, _ in jobs:
                print(f"[workspace-tests] queued {item['package_name']}::{item['target_name']}", flush=True)
        if jobs:
            with ThreadPoolExecutor(max_workers=max_workers) as pool:
                futures = {pool.submit(execute_harness_item, item, env, log_dir,
                                       plan["run_tag"], filter_text): (item, filter_text)
                           for item, filter_text in jobs}
                for future in as_completed(futures):
                    result = future.result()
                    results.append(result)
                    if plan.get("progress"):
                        status = "PASS" if result["exit_code"] == 0 else "FAIL"
                        print(f"[workspace-tests] {status} {result['package']}::{result['binary']} "
                              f"({result['elapsed_seconds']:.2f}s)", flush=True)
    finally:
        for sig, handler in prior_handlers.items():
            signal.signal(sig, handler)
    status = 1 if any(result["exit_code"] for result in results) else 0
    result_path.write_text(json.dumps({"exit_code": status, "harnesses": results},
                                      indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return status
