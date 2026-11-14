#!/usr/bin/env python3
"""Select PR CI jobs from Cargo workspace path dependencies.

This is a shadow planner until the workflow explicitly passes --enable-pruning.
Its job IDs are policy keys; the actual job definitions are read from
scripts/ci.sh --matrix pr so adding an unclassified registry row forces a full
matrix instead of silently omitting the new check.
"""

from __future__ import annotations

import argparse
from collections import defaultdict, deque
import json
import os
from pathlib import Path
import subprocess
import sys
import tomllib


ALWAYS_IDS = {"doc-links"}
KNOWN_IDS = {
    "fast-checks", "doc-links", "check-lint", "eparts-linux", "eparts-macos",
    "eparts-windows", "test-examples", "render-scenes", "benchmarks",
    "web-check", "web-build",
}

ENGINE_PACKAGES = {
    "animatix", "animatix-core", "animatix-render", "animatix-std",
    "animatix-syntax", "animatix-text",
}
BENCHMARK_PACKAGES = {
    "animatix", "animatix-plugin-api", "animatix-plugin-demo",
    "animatix-plugin-tooling",
}
WEB_TOOL_PACKAGES = {"animatix-web-build", "animatix-text-parity"}
FULL_PATHS = {
    "Cargo.toml", "Cargo.lock", "flake.nix", "flake.lock", "rust-toolchain.toml",
    "rust-toolchain", "shell.nix", "default.nix",
    "LICENSE", "LICENSE-MIT", "LICENSE-APACHE",
}
DOC_GENERATOR_PATHS = {
    "docs/properties.md", "docs/property-reference-groups.json",
}
DOC_ROOT_FILES = {
    "README", "README.md", "CHANGELOG.md", "CONTRIBUTING.md",
}


class PlanError(ValueError):
    """A workspace or CI matrix cannot be safely interpreted."""


def _read_toml(path: Path) -> dict:
    try:
        with path.open("rb") as source:
            return tomllib.load(source)
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise PlanError(f"cannot read {path}: {error}") from error


def _dependency_sections(manifest: dict):
    """Yield dependency tables from normal and target-specific manifest sections."""
    for key in ("dependencies", "dev-dependencies", "build-dependencies"):
        table = manifest.get(key)
        if isinstance(table, dict):
            yield table
    target_tables = manifest.get("target", {})
    if isinstance(target_tables, dict):
        for target in target_tables.values():
            if not isinstance(target, dict):
                continue
            for key in ("dependencies", "dev-dependencies", "build-dependencies"):
                table = target.get(key)
                if isinstance(table, dict):
                    yield table


def _dependency_path(spec, workspace_deps: dict, key: str):
    if spec is True:
        spec = {"workspace": True}
    if not isinstance(spec, dict):
        return None, False
    inherited = spec.get("workspace") is True
    if inherited:
        inherited_spec = workspace_deps.get(key, {})
        if not isinstance(inherited_spec, dict):
            return None, True
        spec = {**inherited_spec, **{k: v for k, v in spec.items() if k != "workspace"}}
    path = spec.get("path")
    return (path if isinstance(path, str) else None), inherited


def load_workspace(repo: Path):
    """Return package roots, dependency edges, and reverse dependency edges."""
    root = _read_toml(repo / "Cargo.toml")
    workspace = root.get("workspace")
    if not isinstance(workspace, dict):
        raise PlanError("root Cargo.toml has no [workspace] table")
    members = workspace.get("members")
    if not isinstance(members, list):
        raise PlanError("workspace.members is missing or not an array")

    package_roots: dict[str, Path] = {}
    package_manifests: dict[str, Path] = {}
    manifests: dict[str, dict] = {}
    for member in members:
        if not isinstance(member, str) or any(token in member for token in ("*", "?", "[")):
            raise PlanError(f"unsupported workspace member pattern: {member!r}")
        member_root = (repo / member).resolve()
        manifest_path = member_root / "Cargo.toml"
        data = _read_toml(manifest_path)
        package = data.get("package", {})
        name = package.get("name") if isinstance(package, dict) else None
        if not isinstance(name, str):
            raise PlanError(f"workspace member has no package.name: {manifest_path}")
        if name in package_roots:
            raise PlanError(f"duplicate workspace package name: {name}")
        package_roots[name] = member_root
        package_manifests[name] = manifest_path
        manifests[name] = data

    workspace_deps = workspace.get("dependencies", {})
    if not isinstance(workspace_deps, dict):
        workspace_deps = {}

    root_to_name = {path: name for name, path in package_roots.items()}
    dependencies = {name: set() for name in package_roots}
    for name, data in manifests.items():
        for table in _dependency_sections(data):
            for dependency_key, spec in table.items():
                path, inherited = _dependency_path(spec, workspace_deps, dependency_key)
                target = None
                if path is not None:
                    base = repo if inherited else package_roots[name]
                    raw_path = (base / path).resolve()
                    manifest_path = raw_path if raw_path.name == "Cargo.toml" else raw_path / "Cargo.toml"
                    target = root_to_name.get(manifest_path.parent.resolve())
                if target is None and isinstance(spec, dict):
                    requested_name = spec.get("package", dependency_key)
                    if requested_name in package_roots and path is None:
                        target = requested_name
                if target is not None:
                    dependencies[name].add(target)

    reverse = {name: set() for name in package_roots}
    for package, deps in dependencies.items():
        for dependency in deps:
            reverse[dependency].add(package)
    return package_roots, dependencies, reverse


def reverse_closure(seeds: set[str], reverse: dict[str, set[str]]) -> set[str]:
    affected = set(seeds)
    queue = deque(seeds)
    while queue:
        current = queue.popleft()
        for consumer in reverse.get(current, ()):
            if consumer not in affected:
                affected.add(consumer)
                queue.append(consumer)
    return affected


def package_for_path(path: str, roots: dict[str, Path], repo: Path):
    candidate = (repo / path).resolve()
    matches = [
        (len(str(root)), name)
        for name, root in roots.items()
        if candidate == root or root in candidate.parents
    ]
    return max(matches)[1] if matches else None


def normalize_repo_path(path: str) -> str:
    """Reject unsafe/non-repository paths and normalize only a leading ./."""
    normalized = path.replace("\\", "/")
    while normalized.startswith("./"):
        normalized = normalized[2:]
    parts = normalized.split("/")
    if (not normalized or normalized.startswith("/") or
            (len(normalized) >= 2 and normalized[1] == ":") or
            any(part in {"", ".", ".."} for part in parts)):
        raise PlanError(f"unsafe or unclassified repository path: {path!r}")
    return normalized


def is_full_path(path: str) -> str | None:
    if path in FULL_PATHS:
        return "workspace, lock, toolchain, or Nix input changed"
    if path.startswith(("crates/", "tools/")) and path.endswith("/Cargo.toml"):
        return "package manifest changed; dependency graph may have changed"
    if path.startswith((".github/", ".cargo/", "nix/")):
        return "CI, Cargo config, or Nix input changed"
    if path.startswith("scripts/ci"):
        return "CI planner or registry changed"
    return None


def classify_paths(changed_paths: list[str], roots: dict[str, Path],
                   repo: Path, reverse: dict[str, set[str]] | None = None):
    """Classify changed paths using workspace owners and known content roots."""
    package_seeds: set[str] = set()
    examples_changed = False
    web_changed = False
    web_scenes_changed = False
    web_build_tool_changed = False
    packaging_changed = False
    docs_generator_changed = False
    unknown_paths: list[str] = []
    normalized_paths: list[str] = []
    full_reason = None

    for raw_path in changed_paths:
        try:
            path = normalize_repo_path(raw_path)
        except PlanError as error:
            full_reason = str(error)
            break
        normalized_paths.append(path)

        reason = is_full_path(path)
        if reason:
            full_reason = reason
            break

        # This path owns the packaged web-build content, including its nested
        # split-probe workspace member, which has no Cargo dependency edge back
        # to the web-build tool.
        if path.startswith("tools/animatix-web-build/"):
            web_build_tool_changed = True

        package = package_for_path(path, roots, repo)
        if package:
            package_seeds.add(package)
            continue

        if path in DOC_GENERATOR_PATHS:
            docs_generator_changed = True
            continue
        if path.startswith("docs/") or path in DOC_ROOT_FILES:
            continue
        if path.startswith("web/"):
            web_changed = True
            if path.startswith(("web/scenes/", "web/demos/", "web/fonts/")):
                web_scenes_changed = True
            continue
        if path.startswith("examples/"):
            examples_changed = True
            continue
        if path.startswith("packaging/"):
            packaging_changed = True
            continue
        if path.startswith("vendor/"):
            full_reason = "vendored dependency changed"
            break
        if path.startswith("scripts/"):
            unknown_paths.append(path)
            continue
        unknown_paths.append(path)

    if full_reason is None and unknown_paths:
        full_reason = "unclassified path changed: " + ", ".join(sorted(unknown_paths))
    if reverse is None:
        reverse = load_workspace(repo)[2]
    affected = reverse_closure(package_seeds, reverse) if package_seeds else set()
    return {
        "full_reason": full_reason,
        "changed_paths": sorted(normalized_paths),
        "affected_packages": sorted(affected),
        "package_seeds": sorted(package_seeds),
        "examples_changed": examples_changed,
        "web_changed": web_changed,
        "web_scenes_changed": web_scenes_changed,
        "web_build_tool_changed": web_build_tool_changed,
        "packaging_changed": packaging_changed,
        "docs_generator_changed": docs_generator_changed,
        "docs_only": bool(normalized_paths) and not (
            package_seeds or examples_changed or web_changed or packaging_changed
        ),
    }


def _reverse_edges(repo: Path):
    return load_workspace(repo)[2]


def selected_ids(classification: dict) -> set[str]:
    """Return stable JOB_TABLE ids needed for the classified changes."""
    selected = set(ALWAYS_IDS)
    packages = set(classification["affected_packages"])
    seeds = set(classification["package_seeds"])

    # Rust source/config under a package runs workspace checks and tests. The
    # Cargo/Nix/workflow roots are handled earlier as full-matrix changes.
    if seeds:
        selected.update({"fast-checks", "check-lint", "test-examples"})
    if classification["docs_generator_changed"]:
        selected.add("check-lint")
    if classification["examples_changed"]:
        selected.update({"fast-checks", "test-examples", "render-scenes"})
    if packages & ENGINE_PACKAGES:
        selected.add("render-scenes")
    if packages & BENCHMARK_PACKAGES:
        selected.add("benchmarks")
    if "eparts" in packages:
        selected.update({"eparts-linux", "eparts-macos", "eparts-windows"})

    # Reverse closure picks up animatix-web for shared engine/syntax/text
    # changes; GUI-only edits do not reach the web package.
    if "animatix-web" in packages:
        selected.update({"web-check", "web-build"})
    if seeds & WEB_TOOL_PACKAGES or classification.get("web_build_tool_changed", False):
        selected.add("web-build")
    if classification["web_changed"]:
        selected.update({"fast-checks", "test-examples", "web-build"})
    if classification["packaging_changed"]:
        selected.add("fast-checks")
    if classification.get("web_scenes_changed", False):
        selected.add("render-scenes")
    return selected


def get_pr_matrix(repo: Path):
    result = subprocess.run(
        ["bash", "scripts/ci.sh", "--matrix", "pr"],
        cwd=repo, capture_output=True, check=False,
    )
    if result.returncode:
        raise PlanError("scripts/ci.sh --matrix pr failed: " +
                        result.stderr.decode("utf-8", "replace").strip())
    try:
        matrix = json.loads(result.stdout)
        jobs = matrix["include"]
        if not isinstance(jobs, list) or not jobs:
            raise ValueError("empty include list")
        if any(not isinstance(job, dict) or not isinstance(job.get("id"), str) for job in jobs):
            raise ValueError("matrix rows need stable string ids")
        ids = [job["id"] for job in jobs]
        if len(ids) != len(set(ids)):
            raise ValueError("matrix contains duplicate stable ids")
        return jobs
    except (json.JSONDecodeError, KeyError, TypeError, ValueError) as error:
        raise PlanError(f"cannot interpret PR matrix: {error}") from error


def git_changes(repo: Path, base: str, head: str):
    if not base or not head or base == "0000000000000000000000000000000000000000":
        raise PlanError("base or head is missing")
    exists = subprocess.run(
        ["git", "cat-file", "-e", f"{base}^{{commit}}"], cwd=repo,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
    )
    if exists.returncode:
        raise PlanError("base commit is unavailable")
    exists = subprocess.run(
        ["git", "cat-file", "-e", f"{head}^{{commit}}"], cwd=repo,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
    )
    if exists.returncode:
        raise PlanError("head commit is unavailable")
    ancestor = subprocess.run(
        ["git", "merge-base", "--is-ancestor", base, head], cwd=repo,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
    )
    if ancestor.returncode:
        raise PlanError("base is not an ancestor of head")

    result = subprocess.run(
        ["git", "diff", "--name-only", "-z", "--no-renames", base, head, "--"],
        cwd=repo, capture_output=True, check=False,
    )
    if result.returncode:
        raise PlanError("git diff could not determine changed paths")
    return [os.fsdecode(part) for part in result.stdout.split(b"\0") if part]


def make_plan_from_classification(classification: dict, event: str, base: str,
                                  head: str, all_jobs: list[dict],
                                  enable_pruning: bool = False):
    ids = [job["id"] for job in all_jobs]
    all_ids = set(ids)
    unknown_ids = all_ids - KNOWN_IDS
    if unknown_ids:
        reason = "unclassified CI job id: " + ", ".join(sorted(unknown_ids))
        candidate = set(ids)
    elif event != "pull_request":
        reason = f"event {event!r} always runs the full matrix"
        candidate = set(ids)
    elif not base or base == "0000000000000000000000000000000000000000":
        reason = "base commit is missing"
        candidate = set(ids)
    elif classification.get("full_reason"):
        reason = classification["full_reason"]
        candidate = set(ids)
    else:
        reason = "paths classified by workspace dependency graph"
        candidate = selected_ids(classification)

    candidate &= all_ids
    effective = candidate if enable_pruning else all_ids
    by_id = {job["id"]: job for job in all_jobs}
    return {
        "mode": "full" if candidate == all_ids else ("selective" if enable_pruning else "shadow"),
        "force_full": candidate == all_ids,
        "reason": reason,
        "pruning_enabled": enable_pruning,
        "candidate_job_ids": [job["id"] for job in all_jobs if job["id"] in candidate],
        "job_ids": [job["id"] for job in all_jobs if job["id"] in effective],
        "matrix": {"include": [by_id[job["id"]] for job in all_jobs if job["id"] in effective]},
        "candidate_matrix": {"include": [by_id[job["id"]] for job in all_jobs if job["id"] in candidate]},
        "changed_paths": classification.get("changed_paths", []),
        "package_seeds": classification.get("package_seeds", []),
        "affected_packages": classification.get("affected_packages", []),
    }


def make_plan(repo: Path, event: str, base: str, head: str,
              all_jobs: list[dict], enable_pruning: bool = False):
    try:
        changed = git_changes(repo, base, head)
        roots, _, reverse = load_workspace(repo)
        classification = classify_paths(changed, roots, repo, reverse)
        classification["changed_paths"] = changed
    except PlanError as error:
        classification = {
            "full_reason": f"planner uncertainty: {error}",
            "changed_paths": [],
            "package_seeds": [],
            "affected_packages": [],
        }
    return make_plan_from_classification(
        classification, event, base, head, all_jobs, enable_pruning
    )


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--event", default=os.environ.get("GITHUB_EVENT_NAME", ""))
    parser.add_argument("--base", default="")
    parser.add_argument("--head", default="HEAD")
    parser.add_argument(
        "--enable-pruning", action="store_true",
        help="use candidate job selection; default is shadow/full matrix",
    )
    args = parser.parse_args(argv)
    repo = args.repo.resolve()
    try:
        all_jobs = get_pr_matrix(repo)
        plan = make_plan(repo, args.event, args.base, args.head, all_jobs, args.enable_pruning)
    except PlanError as error:
        print(json.dumps({
            "mode": "full", "force_full": True,
            "reason": f"planner uncertainty: {error}",
            "pruning_enabled": args.enable_pruning,
            "job_ids": [], "candidate_job_ids": [],
            "matrix": {"include": []}, "candidate_matrix": {"include": []},
        }, sort_keys=True))
        return 0
    print(json.dumps(plan, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
