#!/usr/bin/env python3
"""Measure gates without changing shell error handling; validate CI completion."""

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time

from ci_topology import artifact, transform


def environment_snapshot(directory):
    """Keep runner/tool facts next to timings, including cold topology trials."""
    def probe(command):
        try:
            result = subprocess.run(command, capture_output=True, text=True, timeout=5)
            return {"command": command, "exit_code": result.returncode,
                    "output": (result.stdout + result.stderr).strip()}
        except (OSError, subprocess.TimeoutExpired) as error:
            return {"command": command, "error": str(error)}
    revision = probe(["git", "rev-parse", "HEAD"])
    commit = revision.get("output", "unknown") if revision.get("exit_code") == 0 else "unknown"
    label = "-".join((os.environ.get("GITHUB_RUN_ID", "local"),
                      os.environ.get("GITHUB_JOB", "gate"), commit[:12]))
    path = directory / f"environment-{label}.json"
    if path.exists():
        return path.name
    cpu = platform.processor()
    if Path("/proc/cpuinfo").is_file():
        cpu = next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines()
                    if line.startswith("model name")), cpu)
    try:
        ram = os.sysconf("SC_PHYS_PAGES") * os.sysconf("SC_PAGE_SIZE")
    except (AttributeError, OSError, ValueError):
        ram = None
    if ram is None:
        command = (["sysctl", "-n", "hw.memsize"] if platform.system() == "Darwin" else
                   ["powershell", "-NoProfile", "-Command",
                    "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory"])
        try:
            ram = int(probe(command).get("output", ""))
        except ValueError:
            pass
    facts = {"commit": commit, "image": {name: os.environ.get(name) for name in
             ("ImageOS", "ImageVersion", "RUNNER_OS", "RUNNER_ARCH", "RUNNER_NAME")},
             "uname": list(platform.uname()), "cpu_model": cpu, "cpu_count": os.cpu_count(),
             "ram_bytes": ram, "shell": os.environ.get("ANIMATIX_CI_SHELL"),
             "cargo_environment": {key: value for key, value in os.environ.items()
                                   if key.startswith(("CARGO_PROFILE_", "CARGO_BUILD_"))},
             "tools": {name: probe(command) for name, command in {
                 "rustc": [os.environ.get("RUSTC", "rustc"), "-vV"],
                 "cargo": ["cargo", "--version"], "nix": ["nix", "--version"],
             }.items()}}
    directory.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(facts, indent=2) + "\n")
    return path.name


def timed_gate(gate, command):
    """Run an independent shell so wrapping a function never disables errexit."""
    if command[:1] == ["--"]:
        command = command[1:]
    if not command:
        raise ValueError("missing gate command")
    started = datetime.now(timezone.utc).isoformat()
    before = time.monotonic()
    destination = Path(os.environ.get("ANIMATIX_GATE_TIMINGS", "target/ci/timings.jsonl"))
    environment_file = environment_snapshot(destination.parent)
    try:
        result = subprocess.run(command, check=False)
        status = result.returncode
        status = status if status >= 0 else 128 - status
    except OSError as error:
        print(f"cannot start gate {gate}: {error}", file=sys.stderr)
        status = 127
    record = {
        "gate": gate, "started_at": started,
        "finished_at": datetime.now(timezone.utc).isoformat(),
        "elapsed_seconds": time.monotonic() - before, "exit_code": status,
        "commit": os.environ.get("GITHUB_SHA"),
        "job": os.environ.get("GITHUB_JOB"),
        "environment_file": environment_file,
    }
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("a", encoding="utf-8") as output:
        output.write(json.dumps(record, sort_keys=True) + "\n")
    print(f"gate {gate}: exit={status} elapsed={record['elapsed_seconds']:.3f}s", flush=True)
    return status


def require_success(needs, topology="baseline"):
    """A cancelled, failed, missing or skipped selected matrix cannot pass."""
    optional = {"native_cli", "shared_render"}
    if set(needs) - optional != {"plan", "gate"}:
        raise ValueError("CI Required expects results from both plan and gate")
    selected = {name: job for name, job in needs.items() if name not in optional}
    if topology == "shared-cli":
        if not optional.issubset(needs):
            raise ValueError("shared-cli topology is missing its producer or consumer")
        selected.update({name: needs[name] for name in optional})
    elif any(needs[name].get("result") != "skipped" for name in optional & needs.keys()):
        raise ValueError("unexpected artifact job outside the shared-cli experiment")
    failures = {name: job.get("result") for name, job in selected.items()
                if job.get("result") != "success"}
    if failures:
        raise ValueError(f"selected CI jobs did not succeed: {failures}")
    print("CI Required: plan and every selected matrix job succeeded")



def github_plan(policy_path):
    policy = json.loads(Path(policy_path).read_text())
    full = json.loads(subprocess.run(
        ["bash", "scripts/ci.sh", "--matrix", "pr"], check=True,
        capture_output=True, text=True).stdout)
    jobs = full["include"]
    known = {job["id"] for job in jobs}
    selected = known if policy.get("force_full") else set(policy["candidate_job_ids"])
    if not selected or selected - known:
        raise ValueError(f"planner returned empty/unknown jobs: {sorted(selected - known)}")
    selection = os.environ.get("ANIMATIX_CI_SELECTION", "shadow")
    if selection not in {"shadow", "enabled"}:
        raise ValueError("ANIMATIX_CI_SELECTION must be shadow or enabled")
    chosen = jobs if selection == "shadow" else [job for job in jobs if job["id"] in selected]
    topology = os.environ.get("ANIMATIX_CI_TOPOLOGY", "baseline")
    producer_gates = next((job["gate"] for job in chosen if job["id"] == "test-examples"), "") if topology == "shared-cli" else ""
    chosen, render_gates = transform(chosen, topology)
    matrix = {"include": chosen}
    policy.update(effective_selection=selection, effective_topology=topology,
                  effective_job_ids=[job["id"] for job in chosen],
                  effective_matrix=matrix)
    Path(policy_path).write_text(json.dumps(policy, indent=2) + "\n")
    Path("target/ci").mkdir(parents=True, exist_ok=True)
    Path("target/ci/matrix.json").write_text(json.dumps(matrix, indent=2) + "\n")
    print(f"CI selection={selection}; planned={len(selected)}; executed={len(chosen)}")
    output = os.environ.get("GITHUB_OUTPUT")
    if output:
        with open(output, "a", encoding="utf-8") as stream:
            stream.write("matrix=" + json.dumps(matrix, separators=(",", ":")) + "\n")
            stream.write(f"topology={topology}\nshared_render_gates={render_gates}\nnative_cli_gates={producer_gates}\n")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="mode", required=True)
    timer = commands.add_parser("time")
    timer.add_argument("--gate", required=True)
    timer.add_argument("command", nargs=argparse.REMAINDER)
    commands.add_parser("required")
    plan = commands.add_parser("plan")
    plan.add_argument("policy")
    for name in ("stage-cli", "verify-cli"):
        action = commands.add_parser(name)
        action.add_argument("source")
        action.add_argument("destination", nargs="?", default=".")
    arguments = parser.parse_args()
    try:
        if arguments.mode == "time":
            return timed_gate(arguments.gate, arguments.command)
        if arguments.mode == "plan":
            return github_plan(arguments.policy)
        if arguments.mode in {"stage-cli", "verify-cli"}:
            artifact(arguments.mode, arguments.source, arguments.destination)
            return 0
        require_success(json.loads(os.environ.get("ANIMATIX_CI_NEEDS", "{}")),
                        os.environ.get("ANIMATIX_CI_TOPOLOGY", "baseline"))
        return 0
    except (ValueError, TypeError, AttributeError) as error:
        print(f"ci-control: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
