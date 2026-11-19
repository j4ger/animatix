#!/usr/bin/env python3
"""Read-only staged dependency checks for the Vello GPU migration.

The checks use `cargo tree --locked` with explicit package/edge/target scopes.
The minimal-engine and wasm checks inspect only their resolved package graph;
they do not fail just because another workspace target still has an old package
in Cargo.lock. The final full-no-legacy check covers every workspace package,
feature, dependency kind, and target, then also checks the complete lockfile.

Examples:

    python3 tools/ci/dependency-gates.py --stage minimal-engine
    python3 tools/ci/dependency-gates.py --stage wasm-no-legacy
    python3 tools/ci/dependency-gates.py --stage full-no-legacy

No files are changed. Cargo is invoked with `--locked`; reports are emitted as
JSON to stdout or written to `--output`.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass, replace
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib


ROOT = Path(__file__).resolve().parents[2]
TREE_FORMAT = "{p}\t{f}"
PACKAGE_RE = re.compile(r"^([A-Za-z0-9_.+-]+) v([^ ]+)(?: |$)")
LEGACY_FIRST_PARTY_PACKAGES = frozenset({"animatix-std", "animatix-hybrid-bench"})
DEVICE_ONLY_PACKAGES = frozenset({"animatix-render", "wgpu", "wgpu-core", "wgpu-hal", "wgpu-types"})
NEW_VELLO_GPU_STACK = {
    ("vello_gpu", "0.4.0"),
    ("vello_gpu_shaders", "0.4.0"),
    ("vello_common", "0.4.0"),
}


@dataclass(frozen=True)
class Stage:
    name: str
    scope: str
    require_device_neutral: bool = False
    allow_vello_gpu: bool = False
    package: str | None = None
    target: str = "all"
    edges: str = "normal"
    no_default_features: bool = False
    all_features: bool = False
    workspace: bool = False
    inspect_lockfile: bool = False


STAGES = {
    "minimal-engine": Stage(
        name="minimal-engine",
        scope="animatix normal dependency graph, no default features, all targets",
        require_device_neutral=True,
        package="animatix",
        no_default_features=True,
    ),
    "wasm-no-legacy": Stage(
        name="wasm-no-legacy",
        scope="animatix-web normal/build dependency graph for wasm32-unknown-unknown",
        package="animatix-web",
        target="wasm32-unknown-unknown",
        edges="normal,build",
        allow_vello_gpu=True,
    ),
    "full-no-legacy": Stage(
        name="full-no-legacy",
        scope="all workspace packages, all features, dependency kinds, and targets",
        target="all",
        # Cargo's "all" also emits feature nodes that have no package columns.
        edges="normal,build,dev",
        all_features=True,
        workspace=True,
        inspect_lockfile=True,
        allow_vello_gpu=True,
    ),
}


class DependencyGateError(RuntimeError):
    """Dependency metadata was malformed or Cargo could not resolve it."""


def cargo_tree_command(stage: Stage, cargo: str = "cargo") -> list[str]:
    command = [cargo, "tree", "--locked"]
    command += ["--workspace"] if stage.workspace else ["--package", stage.package]
    if stage.no_default_features:
        command.append("--no-default-features")
    if stage.all_features:
        command.append("--all-features")
    command += ["--edges", stage.edges, "--target", stage.target,
                "--format", TREE_FORMAT, "--prefix", "none", "--color", "never"]
    return command


def parse_tree_output(output: str) -> list[dict[str, object]]:
    """Parse the stable tab-delimited Cargo tree format into package rows."""
    packages: dict[tuple[str, str, str], dict[str, object]] = {}
    for line_number, line in enumerate(output.splitlines(), start=1):
        if not line.strip():
            continue
        if "\t" not in line:
            raise DependencyGateError(f"cargo tree output line {line_number} has no tab delimiter")
        package, features = line.split("\t", 1)
        match = PACKAGE_RE.match(package.strip())
        if not match:
            raise DependencyGateError(f"cargo tree output line {line_number} has an invalid package row")
        name, version = match.groups()
        package_id = package.strip()
        key = (name, version, package_id)
        packages.setdefault(key, {
            "name": name,
            "version": version,
            "package_id": package_id,
            "features": sorted(set(filter(None, features.strip().split(",")))),
        })
    if not packages:
        raise DependencyGateError("cargo tree returned no package rows")
    return [packages[key] for key in sorted(packages)]


def lockfile_packages(lockfile: Path) -> list[dict[str, str]]:
    try:
        data = tomllib.loads(lockfile.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise DependencyGateError(f"cannot read Cargo.lock: {error}") from error
    rows = data.get("package")
    if not isinstance(rows, list):
        raise DependencyGateError("Cargo.lock has no package table array")
    packages = []
    for row in rows:
        if (not isinstance(row, dict) or not isinstance(row.get("name"), str)
                or not isinstance(row.get("version"), str)):
            raise DependencyGateError("Cargo.lock contains an invalid package table")
        packages.append({
            "name": row["name"],
            "version": row["version"],
            "source": str(row.get("source", "workspace/path")),
        })
    return packages


def _is_forbidden(stage: Stage, package: dict[str, object]) -> bool:
    name, version = str(package["name"]), str(package["version"])
    if name in LEGACY_FIRST_PARTY_PACKAGES:
        return True
    if stage.require_device_neutral and name in DEVICE_ONLY_PACKAGES:
        return True
    if name.startswith("vello"):
        return not (stage.allow_vello_gpu and (name, version) in NEW_VELLO_GPU_STACK)
    return False


def _package_key(package: dict[str, object]) -> str:
    return f"{package['name']}@{package['version']}"


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run_stage(
    stage_name: str,
    repo: Path = ROOT,
    cargo: str | None = None,
    minimal_target: str | None = None,
) -> dict:
    try:
        stage = STAGES[stage_name]
    except KeyError as error:
        raise DependencyGateError(f"unknown dependency stage: {stage_name}") from error
    if stage_name == "minimal-engine" and minimal_target:
        stage = replace(
            stage,
            target=minimal_target,
            scope=f"animatix normal dependency graph, no default features, target {minimal_target}",
        )
    repo = repo.resolve()
    lockfile = repo / "Cargo.lock"
    if not lockfile.is_file():
        raise DependencyGateError(f"missing lockfile: {lockfile}")
    before = file_sha256(lockfile)
    command = cargo_tree_command(stage, cargo or os.environ.get("CARGO", "cargo"))
    try:
        result = subprocess.run(command, cwd=repo, capture_output=True, text=True, check=False)
    except OSError as error:
        raise DependencyGateError(f"cannot run cargo tree: {error}") from error
    after = file_sha256(lockfile)
    if result.returncode:
        raise DependencyGateError(
            f"cargo tree failed for {stage_name} (exit {result.returncode}): "
            f"{result.stderr.strip()}"
        )
    packages = parse_tree_output(result.stdout)
    graph_findings = sorted({
        _package_key(row) for row in packages if _is_forbidden(stage, row)
    })
    lock_findings: list[str] = []
    if stage.inspect_lockfile:
        lock_findings = sorted({
            _package_key(row) for row in lockfile_packages(lockfile) if _is_forbidden(stage, row)
        })
    lock_unchanged = before == after
    findings = sorted(set(graph_findings + lock_findings))
    if not lock_unchanged:
        findings.append("Cargo.lock changed despite --locked")
    return {
        "stage": stage.name,
        "scope": stage.scope,
        "command": command,
        "status": "passed" if not findings else "failed",
        "allow_vello_gpu": stage.allow_vello_gpu,
        "require_device_neutral": stage.require_device_neutral,
        "findings": findings,
        "graph_findings": graph_findings,
        "lockfile_findings": lock_findings,
        "lockfile_sha256_before": before,
        "lockfile_sha256_after": after,
        "lockfile_unchanged": lock_unchanged,
        "packages": packages,
        "package_count": len(packages),
        "cargo_stderr": result.stderr.strip(),
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", action="append", choices=sorted(STAGES), dest="stages",
                        help="dependency stage to check; repeat to run multiple stages (default: all)")
    parser.add_argument("--repo", type=Path, default=ROOT, help=argparse.SUPPRESS)
    parser.add_argument(
        "--minimal-target",
        help="limit only minimal-engine to a Rust target triple (default: all targets)",
    )
    parser.add_argument("--output", type=Path, help="write the JSON report to this path")
    args = parser.parse_args(argv)
    stages = args.stages or list(STAGES)
    report = {"schema_version": 1, "results": []}
    exit_code = 0
    for name in stages:
        try:
            result = run_stage(name, args.repo, minimal_target=args.minimal_target)
        except DependencyGateError as error:
            result = {"stage": name, "status": "error", "error": str(error)}
        report["results"].append(result)
        if result["status"] != "passed":
            exit_code = 1
    encoded = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.output:
        destination = args.output if args.output.is_absolute() else args.repo / args.output
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(encoded, encoding="utf-8")
        print(destination)
    else:
        sys.stdout.write(encoded)
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
