#!/usr/bin/env python3
"""Probe is inert; higher stages fail closed without Nix or a clean checkout."""
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("cloudflare-build-experiment.sh")


class ExperimentTest(unittest.TestCase):
    def run_stage(self, stage, *, fake_nix=False, dirty=False, git_status_failure=False):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            (root / "web/tools/deploy").mkdir(parents=True)
            (root / "bin").mkdir()
            shutil.copyfile(SCRIPT, root / "web/tools/deploy/probe.sh")
            (root / "flake.lock").write_text("{}")
            bash = shutil.which("bash")
            self.assertIsNotNone(bash)
            shebang = f"#!{bash}\n"
            for tool in ["bash", "dirname", "id", "uname", "getconf", "awk", "cat", "df", "timeout"]:
                (root / "bin" / tool).symlink_to(shutil.which(tool))
            if fake_nix:
                script = root / "bin/nix"
                script.write_text(shebang + "echo UNEXPECTED_NIX_EXECUTION; exit 99\n")
                script.chmod(0o755)
                git = root / "bin/git"
                if git_status_failure:
                    git_script = shebang + "exit 128\n"
                elif dirty:
                    git_script = shebang + "echo ' M dirty'\n"
                else:
                    git_script = shebang + "exit 0\n"
                git.write_text(git_script)
                git.chmod(0o755)
            env = {"PATH": str(root / "bin"), "UNRELATED_SECRET": "must-not-leak-123"}
            result = subprocess.run([bash, str(root / "web/tools/deploy/probe.sh"), *stage],
                                    env=env, text=True, capture_output=True)
            self.assertNotIn("must-not-leak-123", result.stdout + result.stderr)
            self.assertNotIn("UNEXPECTED_NIX_EXECUTION", result.stdout + result.stderr)
            return result

    def test_probe_without_nix_succeeds(self):
        result = self.run_stage(["probe"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("tool=nix available=no", result.stdout)

    def test_default_is_probe(self):
        self.assertEqual(self.run_stage([]).returncode, 0)

    def test_probe_never_invokes_installed_nix(self):
        self.assertEqual(self.run_stage(["probe"], fake_nix=True).returncode, 0)

    def test_other_stages_require_nix(self):
        for stage in ["nix", "sdk", "site"]:
            result = self.run_stage([stage])
            self.assertEqual(result.returncode, 3)
            self.assertIn("Nix is not installed", result.stderr)

    def test_dirty_checkout_blocks_nix(self):
        self.assertEqual(self.run_stage(["site"], fake_nix=True, dirty=True).returncode, 3)

    def test_git_status_failure_blocks_nix(self):
        result = self.run_stage(["site"], fake_nix=True, git_status_failure=True)
        self.assertEqual(result.returncode, 3, result.stderr)
        self.assertIn("BLOCKED: could not verify whether the checkout is clean", result.stderr)
        self.assertNotIn("UNEXPECTED_NIX_EXECUTION", result.stdout + result.stderr)

    def test_invalid_stages(self):
        for args in [["deploy"], ["probe", "extra"]]:
            self.assertEqual(self.run_stage(args).returncode, 2)


if __name__ == "__main__":
    unittest.main()
