"""Fail-closed identity checks for Cargo-built and nextest-discovered binaries."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

from scripts.experiments.common import ExperimentFailure


def _real_file(value: str | Path, label: str) -> Path:
    try:
        path = Path(value).resolve(strict=True)
    except (OSError, RuntimeError, TypeError) as error:
        raise ExperimentFailure(f"{label} is not a resolvable file: {value}: {error}") from error
    if not path.is_file():
        raise ExperimentFailure(f"{label} is not a file: {path}")
    return path


def _sha256(path: Path) -> str:
    try:
        with path.open("rb") as stream:
            return hashlib.file_digest(stream, "sha256").hexdigest()
    except OSError as error:
        raise ExperimentFailure(f"cannot hash test executable {path}: {error}") from error


def _suite_rows(data: dict) -> tuple[dict, list[tuple[str, dict]]]:
    build_meta = data.get("rust-build-meta") if isinstance(data, dict) else None
    if not isinstance(build_meta, dict) or not build_meta.get("target-directory"):
        raise ExperimentFailure("nextest listing is missing rust-build-meta.target-directory")
    suites = data.get("rust-suites")
    if isinstance(suites, dict):
        rows = list(suites.items())
    elif isinstance(suites, list):
        rows = [(suite.get("binary-id", ""), suite) for suite in suites if isinstance(suite, dict)]
        if len(rows) != len(suites):
            raise ExperimentFailure("nextest rust-suites contains a non-object entry")
    else:
        raise ExperimentFailure("nextest listing has no rust-suites object or list")
    if not rows:
        raise ExperimentFailure("nextest listing contains no rust suites")
    if any(not isinstance(suite, dict) for _, suite in rows):
        raise ExperimentFailure("nextest rust-suites contains a non-object suite")
    return build_meta, rows


def verify_nextest_listing(data: dict, artifacts: list[dict], target_dir: Path,
                           *, exact_suite_set: bool) -> dict:
    """Match every listed suite to one prepared Cargo executable and hash it.

    Full listings require an exact suite/artifact bijection. Filtered listings
    may omit suites, but every suite they do report must still match exactly.
    Suites are checked even when their testcase list is empty.
    """
    try:
        expected_target = Path(target_dir).resolve(strict=True)
    except (OSError, RuntimeError, TypeError) as error:
        raise ExperimentFailure(
            f"selected Cargo target directory cannot be resolved: {target_dir}: {error}"
        ) from error
    if not expected_target.is_dir():
        raise ExperimentFailure(f"selected Cargo target directory is not a directory: {expected_target}")
    build_meta, suites = _suite_rows(data)
    listed_target_text = build_meta["target-directory"]
    listed_target = Path(listed_target_text)
    if not listed_target.is_absolute():
        raise ExperimentFailure(f"nextest target directory is not absolute: {listed_target_text}")
    try:
        listed_target = listed_target.resolve(strict=True)
    except (OSError, RuntimeError) as error:
        raise ExperimentFailure(
            f"nextest target directory cannot be resolved: {listed_target_text}: {error}"
        ) from error
    if listed_target != expected_target:
        raise ExperimentFailure(
            f"nextest target directory differs from prepared Cargo target: "
            f"{listed_target} != {expected_target}"
        )

    expected = {}
    for artifact in artifacts:
        package_id = artifact.get("package_id")
        binary_name = artifact.get("target_name")
        executable = _real_file(artifact.get("executable", ""), "prepared Cargo executable")
        if not executable.is_relative_to(expected_target):
            raise ExperimentFailure(f"prepared Cargo executable escapes target directory: {executable}")
        key = (package_id, binary_name, str(executable))
        if not package_id or not binary_name or key in expected:
            raise ExperimentFailure(
                f"prepared Cargo test artifacts have a missing or duplicate identity: {key}"
            )
        actual_hash = _sha256(executable)
        recorded_hash = artifact.get("sha256")
        if recorded_hash is not None and recorded_hash != actual_hash:
            raise ExperimentFailure(f"prepared Cargo executable changed before nextest listing: {executable}")
        features = artifact.get("features", [])
        if not isinstance(features, list):
            raise ExperimentFailure(f"prepared Cargo artifact has invalid feature metadata: {key}")
        expected[key] = {"sha256": actual_hash, "features": sorted(features),
                         "profile": artifact.get("profile", {}),
                         "target_kind": artifact.get("target_kind", [])}

    listed_keys = []
    binaries = []
    for binary_id, suite in suites:
        package_id = suite.get("package-id")
        binary_name = suite.get("binary-name")
        binary_path_text = suite.get("binary-path")
        if not package_id or not binary_name or not binary_path_text:
            raise ExperimentFailure(
                f"nextest suite is missing package-id/binary-name/binary-path: {binary_id}"
            )
        binary_path = _real_file(binary_path_text, "nextest executable")
        if not binary_path.is_relative_to(expected_target):
            raise ExperimentFailure(f"nextest executable escapes target directory: {binary_path}")
        key = (package_id, binary_name, str(binary_path))
        if key not in expected:
            raise ExperimentFailure(
                "nextest suite does not match a prepared Cargo artifact: "
                f"package={package_id!r} binary={binary_name!r} path={binary_path}"
            )
        if key in listed_keys:
            raise ExperimentFailure(f"nextest listing repeats a binary suite: {key}")
        listed_keys.append(key)
        actual_hash = _sha256(binary_path)
        expected_data = expected[key]
        if actual_hash != expected_data["sha256"]:
            raise ExperimentFailure(
                f"nextest executable hash differs from its prepared Cargo artifact: {binary_path}"
            )
        binaries.append({
            "package_id": package_id,
            "binary_name": binary_name,
            "binary_id": suite.get("binary-id") or binary_id,
            "binary_path": str(binary_path),
            "sha256": actual_hash,
            "cargo_features": expected_data["features"],
            "cargo_profile": expected_data["profile"],
            "target_kind": expected_data["target_kind"],
            "listed_testcase_count": len(suite.get("testcases", {}))
            if isinstance(suite.get("testcases", {}), (dict, list)) else None,
        })

    if exact_suite_set and set(listed_keys) != set(expected):
        missing = sorted(set(expected) - set(listed_keys))
        extra = sorted(set(listed_keys) - set(expected))
        raise ExperimentFailure(
            f"nextest suite/artifact inventory differs: missing={missing[:8]} extra={extra[:8]}"
        )
    if not exact_suite_set and not listed_keys:
        raise ExperimentFailure("filtered nextest listing contains no binary suites")

    identity_payload = {
        "target_directory": str(expected_target),
        "binaries": sorted(
            binaries,
            key=lambda row: (row["package_id"], row["binary_name"], row["binary_path"]),
        ),
    }
    identity_hash = hashlib.sha256(
        json.dumps(identity_payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    return {
        "verified": True,
        "target_directory": str(expected_target),
        "exact_suite_set": exact_suite_set,
        "listed_suite_count": len(suites),
        "prepared_artifact_count": len(artifacts),
        "identity_sha256": identity_hash,
        "binaries": identity_payload["binaries"],
    }


def recheck_binary_hashes(identity: dict, target_dir: Path) -> dict:
    """Rehash the established Nextest executable set outside timed intervals."""
    try:
        expected_target = Path(target_dir).resolve(strict=True)
        identity_target = Path(identity.get("target_directory", "")).resolve(strict=True)
    except (OSError, RuntimeError, TypeError) as error:
        raise ExperimentFailure(f"Nextest identity target cannot be resolved: {error}") from error
    if identity_target != expected_target:
        raise ExperimentFailure(
            f"Nextest identity target changed: {identity_target} != {expected_target}"
        )
    binaries = identity.get("binaries")
    if not isinstance(binaries, list) or not binaries:
        raise ExperimentFailure("Nextest identity snapshot has no binaries")
    for binary in binaries:
        path = _real_file(binary.get("binary_path", ""), "Nextest executable")
        if not path.is_relative_to(expected_target):
            raise ExperimentFailure(f"Nextest executable escapes target directory: {path}")
        actual_hash = _sha256(path)
        if actual_hash != binary.get("sha256"):
            raise ExperimentFailure(
                f"Nextest executable changed after identity proof: {path} "
                f"(expected {binary.get('sha256')}, got {actual_hash})"
            )
    return {"checked_binary_count": len(binaries), "identity_sha256": identity["identity_sha256"]}
