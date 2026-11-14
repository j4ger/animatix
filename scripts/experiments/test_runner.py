"""Fast unit tests for the test-runner experiment's coverage accounting."""

from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import tomllib
import unittest


SCRIPT = Path(__file__).with_name("runner.py")
SPEC = importlib.util.spec_from_file_location("experiment_runner", SCRIPT)
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


class RunnerCoverageTests(unittest.TestCase):
    def test_nested_sensitive_modules_execute_each_selected_case_once(self):
        names = ("offscreen::tests::draw", "offscreen::tests::readback_reuse_tests::reuse",
                 "filter_backend::tests::mask")
        cases = [{"name": name, "id": f"animatix-render::unit::{name}"} for name in names]
        groups = RUNNER._case_groups([{"package_name": "animatix-render", "target_name": "unit",
                                       "cases": cases}])
        invocations = [case["id"] for group in groups.values() for case in group["cases"]]
        self.assertCountEqual(invocations, [case["id"] for case in cases])
        self.assertEqual(len(invocations), len(set(invocations)))

    def test_runner_direct_entrypoint_bootstraps_repo_imports_without_pythonpath(self):
        env = os.environ.copy()
        env.pop("PYTHONPATH", None)
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "--help"],
            cwd=SCRIPT.parents[2], env=env, capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("--execute-plan", result.stdout)

    def test_harness_timeout_kills_spawned_child_process_group(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            marker = root / "child-ticks.txt"
            executable = root / "fake-harness"
            child_code = (
                "import pathlib,time; p=pathlib.Path(" + repr(str(marker)) + "); "
                "\nwhile True:\n p.open('a', encoding='utf-8').write('tick\\n'); time.sleep(0.02)"
            )
            executable.write_text(
                f"#!{sys.executable}\nimport subprocess,sys,time\n"
                f"subprocess.Popen([sys.executable, '-c', {child_code!r}])\n"
                "while True: time.sleep(1)\n",
                encoding="utf-8",
            )
            executable.chmod(0o755)
            item = {"executable": str(executable), "target_name": "fake-harness",
                    "package_name": "fixture", "manifest_dir": str(root), "cases": []}
            result = RUNNER._execute_harness_item(item, os.environ.copy(), root, "timeout", timeout_seconds=1)
            self.assertTrue(result["timed_out"])
            self.assertEqual(result["exit_code"], 124)
            self.assertTrue(marker.is_file())
            first_count = len(marker.read_text(encoding="utf-8").splitlines())
            time.sleep(0.1)
            second_count = len(marker.read_text(encoding="utf-8").splitlines())
            self.assertEqual(first_count, second_count, "descendant survived harness timeout")

    def test_outer_dispatch_signal_kills_private_harness_group(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            marker = root / "child-ticks.txt"
            executable = root / "fake-harness"
            child_code = (
                "import pathlib,time; p=pathlib.Path(" + repr(str(marker)) + "); "
                "\nwhile True:\n p.open('a', encoding='utf-8').write('tick\\n'); time.sleep(0.02)"
            )
            executable.write_text(
                f"#!{sys.executable}\nimport subprocess,sys,time\n"
                f"subprocess.Popen([sys.executable, '-c', {child_code!r}])\n"
                "while True: time.sleep(1)\n",
                encoding="utf-8",
            )
            executable.chmod(0o755)
            log_dir = root / "logs"
            plan = {
                "env": {}, "items": [{"package_name": "fixture", "target_name": "fake-harness",
                    "manifest_dir": str(root), "executable": str(executable), "cases": []}],
                "max_workers": 1, "serial_packages": [], "run_tag": "outer", "log_dir": str(log_dir),
            }
            plan_path = root / "plan.json"
            plan_path.write_text(json.dumps(plan), encoding="utf-8")
            result_path = root / "result.json"
            env = os.environ.copy()
            env.pop("PYTHONPATH", None)
            process = subprocess.Popen(
                [sys.executable, str(SCRIPT), "--execute-plan", str(plan_path), "--result", str(result_path)],
                cwd=SCRIPT.parents[2], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                start_new_session=True,
            )
            try:
                deadline = time.monotonic() + 5
                while not marker.exists() and process.poll() is None and time.monotonic() < deadline:
                    time.sleep(0.02)
                self.assertTrue(marker.exists(), "test descendant never started")
                os.killpg(process.pid, signal.SIGTERM)
                process.wait(timeout=5)
                first_count = len(marker.read_text(encoding="utf-8").splitlines())
                time.sleep(0.1)
                second_count = len(marker.read_text(encoding="utf-8").splitlines())
                self.assertEqual(first_count, second_count, "outer timeout left a test descendant alive")
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait(timeout=2)

    def test_cargo_list_parser_keeps_full_test_names_and_ignores_benchmarks(self):
        self.assertEqual(
            RUNNER._parse_test_list(
                "timeline::assets::tests::cache_reused: test\n"
                "some_bench: benchmark\n"
                "tests::plain_name: test\n"
            ),
            ["timeline::assets::tests::cache_reused", "tests::plain_name"],
        )

    def test_motion_merge_alias_maps_four_history_ids_to_one_current_id(self):
        old_names = (
            "move_requires_to_vec2_modifier", "rotate_requires_numeric_by_modifier",
            "scale_requires_positive_numeric_by_modifier", "shift_requires_by_vec2_modifier",
        )
        old = [{"id": f"animatix::animatix::timeline::actions::motion::tests::{name}"}
               for name in old_names]
        new = [{"id": "animatix::animatix::timeline::actions::motion::tests::motion_actions_reject_invalid_modifier_values"}]
        self.assertEqual(RUNNER._historical_canonical_ids(old), RUNNER._historical_canonical_ids(new))
        self.assertNotEqual(RUNNER._canonical_ids(old), RUNNER._canonical_ids(new))

    def test_nextest_json_object_schema_uses_binary_and_case_keys_as_ids(self):
        listing = {
            "rust-suites": {
                "animatix": {
                    "package-name": "animatix", "binary-name": "animatix",
                    "testcases": {
                        "timeline::assets::tests::svg_cache_reused": {
                            "kind": "test", "ignored": False,
                            "filter-match": {"status": "matches"},
                        },
                        "timeline::assets::tests::old_ignored": {
                            "kind": "test", "ignored": True,
                            "filter-match": {"status": "matches"},
                        },
                    },
                },
            },
        }
        cases, suites = RUNNER._parse_nextest_listing(listing)
        self.assertEqual(suites, 1)
        self.assertEqual({case["id"] for case in cases}, {
            "animatix::animatix::timeline::assets::tests::svg_cache_reused",
            "animatix::animatix::timeline::assets::tests::old_ignored",
        })
        self.assertTrue(next(case for case in cases if case["name"] == "timeline::assets::tests::old_ignored")["ignored"])

    def test_nextest_junit_report_maps_actual_cases_to_binary_ids(self):
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "junit.xml"
            report.write_text(
                '<testsuites><testsuite name="animatix::animatix_contracts">'
                '<testcase name="module_tests::imports::load_ok" />'
                '<testcase name="module_tests::imports::load_bad"><failure /></testcase>'
                '<testcase name="ignored_case"><skipped /></testcase>'
                '</testsuite></testsuites>',
                encoding="utf-8",
            )
            cases = [
                {"id": "animatix::animatix_contracts::module_tests::imports::load_ok",
                 "binary_id": "animatix::animatix_contracts", "name": "module_tests::imports::load_ok"},
                {"id": "animatix::animatix_contracts::module_tests::imports::load_bad",
                 "binary_id": "animatix::animatix_contracts", "name": "module_tests::imports::load_bad"},
                {"id": "animatix::animatix_contracts::ignored_case",
                 "binary_id": "animatix::animatix_contracts", "name": "ignored_case"},
            ]
            ids, suites = RUNNER._parse_nextest_junit_ids(report, cases)
            self.assertEqual(set(ids), {cases[0]["id"], cases[1]["id"]})
            self.assertEqual(suites, [{"suite": "animatix::animatix_contracts", "tests": 3}])

    def test_nextest_run_config_preserves_repository_renderer_policy(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config_dir = root / ".config"
            config_dir.mkdir()
            (config_dir / "nextest.toml").write_text(
                '[test-groups]\nrenderer-gpu = { max-threads = 1 }\n\n'
                '[[profile.default.overrides]]\nfilter = "package(animatix-render)"\n'
                'test-group = "renderer-gpu"\n',
                encoding="utf-8",
            )
            output = RUNNER.ExperimentOutput(root, root / "experiment", "runner", {})
            generated, report = RUNNER._write_nextest_config(root, output, "nextest2", 1)
            merged = tomllib.loads(generated.read_text(encoding="utf-8"))
            self.assertEqual(merged["test-groups"]["renderer-gpu"]["max-threads"], 1)
            self.assertEqual(merged["profile"]["default"]["overrides"][0]["test-group"], "renderer-gpu")
            self.assertEqual(merged["profile"]["default"]["junit"]["path"], "junit.xml")
            self.assertEqual(report, output.logs_dir / "nextest-store" / "nextest2" / "round-1" / "default" / "junit.xml")

    def test_coverage_diff_reports_both_directions(self):
        diff = RUNNER._coverage_diff({"kept", "missing"}, {"kept", "extra"})
        self.assertFalse(diff["equal"])
        self.assertEqual(diff["missing"], ["missing"])
        self.assertEqual(diff["unexpected"], ["extra"])

    def test_cargo_artifact_parser_selects_only_workspace_test_executables(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            exe = root / "target" / "debug" / "deps" / "test-bin"
            exe.parent.mkdir(parents=True)
            exe.write_text("fixture", encoding="utf-8")
            records = [
                {"reason": "compiler-artifact", "package_id": "animatix 1.0",
                 "target": {"name": "lib-tests", "test": True},
                 "profile": {"test": True}, "executable": str(exe)},
                {"reason": "compiler-artifact", "package_id": "external 1.0",
                 "target": {"name": "dependency-tests", "test": True},
                 "profile": {"test": True}, "executable": str(exe)},
                {"reason": "compiler-artifact", "package_id": "animatix 1.0",
                 "target": {"name": "normal-bin", "test": False},
                 "profile": {"test": False}, "executable": str(exe)},
            ]
            artifacts = root / "artifacts.jsonl"
            artifacts.write_text("\n".join(json.dumps(record) for record in records), encoding="utf-8")
            result = RUNNER._parse_artifacts(artifacts, {"animatix 1.0"}, root / "target")
            self.assertEqual(len(result), 1)
            self.assertEqual(result[0]["target_name"], "lib-tests")

    def test_doc_result_parser_only_reads_rustdoc_sections(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "docs.log"
            log.write_text(
                "test unit::test_case ... ok\n"
                "   Doc-tests animatix\n"
                "running 2 tests\n"
                "test crates/animatix/src/lib.rs - animatix::Timeline (line 8) ... ok\n"
                "test crates/animatix/src/lib.rs - animatix::Builder (line 20) ... ok\n",
                encoding="utf-8",
            )
            self.assertEqual(RUNNER._parse_doc_ids([{"log_path": str(log)}]), [
                "crates/animatix/src/lib.rs - animatix::Builder (line 20)",
                "crates/animatix/src/lib.rs - animatix::Timeline (line 8)",
            ])

    def test_cargo_execution_parser_maps_running_binary_to_passed_test_ids(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            executable = root / "target" / "debug" / "deps" / "animatix-a1b2"
            executable.parent.mkdir(parents=True)
            executable.write_text("harness", encoding="utf-8")
            log = root / "cargo-test.log"
            log.write_text(
                f"     Running unittests src/lib.rs ({executable})\n"
                "running 2 tests\n"
                "test module::passes ... ok\n"
                "test module::ignored ... ignored\n"
                "test result: ok. 1 passed; 0 failed; 1 ignored\n",
                encoding="utf-8",
            )
            harnesses = [{
                "executable": str(executable),
                "cases": [{"id": "animatix::animatix::module::passes", "name": "module::passes"}],
            }]
            actual, unresolved = RUNNER._parse_cargo_executed_ids(log, harnesses)
            self.assertEqual(actual, ["animatix::animatix::module::passes"])
            self.assertEqual(unresolved, [])


if __name__ == "__main__":
    unittest.main()
