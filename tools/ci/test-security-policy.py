#!/usr/bin/env python3
"""Keep daily security checks independent of rolling-build eligibility."""
import importlib.util
import json
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("plan", ROOT / "tools/ci/ci-plan.py")
PLAN = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PLAN)


class SecurityPolicyTests(unittest.TestCase):
    def test_real_registry_audits_once_outside_check_lint(self):
        jobs = json.loads(subprocess.check_output(
            ["bash", "tools/ci/ci.sh", "--matrix", "pr"], cwd=ROOT, text=True))["include"]
        audits = [j for j in jobs if "audit" in j["gate"].split()]
        self.assertEqual([j["id"] for j in audits], ["security-audit"])
        self.assertEqual(audits[0]["shell"], "nix")
        release = PLAN.make_plan_from_classification(
            {"changed_paths": ["docs/spec.md"]}, "workflow_call", "base", "head", jobs, True)
        self.assertIn("security-audit", release["job_ids"])
        self.assertTrue(release["force_full"])

    def test_nightly_audit_never_depends_on_change_check(self):
        text = (ROOT / ".github/workflows/nightly.yml").read_text()
        audit = text.split("  security-audit:\n", 1)[1].split("\n  build:\n", 1)[0]
        self.assertNotIn("\n    if:", audit)
        self.assertNotIn("\n    needs:", audit)
        self.assertIn("ref: ${{ github.sha }}", audit)
        self.assertIn("nix develop --command tools/ci/ci.sh gate audit", audit)
        self.assertIn("contents: read", audit)
        self.assertNotIn("continue-on-error", audit)
        self.assertNotIn("--no-fetch", audit)
        self.assertIn("if: always()", audit)
        self.assertIn("path: target/ci/security-audit/summary.json", audit)
        self.assertIn("if-no-files-found: error", audit)
        build = text.split("  build:\n", 1)[1]
        self.assertIn("needs: [check, security-audit]", build)
        self.assertIn("if: needs.check.outputs.should_build == 'true'", build)
        self.assertNotIn("always()", build)

    def test_release_and_required_still_pin_and_block(self):
        release = (ROOT / ".github/workflows/release.yml").read_text()
        self.assertIn("uses: ./.github/workflows/ci.yml", release)
        self.assertIn("checkout-ref: ${{ needs.version.outputs.commit }}", release)
        self.assertIn("needs: [gates, version]", release)
        ci = (ROOT / ".github/workflows/ci.yml").read_text()
        self.assertIn("ref: ${{ inputs.checkout-ref || github.sha }}", ci)
        required = ci.split("  required:\n", 1)[1]
        self.assertIn("needs: [plan, gate]", required)
        self.assertIn("tools/ci/ci.sh gate ci-result", required)

    def test_ordinary_pages_push_accepts_daily_window_without_status_latch(self):
        jobs = json.loads(subprocess.check_output(
            ["bash", "tools/ci/ci.sh", "--matrix", "pr"], cwd=ROOT, text=True))["include"]
        classification = {"changed_paths": ["docs/spec.md"], "package_seeds": [],
                          "affected_packages": [], "docs_generator_changed": False,
                          "examples_changed": False, "web_changed": False,
                          "packaging_changed": False}
        plan = PLAN.make_plan_from_classification(classification, "push", "base", "head",
                                                  jobs, True, {"trusted": True})
        self.assertNotIn("security-audit", plan["job_ids"])
        self.assertIn("web-build", plan["job_ids"])
        ci = (ROOT / ".github/workflows/ci.yml").read_text()
        signing = ci.split("name: Sign Cloudflare Pages SDK", 1)[1]
        self.assertIn("required", signing)

    def test_security_job_avoids_build_cache_and_disk_deletion(self):
        ci = (ROOT / ".github/workflows/ci.yml").read_text()
        for step in ("Free disk space", "Rust cache"):
            block = ci.split(f"      - name: {step}\n", 1)[1].split("      - name:", 1)[0]
            self.assertIn("matrix.id != 'security-audit'", block)
        self.assertIn("name: security-audit-${{ github.run_attempt }}", ci)
        self.assertIn("path: target/ci/security-audit/summary.json", ci)


if __name__ == "__main__":
    unittest.main()
