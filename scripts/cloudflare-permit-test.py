#!/usr/bin/env python3
"""Pure, network-free policy tests; mocked gh output is NOT signature evidence."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("permit", Path(__file__).with_name("cloudflare-permit.py"))
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)
SHA = "a" * 40
NOW = 1800000000


def metadata():
    return {"repository_id": p.REPOSITORY_ID, "repository": p.REPOSITORY, "ref": p.REF,
            "workflow": p.WORKFLOW, "site_sha": SHA, "sdk_source_sha": SHA, "producer_sha": SHA,
            "run_id": 123456, "run_attempt": 2, "needs_ci_result": "success", "artifact_id": 789,
            "archive_sha256": "b" * 64, "archive_size_bytes": 22546735}


def verified(permit_bytes):
    return [{"attestation": {}, "verificationResult": {
        "signature": {"certificate": {
            "sourceRepositoryIdentifier": str(p.REPOSITORY_ID),
            "sourceRepositoryURI": "https://github.com/" + p.REPOSITORY,
            "sourceRepositoryDigest": SHA, "sourceRepositoryRef": p.REF,
            "issuer": "https://token.actions.githubusercontent.com", "buildSignerURI": p.SIGNER,
            "buildSignerDigest": SHA, "runnerEnvironment": "github-hosted",
            "runInvocationURI": f"https://github.com/{p.REPOSITORY}/actions/runs/123456/attempts/2"}},
        "verifiedTimestamps": [{"type": "Tlog", "timestamp": "2027-01-15T08:00:00Z"}],
        "statement": {"predicateType": p.PREDICATE, "subject": [
            {"name": "release-permit.json", "digest": {"sha256": hashlib.sha256(permit_bytes).hexdigest()}}]}}}]


class PermitTests(unittest.TestCase):
    def setUp(self):
        self.permit = p.emit(metadata(), now=NOW)
        self.raw = json.dumps(self.permit).encode()

    def test_emit_shape_and_two_hour_lifetime(self):
        self.assertEqual(self.permit["expires_at"] - self.permit["issued_at"], 7200)
        self.assertEqual(self.permit["mode"], "build-only")
        self.assertEqual(p.validate_permit(self.permit, SHA, NOW), self.permit)

    def test_emit_rejects_failed_skipped_and_forged_metadata(self):
        for key, value in [("needs_ci_result", "failure"), ("needs_ci_result", "skipped"),
                           ("needs_ci_result", "cancelled"), ("repository_id", True),
                           ("repository_id", 42), ("repository", "fork/animatix"),
                           ("ref", "refs/heads/main"), ("workflow", ".github/workflows/ci.yml")]:
            with self.subTest(key=key, value=value), self.assertRaises(p.PermitError):
                p.emit(dict(metadata(), **{key: value}), now=NOW)
        with self.assertRaises(p.PermitError):
            p.emit(dict(metadata(), untrusted_extra="ignored?"), now=NOW)

    def test_same_sha_required_for_all_roles(self):
        for key in ("site_sha", "sdk_source_sha", "producer_sha"):
            bad = copy.deepcopy(self.permit)
            bad[key] = "c" * 40
            with self.subTest(key=key), self.assertRaises(p.PermitError):
                p.validate_permit(bad, SHA, NOW)
        with self.assertRaises(p.PermitError):
            p.validate_permit(self.permit, "c" * 40, NOW)

    def test_expiry_future_and_lifetime(self):
        for current in [NOW - 61, NOW + p.TTL, NOW + p.TTL + 1]:
            with self.subTest(now=current), self.assertRaises(p.PermitError):
                p.validate_permit(self.permit, SHA, current)
        bad = copy.deepcopy(self.permit)
        bad["expires_at"] += 1
        with self.assertRaises(p.PermitError):
            p.validate_permit(bad, SHA, NOW)

    def test_reject_production_or_other_destination(self):
        for field, value in [("mode", "deploy"), ("ref", "refs/heads/main"), ("schema_version", True)]:
            with self.subTest(field=field), self.assertRaises(p.PermitError):
                p.validate_permit(dict(self.permit, **{field: value}), SHA, NOW)
        for key in ["account_id", "trigger_uuid"]:
            bad = copy.deepcopy(self.permit)
            bad["cloudflare"][key] = "another"
            with self.subTest(field=key), self.assertRaises(p.PermitError):
                p.validate_permit(bad, SHA, NOW)

    def test_reject_archive_bounds_and_invalid_ids(self):
        for key, value in [("archive_size_bytes", 0), ("archive_size_bytes", p.MAX_ARCHIVE + 1),
                           ("artifact_id", True), ("artifact_id", -1), ("archive_sha256", "g" * 64)]:
            bad = copy.deepcopy(self.permit)
            bad["sdk"][key] = value
            with self.subTest(key=key), self.assertRaises(p.PermitError):
                p.validate_permit(bad, SHA, NOW)

    def test_duplicate_json_rejected(self):
        with self.assertRaises(p.PermitError):
            p.decode('{"mode":"build-only","mode":"deploy"}')

    def test_verified_schema_uses_certificate_fields_not_predicate_claim(self):
        result = verified(self.raw)
        self.assertTrue(p.validate_verified_output(result, self.permit, self.raw))
        cert = result[0]["verificationResult"]["signature"]["certificate"]
        for key, value in [("sourceRepositoryIdentifier", "different"),
                           ("sourceRepositoryIdentifier", p.REPOSITORY_ID),
                           ("sourceRepositoryDigest", "c" * 40),
                           ("sourceRepositoryRef", "refs/heads/main"),
                           ("buildSignerDigest", "c" * 40),
                           ("buildSignerURI", "https://github.com/fork/repo/ci.yml"),
                           ("runnerEnvironment", "self-hosted"),
                           ("runInvocationURI", "https://github.com/j4ger/animatix/actions/runs/123456/attempts/1")]:
            bad = copy.deepcopy(result)
            bad[0]["verificationResult"]["signature"]["certificate"][key] = value
            with self.subTest(field=key), self.assertRaises(p.PermitError):
                p.validate_verified_output(bad, self.permit, self.raw)
        bad = copy.deepcopy(result)
        bad[0]["verificationResult"]["signature"]["certificate"] = {"extensions": cert}
        with self.assertRaises(p.PermitError):
            p.validate_verified_output(bad, self.permit, self.raw)

    def test_signed_subject_and_predicate_must_match(self):
        for change in ("digest", "predicate", "timestamp"):
            result = verified(self.raw)
            v = result[0]["verificationResult"]
            if change == "digest":
                v["statement"]["subject"][0]["digest"]["sha256"] = "0" * 64
            elif change == "predicate":
                v["statement"]["predicateType"] = "untrusted/custom"
            else:
                v["verifiedTimestamps"] = []
            with self.subTest(change=change), self.assertRaises(p.PermitError):
                p.validate_verified_output(result, self.permit, self.raw)
        with self.assertRaises(p.PermitError):
            p.validate_verified_output([], self.permit, self.raw)

    def test_command_pins_offline_and_identity_policy(self):
        command = p.verify_command("/pinned/gh", "permit", "bundle", "root", SHA)
        for key, value in [("--bundle", "bundle"), ("--custom-trusted-root", "root"),
                           ("--source-ref", p.REF), ("--source-digest", SHA), ("--signer-digest", SHA),
                           ("--repo", p.REPOSITORY), ("--signer-workflow", p.REPOSITORY + "/" + p.WORKFLOW)]:
            self.assertEqual(command[command.index(key) + 1], value)
        self.assertIn("--deny-self-hosted-runners", command)
        self.assertNotIn("--no-public-good", command)

    def test_pure_old_result_rejection_does_not_infer_run_order(self):
        self.assertTrue(p.accept_result(SHA, SHA))
        # Old A finishing after newer B cannot be accepted even if A has a higher run ID.
        with self.assertRaises(p.PermitError):
            p.accept_result("b" * 40, SHA)
        with self.assertRaises(p.PermitError):
            p.accept_result("main", SHA)

    def test_verify_invokes_crypto_before_policy_with_no_credentials(self):
        with tempfile.TemporaryDirectory() as root:
            base = Path(root)
            gh = base / "gh"
            gh.write_text("not executed: subprocess is mocked")
            gh.chmod(0o700)
            permit, bundle, trust = [base / x for x in ("permit.json", "bundle.json", "roots.jsonl")]
            permit.write_bytes(self.raw)
            bundle.write_text("{}")
            trust.write_text("{}")
            outputs = [subprocess.CompletedProcess([], 0, stdout="gh version 2.104.0 (2026-11-12)\n"),
                       subprocess.CompletedProcess([], 0, stdout=json.dumps(verified(self.raw)).encode())]
            with patch.object(p.subprocess, "run", side_effect=outputs) as run, patch.object(p.time, "time", return_value=NOW), \
                    patch.dict(os.environ, {"GH_TOKEN": "do-not-inherit", "GITHUB_TOKEN": "do-not-inherit"}):
                self.assertEqual(p.verify(permit, bundle, trust, gh, SHA, SHA), self.permit)
                self.assertEqual(run.call_count, 2)
                for call in run.call_args_list:
                    self.assertNotIn("GH_TOKEN", call.kwargs["env"])
                    self.assertNotIn("GITHUB_TOKEN", call.kwargs["env"])
                    self.assertTrue(call.kwargs["check"])
            with patch.object(p.subprocess, "run", side_effect=subprocess.CalledProcessError(1, "gh")), \
                    patch.object(p, "validate_verified_output") as policy:
                with self.assertRaises(subprocess.CalledProcessError):
                    p.verify(permit, bundle, trust, gh, SHA)
                policy.assert_not_called()


if __name__ == "__main__":
    unittest.main()
