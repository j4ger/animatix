#!/usr/bin/env python3
"""Regression tests for CI environment routing and fresh-checkout doc paths."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
CI = ROOT / "scripts/ci.sh"
spec = importlib.util.spec_from_file_location("doc_paths", ROOT / "scripts/check-doc-paths.py")
doc_paths = importlib.util.module_from_spec(spec)
spec.loader.exec_module(doc_paths)


def ci(*args, **env):
    return subprocess.run(["bash", str(CI), *args], cwd=ROOT,
                          env={**os.environ, **env}, text=True, capture_output=True)


class Environments(unittest.TestCase):
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
        validator = ROOT / "scripts/ci-registry-check.py"
        cases = [
            ("fmt|native|ubuntu-latest|rust-stable|pr", "Fast|fmt|native|ubuntu-latest|rust-stable|pr|fast"),
            ("fmt|nix|ubuntu-latest|nix|pr", "Fast|fmt|web-ci|ubuntu-latest|nix|pr|fast"),
            ("fmt|dist-driver|ubuntu-latest|nix|pr", "Fast|fmt|dist-driver|ubuntu-latest|nix|pr|fast"),
        ]
        for gates, jobs in cases:
            result = subprocess.run(["python3", str(validator), gates, jobs], capture_output=True)
            self.assertNotEqual(result.returncode, 0)

    def test_experiments_are_explicit_and_never_in_default_matrix(self):
        normal = json.loads(ci("--matrix").stdout)["include"]
        self.assertNotIn("experiment", {row["stage"] for row in normal})
        experiments = json.loads(ci("--matrix", "experiment").stdout)["include"]
        self.assertEqual({row["id"] for row in experiments},
                         {"linker-linux", "linker-macos", "runner-linux", "property-reference-linux"})
        self.assertNotEqual(ci("all", "experiment").returncode, 0)

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

    def test_direct_workflow_gates_use_nix(self):
        rows = [line.split("\t") for line in ci("--list").stdout.splitlines()]
        for path in (ROOT / ".github/workflows").glob("*.yml"):
            runner = None
            for line in path.read_text().splitlines():
                if line.startswith("  ") and not line.startswith("   "):
                    runner = None
                if line.strip().startswith("runs-on:"):
                    runner = line.split("runs-on:", 1)[1].strip()
                if "run: scripts/ci.sh gate" in line:
                    gate = line.split("run: scripts/ci.sh gate", 1)[1].strip()
                    matching = [row for row in rows if row[0] == gate]
                    self.assertTrue(matching, f"unregistered gate in {path.name}: {gate}")
                    # Bare native commands are valid on their registered host;
                    # pure control gates also need no Nix shell on Linux.
                    self.assertTrue(any(
                        shell == "none" or (shell == "native" and platform == runner)
                        for _, shell, platform, _, _ in matching),
                        f"bare project gate outside registered environment in {path.name}: {line}")
        nightly = (ROOT / ".github/workflows/nightly.yml").read_text()
        self.assertIn("nix develop .#dist-driver --command scripts/ci.sh gate dist-linux", nightly)
        driver = (ROOT / "scripts/dist-linux.sh").read_text()
        self.assertIn("docker run", driver)
        self.assertNotIn("LD_LIBRARY_PATH", driver)

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
