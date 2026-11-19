#!/usr/bin/env python3
"""Run the same native CI gates against two committed source snapshots.

Snapshots come from ``git archive`` into temporary directories. The current
checkout, index, branches, and worktree are never switched or rewritten. Each
side gets a separate Cargo target directory, while logs and a small JSON report
are kept under ``target/ci/migration-compare`` (or ``--output``).

Run from the required CI shell, for example:

    nix develop -c python3 tools/ci/compare-snapshot-gates.py \
      --baseline 204b668a --candidate <candidate-commit>

The default gate list is the existing native PR plan: check, test, no-video,
render-smoke, and examples. Use repeated ``--gate`` arguments to select a
smaller or larger set of gates that are native PR gates in both snapshots.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time
import uuid


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_GATES = ("check", "test", "no-video", "render-smoke", "examples")
SHA_RE = re.compile(r"^[0-9a-f]{40,64}$")


class SnapshotError(RuntimeError):
    """A reference, gate plan, or archive could not be used safely."""


def _run_git(repo: Path, *args: str, stdout=None) -> str:
    result = subprocess.run(
        ["git", *args], cwd=repo, stdout=stdout or subprocess.PIPE,
        stderr=subprocess.PIPE, text=stdout is None, check=False,
    )
    if result.returncode:
        detail = result.stderr.decode("utf-8", "replace") if isinstance(result.stderr, bytes) else result.stderr
        raise SnapshotError(f"git {' '.join(args)} failed: {detail.strip()}")
    if stdout is not None:
        return ""
    return result.stdout.strip()


def resolve_commit(repo: Path, ref: str) -> str:
    """Resolve a user ref to a full commit ID without changing repository state."""
    if not ref or ref.startswith("-"):
        raise SnapshotError(f"invalid Git reference: {ref!r}")
    commit = _run_git(repo, "rev-parse", "--verify", f"{ref}^{{commit}}")
    if not SHA_RE.fullmatch(commit):
        raise SnapshotError(f"Git returned an invalid commit ID for {ref!r}")
    return commit


def native_pr_gates(repo: Path) -> dict[str, dict[str, str]]:
    """Read the CI registry and return plan gates that run in the Nix shell."""
    result = subprocess.run(
        ["bash", "tools/ci/ci.sh", "--list"], cwd=repo,
        capture_output=True, text=True, check=False,
    )
    if result.returncode:
        raise SnapshotError(f"cannot read CI gate registry: {result.stderr.strip()}")
    gates: dict[str, dict[str, str]] = {}
    for line in result.stdout.splitlines():
        fields = line.split("\t")
        if len(fields) != 5:
            raise SnapshotError(f"malformed CI gate row: {line!r}")
        name, shell, runner, setup, stage = fields
        if shell == "nix" and stage == "pr":
            gates.setdefault(name, {"shell": shell, "runner": runner, "setup": setup, "stage": stage})
    if not gates:
        raise SnapshotError("the CI registry contains no native Nix PR gates")
    return gates


def extract_snapshot(repo: Path, commit: str, destination: Path) -> None:
    """Extract one exact committed tree with git archive; no worktree involved."""
    destination.mkdir(parents=True, exist_ok=False)
    archive_path = destination.parent / f"{destination.name}.tar"
    try:
        with archive_path.open("wb") as archive:
            result = subprocess.run(
                ["git", "archive", "--format=tar", commit], cwd=repo,
                stdout=archive, stderr=subprocess.PIPE, check=False,
            )
        if result.returncode:
            raise SnapshotError(
                f"git archive {commit} failed: {result.stderr.decode('utf-8', 'replace').strip()}"
            )
        subprocess.run(["tar", "-xf", str(archive_path), "-C", str(destination)], check=True)
    finally:
        archive_path.unlink(missing_ok=True)
    if (destination / ".git").exists():
        raise SnapshotError("git archive unexpectedly produced a .git directory")


def _tree_id(repo: Path, commit: str) -> str:
    return _run_git(repo, "rev-parse", "--verify", f"{commit}^{{tree}}")


def _output_dir(root: Path, requested: Path | None) -> Path:
    if requested is not None:
        output = requested if requested.is_absolute() else root / requested
    else:
        stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        output = root / "target" / "ci" / "migration-compare" / f"{stamp}-{os.getpid()}-{uuid.uuid4().hex[:8]}"
    output.mkdir(parents=True, exist_ok=False)
    return output.resolve()


def _write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def _tool_version(command: list[str]) -> dict:
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=5, check=False)
    except (OSError, subprocess.TimeoutExpired) as error:
        return {"available": False, "error": str(error)}
    return {
        "available": result.returncode == 0,
        "exit_code": result.returncode,
        "version": (result.stdout + result.stderr).strip()[:2048],
    }


def _sha256(path: Path) -> str | None:
    if not path.is_file():
        return None
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _run_gate(
    side: str,
    commit: str,
    source: Path,
    gate: str,
    target_dir: Path,
    output: Path,
) -> dict:
    side_output = output / side
    side_output.mkdir(parents=True, exist_ok=True)
    stdout_path = side_output / f"{gate}.stdout.log"
    stderr_path = side_output / f"{gate}.stderr.log"
    timing_path = side_output / "ci-timings.jsonl"
    lock_path = source / "Cargo.lock"
    lock_before = _sha256(lock_path)
    env = os.environ.copy()
    env.update({
        "CARGO_TARGET_DIR": str(target_dir.resolve()),
        "CARGO_TERM_COLOR": "never",
        "ANIMATIX_GATE_TIMINGS": str(timing_path),
        "GITHUB_SHA": commit,
        "GITHUB_JOB": f"snapshot-{side}",
    })
    command = ["bash", "tools/ci/ci.sh", "gate", gate]
    started = datetime.now(timezone.utc).isoformat()
    start = time.monotonic()
    timed_out = False
    try:
        with stdout_path.open("w", encoding="utf-8") as stdout, stderr_path.open("w", encoding="utf-8") as stderr:
            result = subprocess.run(
                command, cwd=source, env=env, stdout=stdout, stderr=stderr,
                check=False, timeout=7200,
            )
        exit_code = result.returncode
    except subprocess.TimeoutExpired:
        timed_out = True
        exit_code = 124
    lock_after = _sha256(lock_path)
    return {
        "gate": gate,
        "command": command,
        "started_at_utc": started,
        "elapsed_seconds": round(time.monotonic() - start, 3),
        "exit_code": exit_code,
        "timed_out": timed_out,
        "lockfile_unchanged": lock_before == lock_after,
        "lockfile_sha256_before": lock_before,
        "lockfile_sha256_after": lock_after,
        "stdout": str(stdout_path.relative_to(output)),
        "stderr": str(stderr_path.relative_to(output)),
    }


def compare(
    repo: Path,
    baseline_ref: str,
    candidate_ref: str,
    gates: tuple[str, ...],
    output: Path | None = None,
    prepare_only: bool = False,
) -> tuple[int, Path]:
    repo = repo.resolve()
    baseline = resolve_commit(repo, baseline_ref)
    candidate = resolve_commit(repo, candidate_ref)
    if baseline == candidate:
        raise SnapshotError("baseline and candidate resolve to the same commit")
    if not gates or len(set(gates)) != len(gates):
        raise SnapshotError("provide one or more distinct gates")

    output_dir = _output_dir(repo, output)
    report = {
        "schema_version": 1,
        "started_at_utc": datetime.now(timezone.utc).isoformat(),
        "repository": str(repo),
        "baseline": {"requested_ref": baseline_ref, "commit": baseline, "tree": _tree_id(repo, baseline)},
        "candidate": {"requested_ref": candidate_ref, "commit": candidate, "tree": _tree_id(repo, candidate)},
        "gates": list(gates),
        "prepare_only": prepare_only,
        "host": {"platform": platform.platform(), "python": platform.python_version()},
        "tools": {
            "git": _tool_version(["git", "--version"]),
            "rustc": _tool_version([os.environ.get("RUSTC", "rustc"), "--version", "--verbose"]),
            "cargo": _tool_version(["cargo", "--version"]),
            "nix": _tool_version(["nix", "--version"]),
        },
        "shell": {
            "animatix_ci_shell": os.environ.get("ANIMATIX_CI_SHELL"),
            "in_nix_shell": bool(os.environ.get("IN_NIX_SHELL")),
        },
        "sides": {"baseline": [], "candidate": []},
    }
    _write_json(output_dir / "report.json", report)

    with tempfile.TemporaryDirectory(prefix="animatix-git-snapshots-") as temporary:
        temp = Path(temporary)
        snapshots: dict[str, Path] = {}
        identities = {"baseline": baseline, "candidate": candidate}
        for side, commit in identities.items():
            snapshot = temp / side
            extract_snapshot(repo, commit, snapshot)
            snapshots[side] = snapshot
            registry = native_pr_gates(snapshot)
            missing = [gate for gate in gates if gate not in registry]
            if missing:
                raise SnapshotError(f"{side} snapshot does not define native PR gates: {', '.join(missing)}")
        report["gate_definitions"] = {
            side: {gate: native_pr_gates(snapshot)[gate] for gate in gates}
            for side, snapshot in snapshots.items()
        }
        if prepare_only:
            report["status"] = "prepared"
        else:
            for side, commit in identities.items():
                # Example fixtures and the render smoke resolve binaries under
                # the snapshot's target directory, as in a regular checkout.
                target_dir = snapshots[side] / "target"
                target_dir.mkdir(parents=True, exist_ok=True)
                for gate in gates:
                    record = _run_gate(side, commit, snapshots[side], gate, target_dir, output_dir)
                    report["sides"][side].append(record)
                    _write_json(output_dir / "report.json", report)
                    if not record["lockfile_unchanged"]:
                        print(f"snapshot comparison: {side}/{gate} changed Cargo.lock; recorded as failed", file=sys.stderr)
            report["status"] = "passed" if all(
                record["exit_code"] == 0 and not record["timed_out"] and record["lockfile_unchanged"]
                for records in report["sides"].values() for record in records
            ) else "failed"
    report["finished_at_utc"] = datetime.now(timezone.utc).isoformat()
    _write_json(output_dir / "report.json", report)
    print(f"snapshot comparison: {report['status']} ({output_dir / 'report.json'})")
    return (0 if report["status"] in {"prepared", "passed"} else 1), output_dir


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True, help="baseline Git commit or ref")
    parser.add_argument("--candidate", required=True, help="candidate Git commit or ref")
    parser.add_argument("--gate", action="append", dest="gates",
                        help="native PR gate to run; repeatable; defaults to check/test/no-video/render-smoke/examples")
    parser.add_argument("--output", type=Path, help="new output directory for report and logs")
    parser.add_argument("--prepare-only", action="store_true",
                        help="verify both archives and gate registries without running Cargo gates")
    parser.add_argument("--repo", type=Path, default=ROOT, help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    try:
        code, _ = compare(
            args.repo, args.baseline, args.candidate,
            tuple(args.gates or DEFAULT_GATES), args.output, args.prepare_only,
        )
        return code
    except (OSError, SnapshotError, subprocess.CalledProcessError) as error:
        print(f"snapshot comparison failed: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
