#!/usr/bin/env python3
"""Hermetic audit contracts; no real cargo, git fetch, or network access."""
import contextlib
from datetime import datetime, timedelta
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("security_audit", Path(__file__).with_name("security-audit.py"))
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)
CHECKOUT = "a" * 40
DATABASE = "b" * 40
IGNORED = "RUSTSEC-2026-0194"
FOUND = "RUSTSEC-2026-1234"


def report(count=0):
    return {"database": {"advisory-count": 123, "last-commit": DATABASE},
            "lockfile": {"dependency-count": 1},
            "settings": {"ignore": [IGNORED], "target_arch": [], "target_os": [], "severity": None},
            "vulnerabilities": {"found": bool(count), "count": count,
                                "list": [{"advisory": {"id": FOUND},
                                          "package": {"source": "secret-source-path"}} for _ in range(count)]},
            "warnings": {}}


class SecurityAuditTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.home = self.root / "cargo-home"
        self.home.mkdir()
        self.allow = self.root / "tools/quality/audit-allow.txt"
        self.allow.parent.mkdir(parents=True)
        self.allow.write_text(f"# Reviewed exception\n{IGNORED}\ttransitive pin; remove after upgrade\n")
        (self.root / "Cargo.lock").write_text('version = 4\n[[package]]\nname = "fixture"\nversion = "1.0.0"\n')
        self.patcher = patch.object(AUDIT, "ROOT", self.root)
        self.patcher.start()
        self.addCleanup(self.patcher.stop)
        self.env = patch.dict(os.environ, {"CARGO_HOME": str(self.home), "SECRET_TOKEN": "DO-NOT-EXPOSE"})
        self.env.start()
        self.addCleanup(self.env.stop)
        self.audit_result = subprocess.CompletedProcess([], 0, json.dumps(report()).encode(), b"private stderr DO-NOT-EXPOSE")
        self.git_status = 0
        self.version = b"cargo-audit 0.22.0\n"
        self.calls = []
        self.summary_path = self.root / "target/ci/security-audit/summary.json"

    def fake_run(self, command, **kwargs):
        self.calls.append(command)
        self.assertTrue(kwargs["capture_output"])
        if command[0] == "git":
            sha = DATABASE if Path(command[2]) == self.home / "advisory-db" else CHECKOUT
            return subprocess.CompletedProcess(command, self.git_status, (sha + "\n").encode(), b"private git output")
        if command == ["cargo", "audit", "--version"]:
            return subprocess.CompletedProcess(command, 0, self.version, b"")
        self.assertEqual(command[:3], ["cargo", "audit", "--json"])
        return self.audit_result

    def invoke(self, side_effect=None):
        with patch.object(AUDIT.subprocess, "run", side_effect=side_effect or self.fake_run), \
                contextlib.redirect_stdout(io.StringIO()) as out, contextlib.redirect_stderr(io.StringIO()) as err:
            code = AUDIT.main()
        result = json.loads(self.summary_path.read_text()) if self.summary_path.exists() else None
        return code, result, out.getvalue() + err.getvalue()

    def set_report(self, value, status=0):
        self.audit_result = subprocess.CompletedProcess([], status, json.dumps(value).encode(), b"private failure message")

    def assert_incomplete(self, result):
        code, summary, logs = result
        self.assertEqual(code, 2)
        self.assertEqual(summary["verdict"], "audit_incomplete")
        self.assertEqual(summary["exit_code"], 2)
        self.assertNotIn("DO-NOT-EXPOSE", logs + json.dumps(summary))
        self.assertNotIn(str(self.root), logs + json.dumps(summary))

    def test_success_has_real_provenance_and_fresh_arguments(self):
        code, summary, logs = self.invoke()
        self.assertEqual(code, 0)
        self.assertEqual(summary["verdict"], "passed")
        self.assertEqual(summary["checkout_sha"], CHECKOUT)
        self.assertEqual(summary["database_sha"], DATABASE)
        self.assertEqual(summary["tool_version"], "cargo-audit 0.22.0")
        self.assertEqual(summary["lockfile_sha256"], hashlib.sha256((self.root / "Cargo.lock").read_bytes()).hexdigest())
        self.assertEqual(summary["allowlist_sha256"], hashlib.sha256(self.allow.read_bytes()).hexdigest())
        self.assertEqual(summary["audit_exit_code"], 0)
        self.assertEqual(summary["vulnerability_count"], 0)
        self.assertEqual(summary["ignored_advisories"], [IGNORED])
        command = next(c for c in self.calls if "--json" in c)
        self.assertNotIn("--no-fetch", command)
        self.assertNotIn("--stale", command)
        self.assertEqual(command[-2:], ["--ignore", IGNORED])
        self.assertEqual(command[command.index("--db") + 1], str(self.home / "advisory-db"))
        self.assertEqual(command[command.index("--file") + 1], str(self.root / "Cargo.lock"))
        self.assertNotIn("private stderr", logs)
        self.assertNotIn("DO-NOT-EXPOSE", logs + json.dumps(summary))
        started, finished = map(datetime.fromisoformat, (summary["started_at"], summary["finished_at"]))
        self.assertEqual(started.utcoffset(), timedelta(0))
        self.assertGreaterEqual(finished, started)
        self.assertGreaterEqual(summary["duration_seconds"], 0)

    def test_nix_tool_version_and_annotated_ignores_are_visible(self):
        self.version = b"cargo-audit-audit 0.22.0\n"
        code, summary, logs = self.invoke()
        self.assertEqual(code, 0)
        self.assertEqual(summary["tool_version"], "cargo-audit-audit 0.22.0")
        self.assertIn(IGNORED + ": transitive pin; remove after upgrade", logs)

    def test_unknown_tool_version_prefix_is_rejected(self):
        self.version = b"cargo-audit-audit-audit 0.22.0\n"
        result = self.invoke()
        self.assert_incomplete(result)
        self.assertEqual(result[1]["reason"], "invalid_tool_version")
        self.assertFalse(any("--json" in command for command in self.calls))

    def test_vulnerabilities_are_failure_and_ids_only(self):
        self.set_report(report(2), 1)
        code, summary, logs = self.invoke()
        self.assertEqual(code, 1)
        self.assertEqual(summary["verdict"], "vulnerability")
        self.assertEqual(summary["vulnerability_count"], 2)
        self.assertEqual(summary["advisory_ids"], [FOUND])
        self.assertNotIn("secret-source-path", logs + json.dumps(summary))

    def test_zero_exit_does_not_hide_reported_vulnerabilities(self):
        self.set_report(report(1), 0)
        code, summary, _ = self.invoke()
        self.assertEqual(code, 1)
        self.assertEqual(summary["verdict"], "vulnerability")

    def test_nonzero_without_vulnerabilities_is_incomplete(self):
        for status in (1, 2, -15):
            with self.subTest(status=status):
                self.set_report(report(), status)
                self.assert_incomplete(self.invoke())

    def test_fatal_exit_with_vulnerability_json_is_incomplete(self):
        self.set_report(report(1), 2)
        self.assert_incomplete(self.invoke())

    def test_network_failure_never_passes(self):
        self.audit_result = subprocess.CompletedProcess([], 2, b"", b"network timeout DO-NOT-EXPOSE")
        self.assert_incomplete(self.invoke())

    def test_malformed_reports_fail_even_at_exit_zero(self):
        for raw in (b"", b"{}", b"[]", b"null", b"not-json", b"\xff"):
            with self.subTest(raw=raw):
                self.audit_result = subprocess.CompletedProcess([], 0, raw, b"")
                self.assert_incomplete(self.invoke())

    def test_missing_metadata_fails(self):
        for section in ("database", "lockfile", "settings", "vulnerabilities"):
            with self.subTest(section=section):
                value = report()
                del value[section]
                self.set_report(value)
                self.assert_incomplete(self.invoke())

    def test_database_metadata_must_match_observed_head(self):
        for key, bad in (("last-commit", "c" * 40), ("last-commit", None),
                         ("advisory-count", 0), ("advisory-count", True), ("advisory-count", "123")):
            with self.subTest(key=key, bad=bad):
                value = report()
                value["database"][key] = bad
                self.set_report(value)
                self.assert_incomplete(self.invoke())

    def test_dependency_count_must_match_lockfile(self):
        for count in (0, 2, True, "1"):
            value = report()
            value["lockfile"]["dependency-count"] = count
            self.set_report(value)
            self.assert_incomplete(self.invoke())

    def test_policy_metadata_cannot_hide_additional_ignores_or_filters(self):
        for key, bad in (("ignore", [IGNORED, FOUND]), ("ignore", []),
                         ("target_arch", ["x86_64"]), ("target_os", ["linux"]), ("severity", "high")):
            with self.subTest(key=key):
                value = report()
                value["settings"][key] = bad
                self.set_report(value)
                self.assert_incomplete(self.invoke())

    def test_old_unfiltered_target_schema_is_accepted(self):
        value = report()
        value["settings"].update(target_arch=None, target_os=None)
        self.set_report(value)
        self.assertEqual(self.invoke()[0], 0)

    def test_inconsistent_vulnerability_counts_fail(self):
        for change in ({"count": True}, {"count": -1}, {"found": 0}, {"list": None},
                       {"count": 1}, {"found": True}):
            with self.subTest(change=change):
                value = report()
                value["vulnerabilities"].update(change)
                self.set_report(value)
                self.assert_incomplete(self.invoke())

    def test_invalid_advisory_ids_fail(self):
        for ident in (IGNORED, "private-source-location", None):
            value = report(1)
            value["vulnerabilities"]["list"][0]["advisory"]["id"] = ident
            self.set_report(value, 1)
            self.assert_incomplete(self.invoke())

    def test_allowlist_rejects_unannotated_empty_reason_duplicate_and_bad_id(self):
        for contents in (IGNORED, IGNORED + "\t", IGNORED + "\t   ", "--no-fetch\treason",
                         f"{IGNORED}\treason\n{IGNORED}\treason"):
            with self.subTest(contents=contents):
                self.allow.write_text(contents)
                self.assert_incomplete(self.invoke())
                self.assertFalse(self.calls)

    def test_allowlist_comments_and_optional_absence(self):
        self.assertEqual(AUDIT.allowlist(self.allow)[0], [IGNORED])
        self.allow.unlink()
        value = report()
        value["settings"]["ignore"] = []
        self.set_report(value)
        self.assertEqual(self.invoke()[0], 0)
        self.assertNotIn("--ignore", self.calls[-3])

    def test_local_or_global_config_fails_closed(self):
        for path in (self.root / ".cargo/audit.toml", self.home / "audit.toml"):
            path.parent.mkdir(exist_ok=True)
            path.write_text("[database]\nfetch = false\n")
            self.assert_incomplete(self.invoke())
            self.assertEqual(self.calls, [])
            path.unlink()

    def test_missing_git_or_tool_metadata_fails(self):
        self.git_status = 1
        self.assert_incomplete(self.invoke())
        self.git_status = 0
        self.version = b"secret tool output"
        self.assert_incomplete(self.invoke())

    def test_missing_database_head_fails(self):
        def missing(command, **kwargs):
            if command[0] == "git" and Path(command[2]) == self.home / "advisory-db":
                return subprocess.CompletedProcess(command, 1, b"", b"not a git repo")
            return self.fake_run(command, **kwargs)
        self.assert_incomplete(self.invoke(missing))

    def test_missing_or_invalid_lockfile_fails(self):
        for content in ("bad = [", "version = 4\n", "package = []"):
            (self.root / "Cargo.lock").write_text(content)
            self.assert_incomplete(self.invoke())
        (self.root / "Cargo.lock").unlink()
        self.assert_incomplete(self.invoke())

    def test_input_mutation_during_audit_is_incomplete(self):
        def changing(command, **kwargs):
            result = self.fake_run(command, **kwargs)
            if "--json" in command:
                (self.root / "Cargo.lock").write_text('[[package]]\nname = "changed"\n')
            return result
        self.assert_incomplete(self.invoke(changing))

    def test_timeout_is_incomplete(self):
        self.assert_incomplete(self.invoke(lambda *a, **k: (_ for _ in ()).throw(subprocess.TimeoutExpired("cargo", 600))))

    def test_old_success_evidence_is_replaced_before_network_failure(self):
        self.summary_path.parent.mkdir(parents=True)
        self.summary_path.write_text('{"verdict":"passed","run_id":"old"}')
        self.audit_result = subprocess.CompletedProcess([], 2, b"", b"network failure")
        result = self.invoke()
        self.assert_incomplete(result)
        self.assertNotEqual(result[1]["run_id"], "old")

    def test_evidence_initialization_failure_prevents_audit(self):
        with patch.object(AUDIT, "write_summary", side_effect=OSError("private")):
            code, summary, logs = self.invoke()
        self.assertEqual(code, 2)
        self.assertEqual(self.calls, [])
        self.assertIsNone(summary)
        self.assertNotIn("private", logs)

    def test_final_evidence_failure_returns_failure_not_old_success(self):
        write = AUDIT.write_summary
        calls = 0
        def fail_final(path, summary):
            nonlocal calls
            calls += 1
            if calls > 1:
                raise OSError("full disk")
            return write(path, summary)
        with patch.object(AUDIT, "write_summary", side_effect=fail_final):
            code, summary, _ = self.invoke()
        self.assertEqual(code, 2)
        self.assertEqual(summary["verdict"], "audit_incomplete")
        self.assertEqual(summary["reason"], "audit_not_completed")

    def test_pass_does_not_claim_registry_yanked_coverage(self):
        self.invoke()
        summary = json.loads(self.summary_path.read_text())
        self.assertEqual(summary["scope"], "rustsec_vulnerabilities")
        self.assertEqual(summary["yanked_coverage"], "unverified")

    def test_only_public_summary_is_written(self):
        self.invoke()
        self.assertEqual(list((self.root / "target").rglob("*.*")), [self.summary_path])
        self.assertEqual(set(json.loads(self.summary_path.read_text())), {
            "schema", "scope", "yanked_coverage", "run_id", "started_at", "finished_at", "duration_seconds", "checkout_sha",
            "lockfile_sha256", "allowlist_sha256", "tool_version", "database_sha", "audit_exit_code",
            "exit_code", "verdict", "reason", "vulnerability_count", "advisory_ids", "ignored_advisories"})


if __name__ == "__main__":
    unittest.main()
