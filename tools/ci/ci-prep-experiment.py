#!/usr/bin/env python3
"""Observe isolated CI preparation experiments; never participate in publishing."""
from __future__ import annotations

from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import signal
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]
JOBS = {"web-build", "check-lint", "test-examples"}
ARMS = {"a", "b", "c"}
ROUNDS = {"cold", "warm", "sample1", "sample2"}
LABELS = {"before", "after-cleanup", "after-cache", "before-gate", "after-gate"}
MIN_FREE = 30 * 1024**3
CLEAN_PATHS = (
    "/usr/share/dotnet", "/opt/ghc", "/usr/local/lib/android",
    "/usr/local/share/boost", "/usr/local/share/chromium",
)
SHELLS = {"nix": [], "web": [".#web-build"], "web-ci": [".#web-ci"]}


def context():
    values = {name: os.environ.get("ANIMATIX_PREP_" + name.upper(), "")
              for name in ("job", "arm", "round")}
    for name, allowed in (("job", JOBS), ("arm", ARMS), ("round", ROUNDS)):
        if values[name] not in allowed:
            raise ValueError(f"invalid experiment {name}")
    if experiment_mode() == "cleanup-only" and values["arm"] == "c":
        raise ValueError("cleanup-only mode cannot select the cache arm")
    return values


def matrix_rows():
    output = subprocess.check_output(
        ["bash", "tools/ci/ci.sh", "--matrix", "pr"], cwd=ROOT, text=True,
    )
    rows = json.loads(output)["include"]
    chosen = [row for row in rows if row.get("id") in JOBS]
    if len(chosen) != len(JOBS) or {row["id"] for row in chosen} != JOBS:
        raise ValueError("registered experiment jobs are missing or duplicated")
    for row in chosen:
        if row.get("os") != "ubuntu-latest" or row.get("shell") not in SHELLS:
            raise ValueError("experiment requires registered Linux Nix jobs")
        if not isinstance(row.get("gate"), str) or not re.fullmatch(r"[a-z0-9 -]+", row["gate"]):
            raise ValueError("invalid registered gate sequence")
        if row.get("stage") != "pr":
            raise ValueError("experiment may not select release gates")
    return chosen


def registered_job(ctx):
    row = next(row for row in matrix_rows() if row["id"] == ctx["job"])
    # Derive execution from the registry; supplied matrix hints may not disagree.
    for key, env_names in (("gate", ("ANIMATIX_PREP_GATE", "GATE")),
                           ("shell", ("ANIMATIX_PREP_SHELL", "GATE_SHELL"))):
        for env in env_names:
            if env in os.environ and os.environ[env] != row[key]:
                raise ValueError("experiment matrix differs from gate registry")
    return row


def nix_command(row, command):
    return ["nix", "develop", *SHELLS[row["shell"]], "--command", *command]


def output_value(name, value):
    if "\n" in value or "\r" in value:
        raise ValueError("multiline output is forbidden")
    destination = os.environ.get("GITHUB_OUTPUT")
    if destination:
        with open(destination, "a", encoding="utf-8") as stream:
            stream.write(f"{name}={value}\n")


def record(action, ctx, data):
    directory = ROOT / "target/ci/prep-experiment"
    directory.mkdir(parents=True, exist_ok=True)
    payload = {"schema": 1, "action": action, **ctx,
               "recorded_at": datetime.now(timezone.utc).isoformat(), **data}
    path = directory / f"{action}-{uuid.uuid4().hex}.json"
    with path.open("x", encoding="utf-8") as stream:
        json.dump(payload, stream, sort_keys=True)
        stream.write("\n")


def disk():
    stats = os.statvfs(ROOT)
    return {"available_bytes": stats.f_bavail * stats.f_frsize,
            "total_bytes": stats.f_blocks * stats.f_frsize}


def cleanup_guard():
    required = {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted",
                "RUNNER_OS": "Linux", "GITHUB_REPOSITORY": "j4ger/animatix"}
    if platform.system() != "Linux" or any(os.environ.get(k) != v for k, v in required.items()):
        raise ValueError("cleanup is restricted to GitHub-hosted Linux experiment runners")
    ref = os.environ.get("GITHUB_REF", "")
    if not re.fullmatch(r"refs/heads/ci-prep-experiment/[A-Za-z0-9][A-Za-z0-9._/-]*", ref):
        raise ValueError("cleanup requires an isolated ci-prep-experiment branch")


def cleanup(ctx):
    cleanup_guard()
    before = disk()
    selected = ctx["arm"] == "a" or before["available_bytes"] < MIN_FREE
    started = time.monotonic()
    status = 0
    after = None
    try:
        if selected:
            subprocess.run(["sudo", "rm", "-rf", "--", *CLEAN_PATHS], check=True)
            subprocess.run(["sudo", "apt-get", "clean"], check=True)
        after = disk()
        # Baseline A is the exact current preparation, not a new disk gate.
        if ctx["arm"] != "a" and after["available_bytes"] < MIN_FREE:
            status = 1
        return status
    except (OSError, subprocess.CalledProcessError):
        status = 1
        raise
    finally:
        record("cleanup", ctx, {"selected": selected, "threshold_bytes": MIN_FREE,
               "before": before, "after": after, "exit_code": status,
               "elapsed_seconds": time.monotonic() - started})


def identity(ctx):
    row = registered_job(ctx)
    started = time.monotonic()
    rust = subprocess.check_output(nix_command(row, ["rustc", "-Vv"]), cwd=ROOT)
    cargo = subprocess.check_output(nix_command(row, ["cargo", "-Vv"]), cwd=ROOT)
    lock = hashlib.sha256((ROOT / "flake.lock").read_bytes()).hexdigest()
    parts = {"rustc_sha256": hashlib.sha256(rust).hexdigest(),
             "cargo_sha256": hashlib.sha256(cargo).hexdigest(), "flake_lock_sha256": lock,
             "shell": row["shell"], "job": ctx["job"], "machine": platform.machine()}
    digest = hashlib.sha256(json.dumps(parts, sort_keys=True).encode()).hexdigest()
    output_value("identity", digest)
    releases = {}
    for name, raw in (("rustc", rust), ("cargo", cargo)):
        match = re.search(rb"(?m)^release: ([0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.]+)?)$", raw)
        if match:
            releases[name + "_release"] = match[1].decode("ascii")
    record("identity", ctx, {**parts, **releases, "identity": digest,
           "elapsed_seconds": time.monotonic() - started})
    print(digest)
    return 0


def runner_identity():
    fields = {
        "GITHUB_SHA": ("sha", r"[0-9a-f]{40}(?:[0-9a-f]{24})?"),
        "GITHUB_RUN_ID": ("run_id", r"[0-9]{1,24}"),
        "GITHUB_RUN_ATTEMPT": ("run_attempt", r"[0-9]{1,8}"),
        "ImageOS": ("image_os", r"[A-Za-z0-9._-]{1,40}"),
        "ImageVersion": ("image_version", r"[A-Za-z0-9._-]{1,64}"),
    }
    result = {}
    for env, (key, pattern) in fields.items():
        value = os.environ.get(env)
        if value is not None:
            if not re.fullmatch(pattern, value):
                raise ValueError("invalid public runner identity")
            result[key] = value
    return result


def snapshot(ctx):
    label = os.environ.get("ANIMATIX_PREP_LABEL", "before")
    if label not in LABELS:
        raise ValueError("invalid snapshot label")
    hit = os.environ.get("ANIMATIX_PREP_CACHE_HIT", "")
    if hit not in {"", "true", "false"}:
        raise ValueError("invalid cache verdict")
    record("snapshot", ctx, {"label": label, "disk": disk(),
           "cache_hit": hit or "unknown", **runner_identity(),
           "cpu_count": os.cpu_count(), "system": platform.system(),
           "machine": platform.machine()})
    return 0


def run_gate(ctx):
    row = registered_job(ctx)
    command = nix_command(row, ["tools/ci/ci.sh", "gate", row["gate"]])
    started = time.monotonic()
    minimum = None
    samples = 0
    sample_errors = 0
    status = 1
    received = []
    child = None

    def sample():
        nonlocal minimum, samples, sample_errors
        try:
            available = disk()["available_bytes"]
            minimum = available if minimum is None else min(minimum, available)
            samples += 1
        except OSError:
            sample_errors += 1

    def forward(signum, _frame):
        received.append(signum)
        if child is not None:
            try:
                os.killpg(child.pid, signum)
            except ProcessLookupError:
                pass

    previous = {sig: signal.getsignal(sig) for sig in (signal.SIGINT, signal.SIGTERM)}
    try:
        sample()
        for sig in previous:
            signal.signal(sig, forward)
        child = subprocess.Popen(command, cwd=ROOT, start_new_session=True)
        for sig in list(received):
            try:
                os.killpg(child.pid, sig)
            except ProcessLookupError:
                pass
        while True:
            try:
                status = child.wait(timeout=5)
                break
            except subprocess.TimeoutExpired:
                sample()
        sample()
        # Shell convention preserves the reason if the child died by a signal.
        if status < 0:
            status = 128 - status
        if received:
            status = 128 + received[-1]
        return status
    finally:
        for sig, handler in previous.items():
            signal.signal(sig, handler)
        try:
            record("run", ctx, {"exit_code": status, "elapsed_seconds": time.monotonic() - started,
                   "minimum_available_bytes": minimum, "disk_samples": samples,
                   "disk_sample_errors": sample_errors, "signals": sorted(set(received)),
                   "gate_count": len(row["gate"].split()), "shell": row["shell"]})
        except OSError:
            print("warning: run evidence unavailable; gate result is unchanged", file=sys.stderr)


def experiment_mode():
    mode = os.environ.get("ANIMATIX_PREP_MODE", "cleanup-only")
    if mode not in {"cleanup-only", "cache-pilot"}:
        raise ValueError("invalid experiment mode")
    return mode


def experiment_scope():
    scope = os.environ.get("ANIMATIX_PREP_SCOPE", "pilot")
    if scope not in {"pilot", "critical"}:
        raise ValueError("invalid experiment scope")
    return scope


def preflight():
    cleanup_guard()
    scope = experiment_scope()
    raw = subprocess.check_output(
        ["gh", "api", "repos/j4ger/animatix/actions/cache/usage"], cwd=ROOT,
    )
    usage = json.loads(raw)
    size = usage.get("active_caches_size_in_bytes")
    count = usage.get("active_caches_count")
    if type(size) is not int or type(count) is not int or size < 0 or count < 0:
        raise ValueError("invalid cache usage response")
    # Conservative local experiment budget, not a claim about account quota.
    maximum_existing = 5 * 1024**3
    mode = experiment_mode()
    allowed = mode == "cleanup-only" or size <= maximum_existing
    record("preflight", {"scope": scope, "mode": mode}, {"active_cache_bytes": size,
           "active_cache_count": count, "maximum_existing_bytes": maximum_existing,
           "allowed": allowed})
    print(json.dumps({"mode": mode, "active_cache_bytes": size, "active_cache_count": count,
                      "maximum_existing_bytes": maximum_existing, "allowed": allowed}))
    return 0 if allowed else 1


def main():
    action = os.environ.get("ANIMATIX_PREP_ACTION", "")
    if action == "preflight":
        return preflight()
    if action == "matrix":
        scope = experiment_scope()
        arms = {"a", "b"} if experiment_mode() == "cleanup-only" else ARMS
        rows = [{**row, "arm": arm} for row in matrix_rows()
                if scope == "critical" or row["id"] == "check-lint"
                for arm in sorted(arms)]
        value = json.dumps({"include": rows}, separators=(",", ":"))
        output_value("matrix", value)
        print(value)
        return 0
    actions = {"cleanup": cleanup, "identity": identity, "snapshot": snapshot, "run": run_gate}
    if action not in actions:
        raise ValueError("unknown preparation experiment action")
    return actions[action](context())


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError, StopIteration):
        # Do not echo arbitrary environment values, child output or local paths.
        print("CI preparation experiment failed; check runner and registered inputs", file=sys.stderr)
        sys.exit(1)
