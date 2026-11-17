#!/usr/bin/env python3
"""Fresh RustSec audit with fail-closed, public-only provenance evidence."""
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import tomllib
import uuid

ROOT = Path(__file__).resolve().parents[2]
SHA = re.compile(r"[0-9a-f]{40}(?:[0-9a-f]{24})?\Z")
ADVISORY = re.compile(r"RUSTSEC-[0-9]{4}-[0-9]{4,}\Z")
VERSION = re.compile(r"cargo-audit(?:-audit)? [0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9.-]+)?\Z")


class Incomplete(Exception):
    """A fixed public reason, never subprocess output or a local path."""


def now():
    return datetime.now(timezone.utc).isoformat()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def allowlist(path):
    if not path.exists():
        return [], None
    raw = path.read_bytes()
    entries = []
    for line in raw.decode("utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        advisory, separator, reason = line.partition("\t")
        if not separator or not ADVISORY.fullmatch(advisory) or not reason.strip():
            raise Incomplete("invalid_allowlist")
        if advisory in entries:
            raise Incomplete("duplicate_allowlist_entry")
        entries.append(advisory)
    return entries, digest(raw)


def metadata(command, cwd):
    result = subprocess.run(command, cwd=cwd, capture_output=True, timeout=30)
    if result.returncode != 0:
        raise Incomplete("metadata_unavailable")
    return result.stdout.decode("utf-8").strip()


def git_sha(path):
    value = metadata(["git", "-C", str(path), "rev-parse", "HEAD"], ROOT)
    if not SHA.fullmatch(value):
        raise Incomplete("invalid_git_identity")
    return value


def cargo_home():
    configured = os.environ.get("CARGO_HOME")
    if configured is None:
        return Path.home() / ".cargo"
    if not configured:
        raise Incomplete("invalid_cargo_home")
    home = Path(configured)
    return home if home.is_absolute() else ROOT / home


def default_audit_policy(home):
    # cargo-audit can silently disable fetching or add filters in these files.
    # This gate has one reviewed policy: defaults plus our annotated allowlist.
    if (ROOT / ".cargo/audit.toml").exists() or (home / "audit.toml").exists():
        raise Incomplete("unreviewed_audit_config")


def lock_identity():
    raw = (ROOT / "Cargo.lock").read_bytes()
    parsed = tomllib.loads(raw.decode("utf-8"))
    packages = parsed.get("package")
    if not isinstance(packages, list) or not packages or not all(isinstance(p, dict) for p in packages):
        raise Incomplete("invalid_lockfile")
    return digest(raw), len(packages)


def integer(value):
    return type(value) is int and value >= 0


def validate_report(raw, expected_db, dependency_count, ignores):
    report = json.loads(raw)
    if not isinstance(report, dict):
        raise Incomplete("invalid_report")
    database = report.get("database")
    lock = report.get("lockfile")
    settings = report.get("settings")
    vulns = report.get("vulnerabilities")
    if not all(isinstance(section, dict) for section in (database, lock, settings, vulns)):
        raise Incomplete("missing_report_metadata")
    if database.get("last-commit") != expected_db or not integer(database.get("advisory-count")) or database["advisory-count"] == 0:
        raise Incomplete("database_identity_mismatch")
    if type(lock.get("dependency-count")) is not int or lock["dependency-count"] != dependency_count:
        raise Incomplete("lockfile_identity_mismatch")
    if settings.get("ignore") != ignores:
        raise Incomplete("audit_policy_mismatch")
    # Older cargo-audit encodes unrestricted targets as null; newer as [].
    if any(key not in settings or settings[key] not in (None, []) for key in ("target_arch", "target_os")):
        raise Incomplete("audit_policy_mismatch")
    if "severity" not in settings or settings["severity"] is not None:
        raise Incomplete("audit_policy_mismatch")
    count, found, entries = vulns.get("count"), vulns.get("found"), vulns.get("list")
    if not integer(count) or type(found) is not bool or not isinstance(entries, list):
        raise Incomplete("invalid_vulnerability_report")
    if count != len(entries) or found != (count > 0):
        raise Incomplete("inconsistent_vulnerability_report")
    ids = []
    for entry in entries:
        advisory = entry.get("advisory") if isinstance(entry, dict) else None
        ident = advisory.get("id") if isinstance(advisory, dict) else None
        if not isinstance(ident, str) or not ADVISORY.fullmatch(ident):
            raise Incomplete("invalid_advisory_identity")
        if ident in ignores:
            raise Incomplete("audit_policy_mismatch")
        ids.append(ident)
    return count, sorted(set(ids))


def write_summary(path, summary):
    # Fixed destination only; replace stale evidence with this invocation's data.
    temporary = path.with_name("summary-" + uuid.uuid4().hex + ".tmp")
    try:
        with temporary.open("x", encoding="utf-8") as stream:
            json.dump(summary, stream, sort_keys=True)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def audit(summary):
    home = cargo_home()
    default_audit_policy(home)
    ignores, policy_hash = allowlist(ROOT / "tools/quality/audit-allow.txt")
    if ignores:
        print("Explicitly allowed advisories (reviewed reasons):")
        for line in (ROOT / "tools/quality/audit-allow.txt").read_text().splitlines():
            if line and not line.startswith("#"):
                print("  " + line.replace("\t", ": ", 1))
    summary["ignored_advisories"] = ignores
    summary["allowlist_sha256"] = policy_hash
    summary["checkout_sha"] = git_sha(ROOT)
    summary["lockfile_sha256"], dependencies = lock_identity()
    version = metadata(["cargo", "audit", "--version"], ROOT)
    if not VERSION.fullmatch(version):
        raise Incomplete("invalid_tool_version")
    summary["tool_version"] = version
    database = home / "advisory-db"
    command = ["cargo", "audit", "--json", "--db", str(database), "--file", str(ROOT / "Cargo.lock")]
    for ident in ignores:
        command.extend(["--ignore", ident])
    # No --no-fetch / --stale: a failed RustSec database fetch cannot pass.
    # cargo-audit 0.22 may skip yanked checks after a crates.io index warning.
    # Its JSON cannot prove that coverage; our verdict covers RustSec only.
    result = subprocess.run(command, cwd=ROOT, capture_output=True, timeout=600)
    summary["audit_exit_code"] = result.returncode
    summary["database_sha"] = git_sha(database)
    default_audit_policy(home)
    current_ignores, current_policy = allowlist(ROOT / "tools/quality/audit-allow.txt")
    if (git_sha(ROOT) != summary["checkout_sha"] or lock_identity()[0] != summary["lockfile_sha256"]
            or current_ignores != ignores or current_policy != policy_hash):
        raise Incomplete("inputs_changed_during_audit")
    count, ids = validate_report(result.stdout, summary["database_sha"], dependencies, ignores)
    summary["vulnerability_count"], summary["advisory_ids"] = count, ids
    if result.returncode in (0, 1) and count > 0:
        summary["verdict"], summary["reason"] = "vulnerability", "reported_vulnerabilities"
        return 1
    if result.returncode != 0:
        raise Incomplete("audit_command_failed")
    summary["verdict"], summary["reason"] = "passed", "fresh_audit_completed"
    return 0


def main():
    started = time.monotonic()
    summary = {"schema": 1, "scope": "rustsec_vulnerabilities", "yanked_coverage": "unverified", "run_id": uuid.uuid4().hex, "started_at": now(), "finished_at": None,
               "duration_seconds": None, "checkout_sha": None, "lockfile_sha256": None,
               "allowlist_sha256": None, "tool_version": None, "database_sha": None,
               "audit_exit_code": None, "exit_code": 2, "verdict": "audit_incomplete",
               "reason": "audit_not_completed", "vulnerability_count": None,
               "advisory_ids": [], "ignored_advisories": []}
    path = ROOT / "target/ci/security-audit/summary.json"
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.unlink(missing_ok=True)
        write_summary(path, summary)
    except OSError:
        print("security audit: evidence initialization failed", file=sys.stderr)
        return 2
    status = 2
    try:
        status = audit(summary)
    except Incomplete as error:
        summary["reason"] = str(error)
    except subprocess.TimeoutExpired:
        summary["reason"] = "command_timeout"
    except (OSError, UnicodeError, json.JSONDecodeError, tomllib.TOMLDecodeError):
        summary["reason"] = "io_or_report_error"
    summary.update(exit_code=status, finished_at=now(), duration_seconds=time.monotonic() - started)
    try:
        write_summary(path, summary)
    except OSError:
        print("security audit: evidence finalization failed", file=sys.stderr)
        return 2
    print(f"security audit: {summary['verdict']} ({summary['reason']})")
    return status


if __name__ == "__main__":
    sys.exit(main())
