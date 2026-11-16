"""Fast checks for shared Cargo inventory and process cleanup helpers."""

from __future__ import annotations

import json
import os
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

from tools.ci.lib.workspace_tests import (
    WorkspaceTestError,
    package_maps,
    parse_cargo_test_artifacts,
    run_logged_command,
)

SCHEDULER_SCRIPT = Path(__file__).resolve().parents[1] / "workspace-tests.py"
SCHEDULER_SPEC = importlib.util.spec_from_file_location("workspace_tests_cli", SCHEDULER_SCRIPT)
SCHEDULER = importlib.util.module_from_spec(SCHEDULER_SPEC)
SCHEDULER_SPEC.loader.exec_module(SCHEDULER)


class WorkspaceTestHelpers(unittest.TestCase):
    def test_package_maps_reject_incomplete_metadata(self):
        with self.assertRaises(WorkspaceTestError):
            package_maps({"workspace_members": ["missing"], "packages": []})

    def test_artifact_parser_rejects_executables_outside_target_dir(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / "target"
            target.mkdir()
            outside = root / "test-binary"
            outside.write_text("fixture", encoding="utf-8")
            artifacts = root / "artifacts.jsonl"
            artifacts.write_text(json.dumps({
                "reason": "compiler-artifact", "package_id": "member",
                "target": {"test": True, "name": "unit", "kind": ["test"]},
                "profile": {"test": True}, "executable": str(outside),
            }) + "\n", encoding="utf-8")
            with self.assertRaisesRegex(WorkspaceTestError, "outside the selected target"):
                parse_cargo_test_artifacts(artifacts, {"member"}, target)

    def test_artifact_parser_rejects_truncated_json_after_valid_artifact(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / "target"
            executable = target / "debug" / "deps" / "test-binary"
            executable.parent.mkdir(parents=True)
            executable.write_text("fixture", encoding="utf-8")
            artifacts = root / "artifacts.jsonl"
            valid = {
                "reason": "compiler-artifact", "package_id": "member",
                "target": {"test": True, "name": "unit", "kind": ["test"]},
                "profile": {"test": True}, "executable": str(executable),
            }
            artifacts.write_text(json.dumps(valid) + '\n{"reason":"compiler-artifact"\n',
                                 encoding="utf-8")
            with self.assertRaisesRegex(WorkspaceTestError, r"invalid Cargo JSON at .*artifacts\.jsonl:2"):
                parse_cargo_test_artifacts(artifacts, {"member"}, target)

    def test_logged_command_keeps_stdout_and_stderr_separate(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            record = run_logged_command(
                [sys.executable, "-c", "import sys; print('machine'); print('diagnostic', file=sys.stderr)"],
                root, os.environ.copy(), root / "stdout", root / "stderr",
            )
            self.assertEqual(record["exit_code"], 0)
            self.assertEqual((root / "stdout").read_text().strip(), "machine")
            self.assertEqual((root / "stderr").read_text().strip(), "diagnostic")

    def test_logged_command_timeout_kills_descendant_process_group(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            marker = root / "child-ticks"
            child = "import pathlib,time; p=pathlib.Path(%r);\nwhile True:\n p.open('a').write('x\\n'); time.sleep(.02)" % str(marker)
            script = "import subprocess,sys,time; subprocess.Popen([sys.executable,'-c',%r]);\nwhile True: time.sleep(1)" % child
            record = run_logged_command(
                [sys.executable, "-c", script], root, os.environ.copy(),
                root / "stdout", root / "stderr", timeout_seconds=0.2,
            )
            self.assertTrue(record["timed_out"])
            self.assertEqual(record["exit_code"], 124)
            first_count = len(marker.read_text(encoding="utf-8").splitlines())
            time.sleep(0.1)
            self.assertEqual(first_count, len(marker.read_text(encoding="utf-8").splitlines()))

    def test_production_entrypoint_uses_fake_cargo_and_checks_case_coverage(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / "repo"
            manifest_dir = repo / "crates" / "sample"
            manifest_dir.mkdir(parents=True)
            (manifest_dir / "Cargo.toml").write_text("[package]\nname='sample'\n", encoding="utf-8")
            target = root / "target"
            executable = target / "debug" / "deps" / "sample-unit"
            executable.parent.mkdir(parents=True)
            private_secret = "synthetic-private-test-sentinel"
            executable.write_text(
                f"#!{sys.executable}\nimport os,sys\n"
                f"if os.environ.get('PRIVATE_TEST_SECRET') != {private_secret!r}:\n"
                " raise SystemExit('synthetic private environment missing')\n"
                "if os.environ.get('CARGO_INCREMENTAL') != '1':\n"
                " raise SystemExit('unexpected CARGO_INCREMENTAL for test harness')\n"
                "if '--ignored' in sys.argv:\n"
                " pass\n"
                "elif '--list' in sys.argv:\n"
                " print('sample::works: test')\n"
                "else:\n"
                " print('synthetic private environment available')\n"
                " print('running 1 test')\n"
                " print('test sample::works ... ok')\n"
                " print('test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out')\n",
                encoding="utf-8",
            )
            executable.chmod(0o755)

            cargo_bin = root / "bin"
            cargo_bin.mkdir()
            cargo = cargo_bin / "cargo"
            cargo.write_text(
                f"#!{sys.executable}\nimport json,os,sys\n"
                "args=sys.argv[1:]\n"
                "if args[0]=='metadata':\n"
                " print(json.dumps({'workspace_members':['sample-id'],'packages':["
                "{'id':'sample-id','name':'sample','version':'1.0.0','manifest_path':"
                + repr(str(manifest_dir / "Cargo.toml"))
                + "}],'target_directory':os.environ['CARGO_TARGET_DIR']}))\n"
                "elif '--no-run' in args:\n"
                " print(json.dumps({'reason':'compiler-artifact','package_id':'sample-id',"
                "'target':{'test':True,'name':'unit','kind':['test']},"
                "'profile':{'test':True},'executable':"
                + repr(str(executable))
                + "}))\n"
                "elif '--doc' in args:\n"
                " print('doc tests passed')\n"
                "else:\n raise SystemExit(2)\n",
                encoding="utf-8",
            )
            cargo.chmod(0o755)
            env = {**os.environ, "PATH": str(cargo_bin) + os.pathsep + os.environ.get("PATH", ""),
                   "CARGO_TARGET_DIR": str(target), "CARGO_INCREMENTAL": "1",
                   "PRIVATE_TEST_SECRET": private_secret}
            with mock.patch.dict(os.environ, env, clear=True):
                status = SCHEDULER._run_workspace(repo)
                self.assertEqual(status, 0)
                summary_path = next((target / "ci" / "workspace-tests").glob("*/summary.json"))
                summary = json.loads(summary_path.read_text(encoding="utf-8"))
                harness = next(stage for stage in summary["stages"] if stage["name"] == "test-harnesses")
                self.assertTrue(harness["coverage"]["equal"])
                self.assertEqual(harness["coverage"]["expected_count"], 1)
                self.assertEqual(summary["environment"]["CARGO_INCREMENTAL"], "1")
                harness_results = json.loads(Path(harness["results_path"]).read_text(encoding="utf-8"))
                harness_log = Path(harness_results["harnesses"][0]["log_path"])
                self.assertIn("synthetic private environment available", harness_log.read_text(encoding="utf-8"))

                executable.write_text(
                    f"#!{sys.executable}\nimport sys\n"
                    "if '--ignored' not in sys.argv and '--list' in sys.argv:\n"
                    " print('sample::works: test')\n"
                    "elif '--list' not in sys.argv:\n"
                    " print('running 1 test')\n"
                    " print('test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out')\n",
                    encoding="utf-8",
                )
                self.assertEqual(SCHEDULER._run_workspace(repo), 1)
                failed = next(
                    summary for path in (target / "ci" / "workspace-tests").glob("*/summary.json")
                    if (summary := json.loads(path.read_text(encoding="utf-8")))
                    and any(stage.get("name") == "test-harnesses" and not stage.get("coverage", {}).get("equal")
                            for stage in summary["stages"])
                )
                failed_harness = next(stage for stage in failed["stages"] if stage["name"] == "test-harnesses")
                self.assertFalse(failed_harness["coverage"]["equal"])

                for path in (target / "ci").rglob("*"):
                    if path.is_file():
                        persisted = path.read_bytes()
                        self.assertNotIn(private_secret.encode("utf-8"), persisted, str(path))
                        if path.suffix in {".json", ".jsonl"}:
                            self.assertNotIn(b"PRIVATE_TEST_SECRET", persisted, str(path))


if __name__ == "__main__":
    unittest.main()
