#!/usr/bin/env python3
"""Regression tests for CI environment routing and fresh-checkout doc paths."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
CI = ROOT / "tools/ci/ci.sh"
spec = importlib.util.spec_from_file_location("doc_paths", ROOT / "tools/docs/check-doc-paths.py")
doc_paths = importlib.util.module_from_spec(spec)
spec.loader.exec_module(doc_paths)


def ci(*args, **env):
    return subprocess.run(["bash", str(CI), *args], cwd=ROOT,
                          env={**os.environ, **env}, text=True, capture_output=True)


class Environments(unittest.TestCase):
    def test_reusable_context_and_permission_ceiling(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        release = (ROOT / ".github/workflows/release.yml").read_text()
        caller = release.split("  gates:", 1)[1].split("  plan:", 1)[0]
        for permission in ("contents: write", "id-token: write", "actions: read", "attestations: write"):
            self.assertIn(permission, caller)
        self.assertIn("ANIMATIX_CI_REUSABLE: ${{ contains(toJSON(inputs), '\"checkout-ref\"') }}", workflow)
        self.assertIn('${ANIMATIX_CI_REUSABLE:-false}', CI.read_text())


    def test_linux_registry_and_jobs_require_nix(self):
        listed = ci("--list")
        self.assertEqual(listed.returncode, 0, listed.stderr)
        rows = listed.stdout.splitlines()
        self.assertTrue(rows)
        for row in rows:
            gate, shell, platform, setup, stage = row.split("\t")
            if platform == "ubuntu-latest" and stage != "control":
                self.assertIn(shell, {"nix", "web", "web-ci", "dist-driver"}, gate)
                self.assertEqual(setup, "nix", gate)
                self.assertEqual(shell == "dist-driver", gate == "dist-linux")
        matrix = ci("--matrix")
        self.assertEqual(matrix.returncode, 0, matrix.stderr)
        jobs = json.loads(matrix.stdout)["include"]
        self.assertTrue(jobs)
        for job in jobs:
            if job["os"] == "ubuntu-latest":
                self.assertNotEqual(job["shell"], "native")

    def test_registry_rejects_native_linux_and_wrong_job_shell(self):
        validator = ROOT / "tools/ci/ci-registry-check.py"
        cases = [
            ("fmt|native|ubuntu-latest|rust-stable|pr", "Fast|fmt|native|ubuntu-latest|rust-stable|pr|fast"),
            ("fmt|nix|ubuntu-latest|nix|pr", "Fast|fmt|web-ci|ubuntu-latest|nix|pr|fast"),
            ("fmt|dist-driver|ubuntu-latest|nix|pr", "Fast|fmt|dist-driver|ubuntu-latest|nix|pr|fast"),
        ]
        for gates, jobs in cases:
            result = subprocess.run(["python3", str(validator), gates, jobs], capture_output=True)
            self.assertNotEqual(result.returncode, 0)

    def test_matrix_rejects_composite_control_and_unknown_stages(self):
        for stage in ("site", "control", "unknown"):
            with self.subTest(stage=stage):
                matrix = ci("--matrix", stage)
                self.assertEqual(matrix.returncode, 2)
                self.assertIn("supports pr|release only", matrix.stderr)
        self.assertNotEqual(ci("all", "unknown").returncode, 0)
        all_experiments = ci("all", "experiment")
        self.assertNotEqual(all_experiments.returncode, 0)
        self.assertIn("unknown stage", all_experiments.stderr)

    def test_repeated_gate_resolves_current_platform(self):
        for platform, expected in (("Linux", "nix develop --command"), ("macOS", ""), ("Windows", "")):
            result = ci("shell", "eparts", RUNNER_OS=platform)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), expected)

    def test_missing_or_wrong_nix_shell_fails_before_gate(self):
        for inside, marker in (("", ""), ("impure", "web-ci"), ("impure", "")):
            result = ci("gate", "fmt", RUNNER_OS="Linux", IN_NIX_SHELL=inside, ANIMATIX_CI_SHELL=marker)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("shell=nix", result.stderr)

    def test_ci_checkouts_preserve_release_and_event_ref_pinning(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        lines = workflow.splitlines()
        checkout_refs = []
        for index, line in enumerate(lines):
            if line.strip() != "- uses: actions/checkout@v5":
                continue
            indent = len(line) - len(line.lstrip())
            body = []
            for candidate in lines[index + 1:]:
                if candidate.strip() and len(candidate) - len(candidate.lstrip()) <= indent:
                    break
                body.append(candidate.strip())
            refs = [entry for entry in body if entry.startswith("ref:")]
            self.assertEqual(refs, ["ref: ${{ inputs.checkout-ref || github.sha }}"])
            checkout_refs.extend(refs)
        self.assertEqual(len(checkout_refs), 3)
        self.assertIn("  workflow_call:\n    inputs:\n      checkout-ref:", workflow)
        release = (ROOT / ".github/workflows/release.yml").read_text()
        self.assertIn("uses: ./.github/workflows/ci.yml", release)
        self.assertIn("checkout-ref: ${{ needs.version.outputs.commit }}", release)

    def test_docs_job_skips_only_heavy_preparation(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        gate = workflow.split("  gate:\n", 1)[1].split("  required:\n", 1)[0]

        def step(name):
            return gate.split(f"      - name: {name}\n", 1)[1].split("      - name:", 1)[0]

        self.assertIn("if: runner.os == 'Linux' && matrix.id != 'doc-links'", step("Free disk space"))
        self.assertIn("if: matrix.id != 'doc-links'", step("Rust cache"))
        for name in ("Install Nix", "Set up Cachix"):
            preparation = step(name)
            self.assertIn("if: matrix.shell == 'nix' || matrix.shell == 'web' || matrix.shell == 'web-ci'", preparation)
            self.assertNotIn("matrix.id != 'doc-links'", preparation)
        jobs = json.loads(ci("--matrix").stdout)["include"]
        docs = next(job for job in jobs if job["id"] == "doc-links")
        self.assertEqual(docs["shell"], "nix")
        self.assertEqual(docs["setup"], "nix")
        self.assertEqual(docs["gate"], "doc-links ci-policy-tests")
        self.assertIn('nix) nix develop --command tools/ci/ci.sh gate "$GATE"', step("Gate"))
        summary = step("Record preparation eligibility")
        self.assertIn('"$GITHUB_STEP_SUMMARY"', summary)
        self.assertIn("FREE_DISK_ELIGIBLE: ${{ runner.os == 'Linux' && matrix.id != 'doc-links' }}", summary)
        self.assertIn("RUST_CACHE_ELIGIBLE: ${{ matrix.id != 'doc-links' }}", summary)
        self.assertNotIn("secrets.", summary)
        self.assertNotIn("GH_TOKEN", summary)

    def test_main_baseline_read_and_pages_publish_are_explicit(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        plan = workflow.split("  plan:\n", 1)[1].split("  gate:\n", 1)[0]
        self.assertIn("    permissions:\n      contents: read\n      actions: read", plan)
        self.assertNotIn("id-token: write", plan)
        self.assertIn("GH_TOKEN: ${{ github.token }}", plan)
        self.assertIn("publish_web: ${{ steps.plan.outputs.publish_web }}", plan)
        publish = workflow.split("  cloudflare_pages:\n", 1)[1].split("  cloudflare_pages_release:\n", 1)[0]
        self.assertIn("needs: [plan, required]", publish)
        self.assertIn("needs.plan.outputs.publish_web == 'true'", publish)
        for condition in ("!cancelled()", "needs.required.result == 'success'", "github.event_name == 'push'",
                          "github.repository == 'j4ger/animatix'", "github.ref == 'refs/heads/main'",
                          "vars.ANIMATIX_CF_PAGES == 'enabled'"):
            self.assertIn(condition, publish)
        self.assertIn("Verify this run's original Nix SDK", publish)
        self.assertIn("run: tools/ci/ci.sh gate cloudflare-pages-publish", publish)

    def test_relocated_tool_entrypoints_exist_without_legacy_shims(self):
        self.assertFalse((ROOT / "scripts").exists())
        self.assertFalse((ROOT / "packaging").exists())
        for relative in (
            "tools/ci/ci.sh", "tools/ci/ci-plan.py", "tools/ci/workspace-tests.py",
            "tools/ci/lib/workspace_tests.py", "tools/docs/check-docs.sh",
            "web/tools/build/build-web.sh", "web/tools/dev/serve-web.py",
            "tools/release/native/linux/source-package.py",
            "crates/animatix-gui/tools/packaging/linux/install-desktop.py",
        ):
            with self.subTest(path=relative):
                self.assertTrue((ROOT / relative).is_file())
        ci_source = CI.read_text()
        self.assertIn("trimmed ~ /tools\\//", ci_source)
        self.assertIn("tools\\/ci\\/ci\\.sh", ci_source)

    def test_direct_workflow_gates_use_nix(self):
        rows = [line.split("\t") for line in ci("--list").stdout.splitlines()]
        for path in (ROOT / ".github/workflows").glob("*.yml"):
            runner = None
            for line in path.read_text().splitlines():
                if line.startswith("  ") and not line.startswith("   "):
                    runner = None
                if line.strip().startswith("runs-on:"):
                    runner = line.split("runs-on:", 1)[1].strip()
                if "run: tools/ci/ci.sh gate" in line:
                    gates = line.split("run: tools/ci/ci.sh gate", 1)[1].strip().split()
                    for gate in gates:
                        matching = [row for row in rows if row[0] == gate]
                        self.assertTrue(matching, f"unregistered gate in {path.name}: {gate}")
                        self.assertTrue(any(
                            shell == "none" or (shell == "native" and platform == runner)
                            for _, shell, platform, _, _ in matching),
                            f"bare project gate outside registered environment in {path.name}: {line}")
        nightly = (ROOT / ".github/workflows/nightly.yml").read_text()
        self.assertIn("nix develop .#dist-driver --command tools/ci/ci.sh gate dist-linux", nightly)
        driver = (ROOT / "tools/release/dist-linux.sh").read_text()
        self.assertIn("docker run", driver)
        self.assertNotIn("LD_LIBRARY_PATH", driver)

    def test_reusable_ci_callers_only_pass_declared_inputs(self):
        def body_after(lines, index):
            indent = len(lines[index]) - len(lines[index].lstrip())
            result = []
            for line in lines[index + 1:]:
                if line.strip() and not line.lstrip().startswith("#"):
                    if len(line) - len(line.lstrip()) <= indent:
                        break
                result.append(line)
            return result

        workflow = (ROOT / ".github/workflows/ci.yml").read_text().splitlines()
        call = workflow.index("  workflow_call:")
        declaration = body_after(workflow, call)
        inputs = body_after(declaration, declaration.index("    inputs:"))
        declared_inputs = {
            line.strip().split(":", 1)[0]
            for line in inputs
            if line.startswith("      ") and not line.startswith("       ")
            and not line.lstrip().startswith("#") and ":" in line
        }
        self.assertTrue(declared_inputs, "reusable CI must declare its input interface")
        callers = 0
        for path in (ROOT / ".github/workflows").glob("*.yml"):
            lines = path.read_text().splitlines()
            for index, line in enumerate(lines):
                if not line.startswith("  ") or line.startswith("   ") or line.lstrip().startswith("#"):
                    continue
                job = body_after(lines, index)
                if "    uses: ./.github/workflows/ci.yml" not in job:
                    continue
                callers += 1
                for offset, candidate in enumerate(job):
                    if candidate == "    with:":
                        supplied = {
                            entry.strip().split(":", 1)[0]
                            for entry in body_after(job, offset)
                            if entry.strip() and not entry.lstrip().startswith("#")
                            and len(entry) - len(entry.lstrip()) == 6 and ":" in entry
                        }
                        self.assertLessEqual(supplied, declared_inputs, f"undeclared CI inputs in {path.name}")
        self.assertGreater(callers, 0, "CI caller scan must not be vacuous")

    def test_generated_doc_root_only(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "docs").mkdir()
            document = root / "docs/test.md"
            document.write_text("`web/runtime` `web/runtime/` `web/runtime:12` `docs/test.md`")
            self.assertEqual(list(doc_paths.stale_paths(root)), [])
            document.write_text("`web/runtim/` `web/runtime/missing.js` `web/missing.js` `scripts/missing.py`")
            self.assertEqual([span for _, span in doc_paths.stale_paths(root)],
                             ["web/runtim/", "web/runtime/missing.js", "web/missing.js", "scripts/missing.py"])


if __name__ == "__main__":
    unittest.main()
