#!/usr/bin/env python3
"""Read-only dependency checks for Vello GPU and the minimal engine.

The checks use `cargo tree --locked` with explicit package/edge/target scopes.
The minimal-engine and wasm checks report on their resolved package graph;
they do not fail just because another workspace target still has an old package
in Cargo.lock. The workspace checks cover every package, feature, dependency
kind, and target, then also check the lockfile. CI permits the documented
third-party CPU edges. The strict no-legacy checks still report them.

Examples:

    python3 tools/ci/dependency-gates.py --stage minimal-engine
    python3 tools/ci/dependency-gates.py --stage wasm-renderer --stage workspace-renderer
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
NEW_VELLO_GPU_STACK = {
    ("vello_gpu", "0.4.0"),
    ("vello_gpu_shaders", "0.4.0"),
    ("vello_common", "0.4.0"),
}
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
REVIEWED_CPU_PARENTS = {
    "vello_cpu@0.0.8": {"hayro@0.7.1"},
    "vello_common@0.0.8": {"vello_cpu@0.0.8"},
    "vello_common@0.0.9": {"glifo@0.1.1"},
    "vello_cpu@0.2.0": {"epaint@0.37.0"},
    "vello_common@0.2.0": {"vello_cpu@0.2.0", "glifo@0.3.0"},
}
LOCK_ONLY_CPU_PACKAGES = {"vello_common@0.0.9"}
NATIVE_WGPU_FEATURES = {"dx12", "gles", "metal", "vulkan", "vulkan-portability"}


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
    reviewed_cpu_packages: frozenset[str] = frozenset()
    require_browser_backends: bool = False


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
STAGES["wasm-renderer"] = replace(
    STAGES["wasm-no-legacy"],
    name="wasm-renderer",
    reviewed_cpu_packages=frozenset({"vello_cpu@0.0.8", "vello_common@0.0.8"}),
    require_browser_backends=True,
)
STAGES["workspace-renderer"] = replace(
    STAGES["full-no-legacy"],
    name="workspace-renderer",
    reviewed_cpu_packages=frozenset(REVIEWED_CPU_PARENTS),
)


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
        row = packages.setdefault(key, {
            "name": name,
            "version": version,
            "package_id": package_id,
            "features": [],
        })
        enabled = features.removesuffix(" (*)").strip().split(",")
        row["features"] = sorted(set(row["features"]) | set(filter(None, enabled)))
    if not packages:
        raise DependencyGateError("cargo tree returned no package rows")
    return [packages[key] for key in sorted(packages)]


def lockfile_packages(lockfile: Path) -> list[dict[str, object]]:
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
        dependencies = row.get("dependencies", [])
        if not isinstance(dependencies, list) or not all(isinstance(dep, str) for dep in dependencies):
            raise DependencyGateError("Cargo.lock contains an invalid dependency list")
        packages.append({
            "name": row["name"],
            "version": row["version"],
            "source": str(row.get("source", "workspace/path")),
            "dependencies": dependencies,
        })
    return packages


def _is_forbidden(stage: Stage, package: dict[str, object]) -> bool:
    name, version = str(package["name"]), str(package["version"])
    if name in LEGACY_FIRST_PARTY_PACKAGES:
        return True
    if stage.require_device_neutral and (
        name in {"animatix-render", "wgpu"} or name.startswith("wgpu-")
    ):
        return True
    if name.startswith("vello"):
        return not (stage.allow_vello_gpu and (name, version) in NEW_VELLO_GPU_STACK)
    return False


def _package_key(package: dict[str, object]) -> str:
    return f"{package['name']}@{package['version']}"


def reviewed_cpu_packages(stage: Stage, lock: list[dict[str, object]]) -> set[str]:
    approved = set()
    for key in stage.reviewed_cpu_packages:
        rows = [row for row in lock if _package_key(row) == key]
        if len(rows) != 1 or rows[0]["source"] != CRATES_IO:
            continue
        name, version = key.split("@")
        parents = [row for row in lock if any(
            dep.split(maxsplit=2)[:2] in ([name], [name, version])
            for dep in row["dependencies"]
        )]
        if parents and all(
            _package_key(parent) in REVIEWED_CPU_PARENTS[key] and parent["source"] == CRATES_IO
            for parent in parents
        ):
            approved.add(key)
    return approved


def browser_backend_findings(packages: list[dict[str, object]]) -> list[str]:
    features = {feature for row in packages if row["name"] == "wgpu" for feature in row["features"]}
    missing = sorted({"webgpu", "webgl"} - features)
    native = sorted(features & NATIVE_WGPU_FEATURES)
    return ([f"Wasm wgpu feature missing: {feature}" for feature in missing]
            + [f"Wasm wgpu enables native backend: {feature}" for feature in native])


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
    lock = lockfile_packages(lockfile) if stage.inspect_lockfile or stage.reviewed_cpu_packages else []
    approved = reviewed_cpu_packages(stage, lock)
    graph_approved = approved - LOCK_ONLY_CPU_PACKAGES
    graph_findings = sorted({
        _package_key(row) for row in packages if _is_forbidden(stage, row)
    } - graph_approved)
    lock_findings: list[str] = []
    if stage.inspect_lockfile:
        lock_findings = sorted({
            _package_key(row) for row in lock if _is_forbidden(stage, row)
        } - approved)
    lock_unchanged = before == after
    backend_findings = browser_backend_findings(packages) if stage.require_browser_backends else []
    findings = sorted(set(graph_findings + lock_findings + backend_findings))
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
        "reviewed_cpu_packages": sorted(approved),
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
            if args.output:
                for finding in result.get("findings", [result.get("error")]):
                    print(f"{name}: {finding}", file=sys.stderr)
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
