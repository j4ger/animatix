#!/usr/bin/env python3
"""Unit tests for committed source snapshot comparisons."""

import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("compare-snapshot-gates.py")
SPEC = importlib.util.spec_from_file_location("compare_snapshot_gates", SCRIPT)
COMPARE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(COMPARE)


def git(repo, *args):
    return subprocess.check_output(["git", *args], cwd=repo, text=True).strip()


class SnapshotGateTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="animatix-snapshot-test-")
        self.repo = Path(self.temporary.name) / "repo"
        self.repo.mkdir()
        git(self.repo, "init", "--quiet")
        git(self.repo, "config", "user.name", "Snapshot Test")
        git(self.repo, "config", "user.email", "snapshot-test@example.invalid")
        ci = self.repo / "tools" / "ci" / "ci.sh"
        ci.parent.mkdir(parents=True)
        ci.write_text(
            "#!/usr/bin/env bash\n"
            "set -eu\n"
            "if [ \"${1:-}\" = --list ]; then\n"
            "  printf 'check\\tnix\\tubuntu-latest\\tnix\\tpr\\n'\n"
            "elif [ \"${1:-}\" = gate ]; then\n"
            "  test \"$CARGO_TARGET_DIR\" = \"$(pwd -P)/target\"\n"
            "  printf 'gate %s\\n' \"${2:-}\"\n"
            "else exit 2; fi\n",
            encoding="utf-8",
        )
        (self.repo / "source.txt").write_text("baseline\n", encoding="utf-8")
        git(self.repo, "add", ".")
        git(self.repo, "commit", "--quiet", "-m", "baseline")
        self.baseline = git(self.repo, "rev-parse", "HEAD")
        (self.repo / "source.txt").write_text("candidate\n", encoding="utf-8")
        git(self.repo, "add", ".")
        git(self.repo, "commit", "--quiet", "-m", "candidate")
        self.candidate = git(self.repo, "rev-parse", "HEAD")

    def tearDown(self):
        self.temporary.cleanup()

    def test_resolves_committed_refs_and_archives_without_git_metadata(self):
        self.assertEqual(COMPARE.resolve_commit(self.repo, self.baseline), self.baseline)
        destination = Path(self.temporary.name) / "snapshot"
        COMPARE.extract_snapshot(self.repo, self.baseline, destination)
        self.assertEqual((destination / "source.txt").read_text(encoding="utf-8"), "baseline\n")
        self.assertFalse((destination / ".git").exists())

    def test_reads_only_native_pr_gates_from_the_canonical_registry(self):
        gates = COMPARE.native_pr_gates(self.repo)
        self.assertEqual(set(gates), {"check"})
        self.assertEqual(gates["check"]["shell"], "nix")
        self.assertEqual(gates["check"]["stage"], "pr")

    def test_prepare_mode_compares_exact_trees_without_mutating_checkout(self):
        before = git(self.repo, "status", "--porcelain")
        output = Path(self.temporary.name) / "evidence"
        code, result = COMPARE.compare(
            self.repo, self.baseline, self.candidate, ("check",), output, prepare_only=True,
        )
        report = json.loads((result / "report.json").read_text(encoding="utf-8"))
        self.assertEqual(code, 0)
        self.assertEqual(report["status"], "prepared")
        self.assertEqual(report["baseline"]["commit"], self.baseline)
        self.assertEqual(report["candidate"]["commit"], self.candidate)
        self.assertNotEqual(report["baseline"]["tree"], report["candidate"]["tree"])
        self.assertEqual(report["sides"], {"baseline": [], "candidate": []})
        self.assertEqual(git(self.repo, "status", "--porcelain"), before)

    def test_run_mode_invokes_the_same_gate_in_each_archive(self):
        output = Path(self.temporary.name) / "run-evidence"
        code, result = COMPARE.compare(
            self.repo, self.baseline, self.candidate, ("check",), output,
        )
        report = json.loads((result / "report.json").read_text(encoding="utf-8"))
        self.assertEqual(code, 0)
        self.assertEqual(report["status"], "passed")
        for side in ("baseline", "candidate"):
            record = report["sides"][side][0]
            self.assertEqual(record["gate"], "check")
            self.assertEqual(record["exit_code"], 0)
            self.assertIn("gate check", (result / record["stdout"]).read_text(encoding="utf-8"))

    def test_rejects_identical_refs_and_non_plan_gates(self):
        with self.assertRaises(COMPARE.SnapshotError):
            COMPARE.compare(self.repo, self.baseline, self.baseline, ("check",),
                            Path(self.temporary.name) / "same")
        with self.assertRaises(COMPARE.SnapshotError):
            COMPARE.compare(self.repo, self.baseline, self.candidate, ("web-build",),
                            Path(self.temporary.name) / "invalid")


if __name__ == "__main__":
    unittest.main()
