#!/usr/bin/env python3
"""Run opt-in build/linker experiments with isolated targets and saved evidence."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform as host_platform
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from scripts.experiments.common import (
    ExperimentFailure, ExperimentOutput, run_timed, source_fingerprint,
)
from scripts.experiments.compat import run_linker_compatibility
from scripts.experiments.linkers import (
    MOLD_SOURCE_REV, collect_elf_linker_evidence, collect_wrapper_evidence,
    linker_arms, prepare_mold_source, validate_revision,
)


BUILD_COMMAND = [
    "cargo", "test", "--workspace", "--no-run", "--locked",
    "--message-format=json-render-diagnostics",
]
LINKER_WORKLOADS = ("cli", "gui", "workspace-tests")


def _workload_command(workload: str, platform: str) -> list[str]:
    profile = ["--profile", "dist"] if platform == "macos" and workload in {"cli", "gui"} else []
    if workload == "cli":
        return ["cargo", "build", "--locked", *profile,
                "--message-format=json-render-diagnostics", "-p", "animatix-cli", "--bin", "animatix"]
    if workload == "gui":
        return ["cargo", "build", "--locked", *profile,
                "--message-format=json-render-diagnostics", "-p", "animatix-gui", "--bin", "animatix-gui"]
    if workload == "workspace-tests":
        return BUILD_COMMAND
    raise ExperimentFailure(f"unknown linker workload: {workload}")


def _selected_linker_workloads(requested: list[str] | None = None,
                               environment_value: str | None = None) -> list[str]:
    if requested:
        selected = requested
    elif environment_value and environment_value.strip().lower() != "all":
        selected = [item.strip() for item in environment_value.split(",") if item.strip()]
    else:
        selected = list(LINKER_WORKLOADS)
    if not selected or len(selected) != len(set(selected)):
        raise ExperimentFailure("linker workloads must be a nonempty list without duplicates")
    unknown = sorted(set(selected) - set(LINKER_WORKLOADS))
    if unknown:
        raise ExperimentFailure(f"unknown linker workload(s): {', '.join(unknown)}")
    return [workload for workload in LINKER_WORKLOADS if workload in selected]


def _command_text(command: list[str]) -> dict:
    try:
        result = subprocess.run(command, capture_output=True, text=True, check=False, timeout=15)
        return {"command": command, "exit_code": result.returncode,
                "output": (result.stdout + result.stderr).strip()}
    except (OSError, subprocess.TimeoutExpired) as error:
        return {"command": command, "exit_code": None, "output": str(error)}


def _git_revision(repo: Path) -> str | None:
    result = subprocess.run(
        ["git", "-C", str(repo), "rev-parse", "HEAD"], capture_output=True,
        text=True, check=False,
    )
    return result.stdout.strip() if result.returncode == 0 else None


def _cpu_model() -> str | None:
    if host_platform.system() == "Darwin":
        result = _command_text(["sysctl", "-n", "machdep.cpu.brand_string"])
        return result["output"] if result["exit_code"] == 0 else None
    try:
        for line in Path("/proc/cpuinfo").read_text(encoding="utf-8", errors="replace").splitlines():
            if line.lower().startswith(("model name", "hardware")):
                return line.split(":", 1)[-1].strip()
    except OSError:
        pass
    return host_platform.processor() or None


def _ram_bytes() -> int | None:
    if host_platform.system() == "Darwin":
        result = _command_text(["sysctl", "-n", "hw.memsize"])
        try:
            return int(result["output"]) if result["exit_code"] == 0 else None
        except ValueError:
            return None
    try:
        for line in Path("/proc/meminfo").read_text(encoding="ascii").splitlines():
            if line.startswith("MemTotal:"):
                return int(line.split()[1]) * 1024
    except (OSError, ValueError, IndexError):
        pass
    return None


def _machine_info(repo: Path, experiment: str, platform: str, rounds: dict,
                  workloads: tuple[str, ...] | list[str] = LINKER_WORKLOADS) -> dict:
    lock = repo / "flake.lock"
    rustup = _command_text(["rustup", "show", "active-toolchain"])
    try:
        affinity = sorted(os.sched_getaffinity(0))
    except (AttributeError, OSError):
        affinity = None
    try:
        load_average = list(os.getloadavg())
    except (AttributeError, OSError):
        load_average = None
    mac_host = host_platform.system() == "Darwin"
    try:
        disk = shutil.disk_usage(repo)
        disk_facts = {"total_bytes": disk.total, "used_bytes": disk.used,
                      "free_bytes": disk.free}
    except OSError as error:
        disk_facts = {"error": str(error)}
    return {
        "schema": 1,
        "experiment": experiment,
        "recorded_at_utc": datetime.now(timezone.utc).isoformat(),
        "repository": str(repo),
        "animatix_revision": os.environ.get("GITHUB_SHA") or _git_revision(repo),
        "platform": platform,
        "host": {
            "system": host_platform.system(),
            "release": host_platform.release(),
            "machine": host_platform.machine(),
            "processor": host_platform.processor(),
            "cpu_model": _cpu_model(),
            "python": sys.version,
            "cpu_count": os.cpu_count(),
            "cpu_affinity": affinity,
            "load_average_1_5_15": load_average,
            "ram_bytes": _ram_bytes(),
            "disk": disk_facts,
            "github_image_os": os.environ.get("ImageOS"),
            "github_image_version": os.environ.get("ImageVersion"),
            "uname": list(host_platform.uname()),
        },
        "toolchain": {
            "rustc": _command_text(["rustc", "-Vv"]),
            "cargo": _command_text(["cargo", "--version"]),
            "rustup_active": rustup,
            "nix": _command_text(["nix", "--version"]),
            "in_nix_shell": os.environ.get("IN_NIX_SHELL"),
            "nix_shell": os.environ.get("ANIMATIX_CI_SHELL"),
            "xcodebuild": _command_text(["xcodebuild", "-version"]) if mac_host else None,
            "macos_sdk_version": _command_text(["xcrun", "--show-sdk-version"]) if mac_host else None,
            "flake_lock_path": str(lock) if lock.exists() else None,
            "flake_lock_sha256": _sha256(lock) if lock.exists() else None,
        },
        "rounds": rounds,
        "workloads": {name: _workload_command(name, platform) for name in workloads},
        "execution": {
            "parallel_measurements": 1,
            "cargo_incremental": "0",
            "cold_target_definition": "fresh owned CARGO_TARGET_DIR per workload and linker arm; Cargo registry and toolchain caches may be warm",
            "warm_target_definition": "one warmed owned target per workload and linker arm; each measured run removes only that workload's output executable(s) to force a warm relink",
            "workspace_target_is_never_cleaned": True,
        },
    }


def _sha256(path: Path) -> str:
    import hashlib
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _executables_from_cargo_log(log_path: Path, target: Path) -> list[Path]:
    executables = set()
    for line in log_path.read_text(encoding="utf-8", errors="replace").splitlines():
        try:
            record = json.loads(line)
        except json.JSONDecodeError:
            continue
        if record.get("reason") != "compiler-artifact":
            continue
        executable = record.get("executable")
        if not executable:
            continue
        candidate = Path(executable).resolve()
        if not candidate.is_relative_to(target.resolve()):
            raise ExperimentFailure(f"Cargo reported an executable outside owned target: {candidate}")
        executables.add(candidate)
    return sorted(executables)


def _select_workload_executables(executables: list[Path], target: Path,
                                 platform: str, workload: str) -> list[Path]:
    binary = {"cli": "animatix", "gui": "animatix-gui"}.get(workload)
    if binary is None:
        return executables
    profile = "dist" if platform == "macos" else "debug"
    expected = (target / profile / binary).resolve()
    return [path for path in executables if path.resolve() == expected]


def _run_cargo_build(output: ExperimentOutput, platform: str, phase: str, arm: dict,
                     workload: str, command: list[str], target: Path,
                     round_number: int | None, timed: bool) -> dict:
    run_name = f"{platform}-{workload}-{phase}-{arm['id']}" + (f"-{round_number}" if round_number is not None else "")
    audit_log = output.logs_dir / f"{run_name}.linker-audit.jsonl"
    env = {
        **arm["env"],
        "CARGO_TARGET_DIR": str(target),
        "CARGO_INCREMENTAL": "0",
        "CARGO_TERM_COLOR": "never",
        "ANIMATIX_LINKER_AUDIT_LOG": str(audit_log) if arm.get("audit") else None,
    }
    run = run_timed(
        run_name, command, output.repo, env,
        output.logs_dir / f"{run_name}.log", timeout_seconds=7200,
    )
    run.update({
        "platform": platform,
        "workload": workload,
        "phase": phase,
        "arm": arm["id"],
        "round": round_number,
        "timed_measurement": timed,
        "target_dir": str(target),
        "rustflags": arm["env"].get("RUSTFLAGS"),
        "expected_linker": arm.get("expected_linker"),
    })
    output.append_record(run)
    if run["exit_code"] or run["timed_out"]:
        raise ExperimentFailure(f"Cargo {phase} failed for {arm['id']}; see {run['log_path']}")

    executables = _executables_from_cargo_log(Path(run["log_path"]), target)
    executables = _select_workload_executables(executables, target, platform, workload)
    if not executables:
        raise ExperimentFailure(f"Cargo produced no executable artifacts for {workload}/{arm['id']}; see {run['log_path']}")
    run["executable_artifacts"] = [str(path) for path in executables]
    run["executable_sha256"] = [
        {"path": str(path), "sha256": _sha256(path)} for path in executables
    ]
    run["audit_log_path"] = str(audit_log) if arm.get("audit") else None
    if platform == "linux":
        if arm.get("audit"):
            artifact_evidence = collect_elf_linker_evidence(
                executables, arm["artifact_identity"], allow_unknown=True,
            )
            audit_evidence = collect_wrapper_evidence(
                audit_log, arm["audit"], executables,
            )
            evidence = {"artifacts": artifact_evidence, "actual_linker_invocations": audit_evidence}
        else:
            evidence = collect_elf_linker_evidence(executables, arm["expected_linker"])
    elif arm.get("audit"):
        evidence = collect_wrapper_evidence(audit_log, arm["audit"], executables)
    else:
        expected_path = arm.get("expected_linker_path")
        lines = Path(run["log_path"]).read_text(encoding="utf-8", errors="replace").splitlines()
        evidence = [line for line in lines if expected_path and expected_path in line]
        if not evidence:
            raise ExperimentFailure(
                f"Apple ld path {expected_path!r} was not observed in clang's executed-linker output"
            )
    run["linker_evidence"] = evidence
    output.append_record({"type": "linker-evidence", "platform": platform,
                          "workload": workload, "phase": phase, "arm": arm["id"], "round": round_number,
                          "evidence": evidence})
    return run


def _prime_warm_targets(output: ExperimentOutput, platform: str, workload: str,
                        arms: list[dict], failures: list[dict]) -> dict[str, dict]:
    warm_targets = {}
    command = _workload_command(workload, platform)
    for arm in arms:
        target = output.owned_target(
            f"build-targets/{platform}/{workload}/warm/{arm['id']}", reset=True,
        )
        try:
            run = _run_cargo_build(output, platform, "warmup", arm, workload,
                                   command, target, None, False)
            warm_targets[arm["id"]] = {
                "target": target,
                "executables": [Path(path) for path in run["executable_artifacts"]],
                "audit_log_path": run.get("audit_log_path"),
                "build_log": run["log_path"],
            }
            output.append_record({"type": "warm-target", "platform": platform,
                                  "workload": workload, "arm": arm["id"],
                                  "target_dir": str(target),
                                  "executables": run["executable_artifacts"],
                                  "executable_sha256": run["executable_sha256"]})
        except (ExperimentFailure, OSError, subprocess.SubprocessError) as error:
            failure = {"arm": arm["id"], "workload": workload,
                       "phase": "warmup", "reason": str(error)}
            failures.append(failure)
            output.append_record({"type": "failure", "platform": platform, **failure})
    return warm_targets


def _stage_compatibility_binary(output: ExperimentOutput, platform: str, arm_id: str,
                                workload: str, source: dict) -> dict:
    binary_name = {"cli": "animatix", "gui": "animatix-gui"}[workload]
    staged_dir = output.directory / "compatibility" / platform / arm_id / "binaries"
    staged_dir.mkdir(parents=True, exist_ok=True)
    staged_binary = staged_dir / binary_name
    if staged_binary.exists() or staged_binary.is_symlink():
        raise ExperimentFailure(f"compatibility binary already exists: {staged_binary}")
    try:
        os.link(source["binary"], staged_binary)
        staging_method = "hardlink"
    except OSError:
        shutil.copy2(source["binary"], staged_binary)
        staging_method = "copy"
    staged_hash = _sha256(staged_binary)
    if source.get("sha256") and staged_hash != source["sha256"]:
        raise ExperimentFailure(f"staged {workload} binary hash changed for {arm_id}")
    return {
        "binary_path": str(staged_binary),
        "link_output_path": str(source["binary"]),
        "audit_log_path": source.get("audit_log_path"),
        "build_log": source["build_log"],
        "sha256": staged_hash,
        "staging_method": staging_method,
        "workload": workload,
        "measured_build": bool(source.get("measured_build")),
    }


def _ensure_compatibility_targets(output: ExperimentOutput, platform: str,
                                  arms: list[dict], targets: dict[str, dict[str, dict]],
                                  failures: list[dict]) -> None:
    """Build only the unmeasured app counterpart needed for complete smoke checks."""
    for arm in arms:
        per_arm = targets.setdefault(arm["id"], {})
        for workload in ("cli", "gui"):
            if workload in per_arm:
                continue
            target_rel = f"compatibility-build-targets/{platform}/{workload}/{arm['id']}"
            target = output.owned_target(target_rel, reset=True)
            try:
                record = _run_cargo_build(
                    output, platform, "compatibility-build", arm, workload,
                    _workload_command(workload, platform), target, None, False,
                )
                source = {
                    "binary": record["executable_artifacts"][0],
                    "audit_log_path": record.get("audit_log_path"),
                    "build_log": record["log_path"],
                    "sha256": record["executable_sha256"][0]["sha256"],
                    "measured_build": False,
                }
                per_arm[workload] = _stage_compatibility_binary(
                    output, platform, arm["id"], workload, source,
                )
            except (ExperimentFailure, OSError, subprocess.SubprocessError) as error:
                failure = {"arm": arm["id"], "workload": workload,
                           "phase": "compatibility-build", "reason": str(error)}
                failures.append(failure)
                output.append_record({"type": "failure", "platform": platform, **failure})
            finally:
                output.owned_target(target_rel, reset=True)


def _measure_linker(output: ExperimentOutput, platform: str, arms: list[dict],
                    warm_rounds: int, cold_rounds: int,
                    workloads: tuple[str, ...] | list[str] = LINKER_WORKLOADS) -> dict:
    failures = []
    compat_targets: dict[str, dict[str, dict]] = {arm["id"]: {} for arm in arms}
    measured = []
    for workload in workloads:
        command = _workload_command(workload, platform)
        warm_targets = _prime_warm_targets(output, platform, workload, arms, failures) if warm_rounds else {}
        failed_arms = {failure["arm"] for failure in failures if failure.get("workload") == workload}
        compatibility_sources: dict[str, dict] = {}
        for index in range(max(warm_rounds, cold_rounds)):
            arm_order = arms if index % 2 == 0 else list(reversed(arms))
            phases = ["warm", "cold"] if index % 2 == 0 else ["cold", "warm"]
            for phase in phases:
                if phase == "warm" and index >= warm_rounds:
                    continue
                if phase == "cold" and index >= cold_rounds:
                    continue
                for arm in arm_order:
                    if arm["id"] in failed_arms:
                        continue
                    round_number = index + 1
                    if phase == "warm":
                        item = warm_targets.get(arm["id"])
                        if not item:
                            continue
                        target = item["target"]
                        removed = []
                        for executable in item["executables"]:
                            if executable.exists():
                                executable.unlink()
                                removed.append(str(executable))
                        if not removed:
                            failure = {"arm": arm["id"], "workload": workload,
                                       "phase": phase, "round": round_number,
                                       "reason": "warm relink had no owned executables to remove"}
                            failures.append(failure)
                            failed_arms.add(arm["id"])
                            output.append_record({"type": "failure", "platform": platform, **failure})
                            continue
                    else:
                        target = output.owned_target(
                            f"build-targets/{platform}/{workload}/cold/{arm['id']}", reset=True,
                        )
                        removed = []
                    try:
                        record = _run_cargo_build(
                            output, platform, phase, arm, workload, command,
                            target, round_number, True,
                        )
                        record["removed_owned_executables"] = removed
                        measured.append(record)
                        if workload in {"cli", "gui"} and phase == "warm":
                            compatibility_sources[arm["id"]] = {
                                "binary": record["executable_artifacts"][0],
                                "audit_log_path": record.get("audit_log_path"),
                                "build_log": record["log_path"],
                                "sha256": record["executable_sha256"][0]["sha256"],
                            }
                        output.append_record({"type": "measurement-detail", "platform": platform,
                                              "workload": workload, "phase": phase,
                                              "arm": arm["id"], "round": round_number,
                                              "removed_owned_executables": removed,
                                              "executable_sha256": record["executable_sha256"],
                                              "elapsed_seconds": record["elapsed_seconds"],
                                              "peak_tree_rss_bytes": record["peak_tree_rss_bytes"],
                                              "exit_code": record["exit_code"]})
                        if phase == "cold":
                            output.owned_target(
                                f"build-targets/{platform}/{workload}/cold/{arm['id']}", reset=True,
                            )
                    except (ExperimentFailure, OSError, subprocess.SubprocessError) as error:
                        failure = {"arm": arm["id"], "workload": workload,
                                   "phase": phase, "round": round_number,
                                   "reason": str(error)}
                        failures.append(failure)
                        failed_arms.add(arm["id"])
                        output.append_record({"type": "failure", "platform": platform, **failure})
                        if phase == "cold":
                            output.owned_target(
                                f"build-targets/{platform}/{workload}/cold/{arm['id']}", reset=True,
                            )

        if workload in {"cli", "gui"}:
            for arm_id, source in compatibility_sources.items():
                try:
                    compat_targets[arm_id][workload] = _stage_compatibility_binary(
                        output, platform, arm_id, workload,
                        {**source, "measured_build": True},
                    )
                except (ExperimentFailure, OSError) as error:
                    failure = {"arm": arm_id, "workload": workload,
                               "phase": "compatibility-stage", "reason": str(error)}
                    failures.append(failure)
                    output.append_record({"type": "failure", "platform": platform, **failure})

        # Targets are strictly marker-owned and reset only after output binaries,
        # linker audit logs, and artifact hashes have been preserved.
        for arm_id in arms:
            warm_rel = f"build-targets/{platform}/{workload}/warm/{arm_id['id']}"
            if warm_rounds:
                output.owned_target(warm_rel, reset=True)
    return {
        "arms": [arm["id"] for arm in arms],
        "workloads": list(workloads),
        "warm_rounds": warm_rounds,
        "cold_rounds": cold_rounds,
        "failures": failures,
        "measurement_order": [
            {"round": record["round"], "phase": record["phase"],
             "workload": record["workload"], "arm": record["arm"]}
            for record in measured
        ],
        "measurements": measured,
        "compatibility_targets": compat_targets,
    }


def run_linker_experiment(repo: Path, output_path: Path, platform: str,
                          warm_rounds: int, cold_rounds: int,
                          mold_source_rev: str = MOLD_SOURCE_REV,
                          workloads: tuple[str, ...] | list[str] = LINKER_WORKLOADS) -> dict:
    if warm_rounds < 1 or cold_rounds < 0:
        raise ExperimentFailure("warm_rounds must be at least 1 and cold_rounds nonnegative")
    workloads = _selected_linker_workloads(list(workloads))
    if platform == "macos":
        mold_source_rev = validate_revision(mold_source_rev)
    output = ExperimentOutput(
        repo, output_path, "linker",
        _machine_info(repo, "linker", platform, {"warm": warm_rounds, "cold": cold_rounds}, workloads),
    )
    failures = []
    result = {"arms": [], "workloads": list(workloads),
              "warm_rounds": warm_rounds, "cold_rounds": cold_rounds,
              "failures": failures, "measurements": [], "measurement_order": []}
    with output.machine_lock():
        fingerprint_before = source_fingerprint(repo)
        output.metadata["source_fingerprint_before"] = fingerprint_before
        output.metadata["mold_candidate"] = {"source_revision": mold_source_rev if platform == "macos" else None}
        output.write_metadata()
        mold_binary = None
        if platform == "macos":
            try:
                mold_alias, provenance = prepare_mold_source(repo, output, mold_source_rev)
                # Keep the binary path separate from the required argv0 alias.
                mold_binary = Path(provenance["binary_path"])
                provenance["argv0_alias"] = str(mold_alias)
                output.metadata["mold_candidate"] = provenance
            except (ExperimentFailure, OSError, subprocess.SubprocessError) as error:
                failure = {"arm": "ld64-mold", "phase": "setup", "reason": str(error)}
                failures.append(failure)
                output.append_record({"type": "failure", "platform": platform, **failure})
                output.metadata["mold_candidate"]["status"] = "failed"
                output.metadata["mold_candidate"]["failure"] = str(error)
        try:
            arms, inventory, setup_failures = linker_arms(platform, output, mold_binary)
            failures.extend(setup_failures)
            for failure in setup_failures:
                output.append_record({"type": "failure", "platform": platform, "phase": "setup", **failure})
            output.metadata["linkers"] = inventory
            output.metadata["linker_arms"] = [
                {"id": arm["id"], "rustflags": arm["env"].get("RUSTFLAGS"),
                 "expected_linker": arm.get("expected_linker"),
                 "artifact_identity": arm.get("artifact_identity"),
                 "audit": arm.get("audit")}
                for arm in arms
            ]
            output.write_metadata()
            try:
                result = _measure_linker(output, platform, arms, warm_rounds, cold_rounds, workloads)
                failures.extend(result["failures"])
                compatibility = []
                compatibility_targets = result.get("compatibility_targets", {})
                _ensure_compatibility_targets(
                    output, platform, arms, compatibility_targets, failures,
                )
                for arm in arms:
                    targets = compatibility_targets.get(arm["id"], {})
                    try:
                        if not all(name in targets for name in ("cli", "gui")):
                            raise ExperimentFailure(
                                "CLI/GUI workload produced no owned compatibility targets"
                            )
                        compatibility.append(run_linker_compatibility(
                            output, repo, platform, arm, targets,
                        ))
                    except (ExperimentFailure, OSError, subprocess.SubprocessError, ValueError) as error:
                        failure = {"arm": arm["id"], "phase": "compatibility", "reason": str(error)}
                        failures.append(failure)
                        output.append_record({"type": "failure", "platform": platform, **failure})
                        compatibility.append({"status": "failed", **failure})
                result["compatibility"] = compatibility
                result["failures"] = failures
                result.pop("compatibility_targets", None)
            except (ExperimentFailure, OSError, subprocess.SubprocessError) as error:
                failure = {"phase": "measurement", "reason": str(error)}
                failures.append(failure)
                output.append_record({"type": "failure", "platform": platform, **failure})
        except (ExperimentFailure, OSError, subprocess.SubprocessError) as error:
            failure = {"phase": "candidate-setup", "reason": str(error)}
            failures.append(failure)
            output.append_record({"type": "failure", "platform": platform, **failure})
        fingerprint_after = source_fingerprint(repo)
        output.metadata["source_fingerprint_after"] = fingerprint_after
        if fingerprint_before != fingerprint_after:
            failure = {"phase": "source-integrity", "reason": "workspace source changed during the experiment"}
            failures.append(failure)
            output.append_record({"type": "failure", "platform": platform, **failure})
        output.metadata["failures"] = failures
        output.metadata["completed_at_utc"] = datetime.now(timezone.utc).isoformat()
        output.metadata["result"] = {
            "measurement_count": len(result.get("measurements", [])),
            "arms": result.get("arms", []),
            "warm_rounds": warm_rounds,
            "cold_rounds": cold_rounds,
            "compatibility": result.get("compatibility", []),
            "compatibility_scope": (
                "CLI help/version and PNG image render; GUI help/version and visible window launch; "
                "macOS additionally checks native dependencies, app bundle/icon, ZIP, and packaged GUI launch"
            ),
        }
        output.write_metadata()
    if failures:
        raise ExperimentFailure(f"linker experiment recorded {len(failures)} failure(s); inspect {output.metadata_path}")
    return result


def run_runner(repo: Path, output_path: Path, warm_rounds: int, cold_rounds: int,
               build_artifact_json: Path | None = None) -> dict:
    from scripts.experiments.runner import run_runner_experiment
    output = ExperimentOutput(
        repo, output_path, "runner",
        _machine_info(repo, "runner", "linux", {"warm": warm_rounds, "cold": cold_rounds}),
    )
    with output.machine_lock():
        before = source_fingerprint(repo)
        output.metadata["source_fingerprint_before"] = before
        output.metadata["build_artifact_json"] = str(build_artifact_json) if build_artifact_json else None
        output.write_metadata()
        try:
            result = run_runner_experiment(
                repo, output, warm_rounds, cold_rounds,
                build_artifact_json=build_artifact_json,
            )
        except Exception as error:
            output.metadata["source_fingerprint_after"] = source_fingerprint(repo)
            output.metadata["failure"] = str(error)
            output.metadata["completed_at_utc"] = datetime.now(timezone.utc).isoformat()
            output.write_metadata()
            raise
        after = source_fingerprint(repo)
        output.metadata["source_fingerprint_after"] = after
        output.metadata["completed_at_utc"] = datetime.now(timezone.utc).isoformat()
        output.metadata["result"] = result
        if before != after:
            output.metadata["failure"] = "workspace source changed during the experiment"
            result["exit_code"] = 1
        if result.get("exit_code", 0) or result.get("timed_out") or result.get("coverage_complete") is False:
            output.metadata["status"] = "failed"
        output.write_metadata()
    return result


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="experiment", required=True)
    linker = commands.add_parser("linker")
    linker.add_argument("--platform", choices=("linux", "macos"), required=True)
    linker.add_argument("--output", type=Path, required=True)
    linker.add_argument("--warm-rounds", type=int, default=5)
    linker.add_argument("--cold-rounds", type=int, default=3)
    linker.add_argument("--rounds", type=int, help=argparse.SUPPRESS)
    linker.add_argument("--mold-source-rev", default=os.environ.get("ANIMATIX_MOLD_SOURCE_REV", MOLD_SOURCE_REV))
    linker.add_argument("--workload", choices=LINKER_WORKLOADS, action="append",
                        help="measure only this workload; repeat for multiple workloads")
    runner = commands.add_parser("runner")
    runner.add_argument("--output", type=Path, required=True)
    runner.add_argument("--warm-rounds", type=int, default=5)
    runner.add_argument("--cold-rounds", type=int, default=3)
    runner.add_argument("--rounds", type=int, help=argparse.SUPPRESS)
    runner.add_argument("--build-artifact-json", type=Path,
                        default=Path(os.environ["ANIMATIX_EXPERIMENT_BUILD_JSON"])
                        if os.environ.get("ANIMATIX_EXPERIMENT_BUILD_JSON") else None)
    args = parser.parse_args(argv)
    if args.rounds is not None:
        args.warm_rounds = args.rounds

    try:
        if args.experiment == "linker":
            workloads = _selected_linker_workloads(
                args.workload, os.environ.get("ANIMATIX_LINKER_WORKLOADS"),
            )
            result = run_linker_experiment(
                ROOT, args.output, args.platform, args.warm_rounds, args.cold_rounds,
                args.mold_source_rev, workloads,
            )
        else:
            result = run_runner(ROOT, args.output, args.warm_rounds, args.cold_rounds,
                                args.build_artifact_json)
        print(json.dumps({"experiment": args.experiment, "result": result}, sort_keys=True))
        if args.experiment == "runner" and (
                result.get("exit_code", 0) or result.get("timed_out")
                or result.get("coverage_complete") is False):
            return int(result.get("exit_code") or 1)
        return 0
    except (ExperimentFailure, OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"experiment {args.experiment} failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
