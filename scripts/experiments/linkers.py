"""Pinned linker discovery and platform-specific compiler configurations."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

from scripts.experiments.common import ExperimentFailure, ExperimentOutput, run_timed


MOLD_SOURCE_REPOSITORY = "https://github.com/rui314/mold.git"
MOLD_SOURCE_REV = "331ac663668dc0b1f4949403854613b9b1913025"
MOLD_MACHO_FEATURES = ["macho-arm64", "macho-x86_64"]
EXPECTED_LLVM_VERSION = "21.1.8"


def validate_revision(revision: str) -> str:
    if not re.fullmatch(r"[0-9a-fA-F]{40}", revision or ""):
        raise ExperimentFailure("mold_source_rev must be an exact 40-character commit SHA")
    return revision.lower()


def _tool_version(command: list[str]) -> dict:
    try:
        result = subprocess.run(command, capture_output=True, text=True, check=False, timeout=15)
        return {"command": command, "exit_code": result.returncode,
                "version": (result.stdout + result.stderr).strip()}
    except (OSError, subprocess.TimeoutExpired) as error:
        return {"command": command, "exit_code": None, "version": str(error)}


def _file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _run_setup(output: ExperimentOutput, platform: str, name: str,
               command: list[str], cwd: Path, timeout: float = 300) -> dict:
    record = run_timed(
        f"{platform}-{name}", command, cwd, {}, output.logs_dir / f"{platform}-{name}.log",
        timeout_seconds=timeout,
    )
    output.append_record(record)
    if record["exit_code"] or record["timed_out"]:
        raise ExperimentFailure(f"{name} failed; see {record['log_path']}")
    return record


def prepare_mold_source(repo: Path, output: ExperimentOutput,
                        revision: str) -> tuple[Path, dict]:
    """Fetch and build macOS mold from a pinned commit in owned directories."""
    revision = validate_revision(revision)
    source_dir = output.owned_target(f"sources/mold-{revision[:12]}", reset=True)
    _run_setup(output, "macos", "mold-source-init", ["git", "init", str(source_dir)], repo)
    _run_setup(output, "macos", "mold-source-remote",
               ["git", "-C", str(source_dir), "remote", "add", "origin", MOLD_SOURCE_REPOSITORY], repo)
    _run_setup(output, "macos", "mold-source-fetch",
               ["git", "-C", str(source_dir), "fetch", "--depth=1", "origin", revision], repo,
               timeout=900)
    _run_setup(output, "macos", "mold-source-checkout",
               ["git", "-C", str(source_dir), "checkout", "--detach", "FETCH_HEAD"], repo)

    verify = subprocess.run(
        ["git", "-C", str(source_dir), "rev-parse", "HEAD"], capture_output=True,
        text=True, check=False,
    )
    actual_revision = verify.stdout.strip().lower()
    if verify.returncode or actual_revision != revision:
        raise ExperimentFailure(
            f"mold checkout SHA mismatch: expected {revision}, got {actual_revision or verify.returncode}"
        )

    target = output.owned_target("mold-build-target", reset=True)
    build_env = {
        "CARGO_TARGET_DIR": str(target),
        "CARGO_INCREMENTAL": "0",
        "RUSTUP_TOOLCHAIN": "stable",
    }
    build = run_timed(
        "macos-mold-source-build",
        ["cargo", "build", "--release", "--bin", "mold", "--features", ",".join(MOLD_MACHO_FEATURES)],
        source_dir, build_env, output.logs_dir / "macos-mold-build.log", timeout_seconds=7200,
    )
    output.append_record(build)
    if build["exit_code"] or build["timed_out"]:
        raise ExperimentFailure(f"building pinned mold failed; see {build['log_path']}")
    binary = target / "release" / "mold"
    if not binary.is_file():
        raise ExperimentFailure(f"pinned mold build did not produce {binary}")
    version = _tool_version([str(binary), "--version"])
    if version["exit_code"]:
        raise ExperimentFailure(f"pinned mold version probe failed: {version['version']}")
    link_dir = output.directory / "linkers"
    link_dir.mkdir(parents=True, exist_ok=True)
    ld64_mold = link_dir / "ld64.mold"
    if ld64_mold.exists() or ld64_mold.is_symlink():
        ld64_mold.unlink()
    ld64_mold.symlink_to(binary)
    return ld64_mold, {
        "source_repository": MOLD_SOURCE_REPOSITORY,
        "source_revision": revision,
        "source_actual_revision": actual_revision,
        "source_path": str(source_dir),
        "binary_path": str(binary),
        "binary_sha256": _file_sha256(binary),
        "macho_cli_features": MOLD_MACHO_FEATURES,
        "version_probe": version,
    }


def _absolute_keep_alias(path: str | Path) -> Path:
    """Make an absolute path without resolving away an ld64 dispatch basename."""
    return Path(os.path.abspath(os.fspath(path)))


def _find_macos_lld() -> Path:
    candidate = shutil.which("ld64.lld")
    brew = shutil.which("brew")
    if not candidate and brew:
        prefix = subprocess.run(
            [brew, "--prefix", "llvm@21"], capture_output=True, text=True, check=False,
        )
        if prefix.returncode == 0:
            candidate_path = Path(prefix.stdout.strip()) / "bin" / "ld64.lld"
            if candidate_path.is_file():
                candidate = str(candidate_path)
    if not candidate:
        raise ExperimentFailure("pinned lld@21 `ld64.lld` is unavailable; candidate cannot be skipped")
    candidate_path = _absolute_keep_alias(candidate)
    version = _tool_version([str(candidate_path), "--version"])
    if version["exit_code"] or not re.search(
            rf"\b{re.escape(EXPECTED_LLVM_VERSION)}\b", version["version"]):
        raise ExperimentFailure(
            f"ld64.lld must be LLVM {EXPECTED_LLVM_VERSION}; got {version['version']!r}"
        )
    expected_hash = os.environ.get("ANIMATIX_EXPECTED_LLD_SHA256")
    actual_hash = _file_sha256(candidate_path)
    if expected_hash and actual_hash.lower() != expected_hash.strip().lower():
        raise ExperimentFailure(
            "ld64.lld SHA-256 differs from the hash recorded during pinned LLVM installation: "
            f"expected {expected_hash.strip().lower()}, got {actual_hash}"
        )
    return candidate_path


def _make_audit_wrapper(wrapper_path: Path, callee: Path, argv0: str) -> dict:
    wrapper_path.parent.mkdir(parents=True, exist_ok=True)
    callee_abs = _absolute_keep_alias(callee)
    callee_hash = _file_sha256(callee_abs)
    source = f'''#!/usr/bin/env python3
import hashlib, json, os, sys, time
callee = {str(callee_abs)!r}
argv0 = {argv0!r}
log = os.environ.get("ANIMATIX_LINKER_AUDIT_LOG")
if not log:
    raise SystemExit("ANIMATIX_LINKER_AUDIT_LOG is required for linker audit")
h = hashlib.sha256()
with open(callee, "rb") as f:
    for chunk in iter(lambda: f.read(1048576), b""):
        h.update(chunk)
args = sys.argv[1:]
output = None
for index, arg in enumerate(args):
    if arg == "-o" and index + 1 < len(args):
        output = args[index + 1]
        break
record = {{"time": time.time(), "wrapper": sys.argv[0], "callee": callee,
          "argv0": argv0, "argv": args, "cwd": os.getcwd(),
          "output_path": os.path.abspath(output) if output else None,
          "callee_sha256": h.hexdigest()}}
with open(log, "a", encoding="utf-8") as f:
    f.write(json.dumps(record, sort_keys=True) + "\\n")
    f.flush()
os.execve(callee, [argv0, *sys.argv[1:]], os.environ.copy())
'''
    wrapper_path.write_text(source, encoding="utf-8")
    wrapper_path.chmod(0o755)
    return {"wrapper_path": str(wrapper_path), "callee_path": str(callee_abs),
            "callee_sha256": callee_hash, "argv0": argv0}


def _mold_linux_inventory() -> dict | None:
    path = shutil.which("mold")
    if not path:
        return None
    executable = _absolute_keep_alias(path)
    return {"path": str(executable), "sha256": _file_sha256(executable),
            **_tool_version([str(executable), "--version"])}


def linker_arms(platform: str, output: ExperimentOutput,
                mold_binary: Path | None = None) -> tuple[list[dict], dict, list[dict]]:
    """Return available arms, inventory, and explicit candidate setup failures."""
    failures = []
    if platform == "linux":
        clang = shutil.which("clang")
        bfd = shutil.which("ld.bfd")
        mold = _mold_linux_inventory()
        # Rust 1.90+ defaults are intentionally measured as configured by this
        # toolchain. -v is diagnostic only; it does not select a linker.
        arms = [{"id": "default", "env": {"RUSTFLAGS": "-C link-arg=-v",
                                                "CARGO_ENCODED_RUSTFLAGS": None},
                 "expected_linker": "toolchain-default"}]
        if not clang or not bfd:
            failures.append({"arm": "system-bfd", "reason": "clang or ld.bfd unavailable"})
        else:
            clang_abs = _absolute_keep_alias(clang)
            bfd_abs = _absolute_keep_alias(bfd)
            wrapper = output.directory / "linker-wrappers" / "ld.bfd"
            bfd_audit = _make_audit_wrapper(wrapper, bfd_abs, bfd_abs.name)
            arms.append({
                "id": "system-bfd",
                "env": {"RUSTFLAGS": f"-C linker={clang_abs} -C linker-features=-lld -C link-arg=--ld-path={wrapper} -C link-arg=-v",
                        "CARGO_ENCODED_RUSTFLAGS": None},
                "expected_linker": "wrapper-audit", "artifact_identity": "gnu",
                "audit": bfd_audit,
            })
        if mold is None:
            failures.append({"arm": "mold", "reason": "Nix-provided mold executable unavailable"})
        elif not clang:
            failures.append({"arm": "mold", "reason": "clang driver unavailable"})
        else:
            clang_abs = _absolute_keep_alias(clang)
            mold_path = Path(mold["path"])
            wrapper = output.directory / "linker-wrappers" / "mold"
            mold_audit = _make_audit_wrapper(wrapper, mold_path, mold_path.name)
            arms.append({
                "id": "mold",
                "env": {"RUSTFLAGS": f"-C linker={clang_abs} -C linker-features=-lld -C link-arg=--ld-path={wrapper} -C link-arg=-v",
                        "CARGO_ENCODED_RUSTFLAGS": None},
                "expected_linker": "wrapper-audit", "artifact_identity": "mold",
                "audit": mold_audit,
            })
        inventory = {
            "clang": _tool_version([str(_absolute_keep_alias(clang)), "--version"]) if clang else None,
            "system_linker": {"path": str(_absolute_keep_alias(bfd)), "sha256": _file_sha256(_absolute_keep_alias(bfd)),
                              **_tool_version([str(_absolute_keep_alias(bfd)), "--version"]),
                              "wrapper": next((arm["audit"] for arm in arms if arm["id"] == "system-bfd"), None)} if bfd else None,
            "nix_mold": mold,
            "mold_wrapper": next((arm["audit"] for arm in arms if arm["id"] == "mold"), None),
        }
        return arms, inventory, failures

    if platform == "macos":
        clang = shutil.which("clang") or "/usr/bin/clang"
        apple_ld = subprocess.run(["xcrun", "--find", "ld"], capture_output=True, text=True, check=False)
        arms = []
        inventory = {"clang": _tool_version([clang, "--version"])}
        if apple_ld.returncode:
            failures.append({"arm": "apple", "reason": "xcrun --find ld failed"})
        else:
            apple_path = _absolute_keep_alias(apple_ld.stdout.strip())
            arms.append({"id": "apple", "env": {"RUSTFLAGS": "-C link-arg=-v", "CARGO_ENCODED_RUSTFLAGS": None},
                         "expected_linker": "apple", "expected_linker_path": str(apple_path)})
            inventory["apple_ld"] = {"path": str(apple_path), "sha256": _file_sha256(apple_path),
                                      **_tool_version([str(apple_path), "-v"])}

        try:
            lld = _find_macos_lld()
            wrapper_dir = output.directory / "linker-wrappers"
            lld_wrapper = wrapper_dir / "ld64.lld"
            lld_audit = _make_audit_wrapper(lld_wrapper, lld, "ld64.lld")
            arms.append({
                "id": "ld64-lld",
                "env": {"RUSTFLAGS": f"-C linker={clang} -C link-arg=--ld-path={lld_wrapper} -C link-arg=-v",
                        "CARGO_ENCODED_RUSTFLAGS": None, "ANIMATIX_LINKER_ARGV0": "ld64.lld"},
                "expected_linker": "wrapper-audit", "audit": lld_audit,
            })
            inventory["ld64_lld"] = {"path": str(lld), "sha256": _file_sha256(lld),
                                      **_tool_version([str(lld), "--version"]), "wrapper": lld_audit}
        except (ExperimentFailure, OSError) as error:
            failures.append({"arm": "ld64-lld", "reason": str(error)})

        if mold_binary is None:
            failures.append({"arm": "ld64-mold", "reason": "pinned mold source candidate was not built"})
        else:
            # The real binary is kept under the required ld64.mold alias so its
            # argv[0] selects the Mach-O implementation, not the ELF driver.
            link_dir = output.directory / "linkers"
            link_dir.mkdir(parents=True, exist_ok=True)
            mold_alias = link_dir / "ld64.mold"
            if mold_alias.exists() and not mold_alias.is_symlink():
                raise ExperimentFailure(f"refusing to replace non-symlink linker alias {mold_alias}")
            if mold_alias.is_symlink():
                mold_alias.unlink()
            mold_alias.symlink_to(_absolute_keep_alias(mold_binary))
            wrapper_dir = output.directory / "linker-wrappers"
            mold_wrapper = wrapper_dir / "ld64.mold"
            mold_audit = _make_audit_wrapper(mold_wrapper, mold_alias, "ld64.mold")
            arms.append({
                "id": "ld64-mold",
                "env": {"RUSTFLAGS": f"-C linker={clang} -C link-arg=--ld-path={mold_wrapper} -C link-arg=-v",
                        "CARGO_ENCODED_RUSTFLAGS": None, "ANIMATIX_LINKER_ARGV0": "ld64.mold"},
                "expected_linker": "wrapper-audit", "audit": mold_audit,
            })
            inventory["ld64_mold"] = {"path": str(mold_alias), "sha256": _file_sha256(mold_alias),
                                       "mold_binary_path": str(mold_binary),
                                       "mold_binary_sha256": _file_sha256(mold_binary),
                                       **_tool_version([str(mold_alias), "--version"]),
                                       "wrapper": mold_audit,
                                       "macho_cli_features": MOLD_MACHO_FEATURES}
        return arms, inventory, failures

    raise ExperimentFailure(f"unsupported linker experiment platform: {platform}")


def collect_elf_linker_evidence(executables: list[Path], expected: str,
                                allow_unknown: bool = False) -> list[dict]:
    """Inspect actual ELF output .comment sections instead of matching Cargo logs."""
    readelf = shutil.which("readelf")
    if not readelf:
        raise ExperimentFailure("readelf is unavailable; cannot prove Linux linker use from ELF artifacts")
    evidence = []
    for executable in executables:
        result = subprocess.run(
            [readelf, "--string-dump=.comment", str(executable)],
            capture_output=True, text=True, check=False,
        )
        if result.returncode:
            evidence.append({"artifact": str(executable), "detected": "unknown",
                             "readelf_exit_code": result.returncode,
                             "comment": (result.stdout + result.stderr).strip()})
            continue
        text = (result.stdout + result.stderr).strip()
        lower = text.lower()
        if "mold" in lower:
            detected = "mold"
        elif "lld" in lower:
            detected = "lld"
        elif re.search(r"\bgnu (?:ld|gold)(?:\b|[- ])", lower):
            detected = "gnu"
        else:
            detected = "unknown"
        evidence.append({"artifact": str(executable), "detected": detected, "comment": text})
    if not evidence:
        raise ExperimentFailure("readelf could not inspect any actual ELF .comment outputs")
    identities = {item["detected"] for item in evidence} - {"unknown"}
    if expected == "toolchain-default":
        if any(item["detected"] == "unknown" for item in evidence) or len(identities) != 1:
            raise ExperimentFailure(
                "default Linux linker artifacts must all identify one consistent linker: "
                f"{sorted(item['detected'] for item in evidence)}"
            )
    else:
        if identities - {expected}:
            raise ExperimentFailure(
                f"ELF artifacts disagree with the audited {expected} linker: "
                f"{sorted(item['detected'] for item in evidence)}"
            )
        if not allow_unknown and (not identities or any(item["detected"] == "unknown" for item in evidence)):
            raise ExperimentFailure(f"expected {expected} identity in every ELF artifact")
        if allow_unknown and not identities and all(item["detected"] == "unknown" for item in evidence):
            # The caller must also prove the wrapper was invoked for every
            # expected output before unknown .comment entries can be accepted.
            pass
    return evidence


def collect_wrapper_evidence(log_path: Path, expected: dict,
                             expected_outputs: list[Path] | None = None) -> list[dict]:
    try:
        records = [json.loads(line) for line in log_path.read_text(encoding="utf-8").splitlines() if line]
    except (OSError, json.JSONDecodeError) as error:
        raise ExperimentFailure(f"invalid linker wrapper audit log {log_path}: {error}") from error
    if not records:
        raise ExperimentFailure(f"linker wrapper was configured but never executed: {log_path}")
    for record in records:
        if record.get("callee_sha256") != expected["callee_sha256"]:
            raise ExperimentFailure(f"linker wrapper callee hash mismatch in {log_path}")
        if Path(record.get("callee", "")).name != Path(expected["callee_path"]).name and expected["argv0"] != "ld64.mold":
            raise ExperimentFailure(f"linker wrapper executed unexpected callee in {log_path}")
        if record.get("argv0") != expected["argv0"]:
            raise ExperimentFailure(f"linker wrapper used unexpected argv0 in {log_path}")
    if expected_outputs is not None:
        actual_outputs = {str(Path(record["output_path"]).resolve())
                          for record in records if record.get("output_path")}
        missing = sorted({str(Path(path).resolve()) for path in expected_outputs} - actual_outputs)
        if missing:
            raise ExperimentFailure(
                f"linker wrapper did not link every expected artifact in {log_path}: {missing[:8]}"
            )
    return records
