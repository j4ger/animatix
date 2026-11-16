#!/usr/bin/env python3
"""Failure propagation and required-check regression tests."""

import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from urllib.error import HTTPError, URLError
from urllib.parse import parse_qs, urlparse
from unittest.mock import patch

SCRIPT = Path(__file__).with_name("ci-control.py")
SPEC = importlib.util.spec_from_file_location("ci_control", SCRIPT)
CONTROL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CONTROL)


class ControlTests(unittest.TestCase):
    def test_selection_records_effective_jobs_and_force_full_wins(self):
        jobs = [{"id": name} for name in ("doc-links", "check-lint", "test-examples")]
        registry = subprocess.CompletedProcess([], 0, json.dumps({"include": jobs}), "")
        with tempfile.TemporaryDirectory() as directory:
            previous = Path.cwd()
            try:
                os.chdir(directory)
                for selection, force, expected in (
                    ("shadow", False, [job["id"] for job in jobs]),
                    ("enabled", False, ["doc-links"]),
                    ("enabled", True, [job["id"] for job in jobs]),
                ):
                    policy = Path("policy.json")
                    policy.write_text(json.dumps({"candidate_job_ids": ["doc-links"], "force_full": force, "web_smoke": {"depth": "deep"}}))
                    with patch.object(CONTROL.subprocess, "run", return_value=registry), patch.dict(
                        os.environ, {"ANIMATIX_CI_SELECTION": selection, "GITHUB_OUTPUT": str(Path("outputs").resolve())}
                    ):
                        CONTROL.github_plan(policy)
                    self.assertIn("web_smoke_depth=deep\n", Path("outputs").read_text())
                    self.assertEqual(json.loads(policy.read_text())["effective_job_ids"], expected)
                    matrix = json.loads(Path("target/ci/matrix.json").read_text())
                    self.assertEqual([row["id"] for row in matrix["include"]], expected)
            finally:
                os.chdir(previous)

    def test_publication_requires_main_push_same_run_web_and_no_reusable_override(self):
        jobs = [{"id": "doc-links"}, {"id": "web-build"}]
        registry = subprocess.CompletedProcess([], 0, json.dumps({"include": jobs}), "")
        base = {"GITHUB_EVENT_NAME": "push", "GITHUB_REF": "refs/heads/main",
                "GITHUB_REPOSITORY": "j4ger/animatix", "ANIMATIX_CI_SELECTION": "enabled"}
        with tempfile.TemporaryDirectory() as directory:
            previous = Path.cwd()
            try:
                os.chdir(directory)
                for changes, ids, expected in (
                    ({}, ["doc-links", "web-build"], True),
                    ({}, ["doc-links"], False),
                    ({"GITHUB_EVENT_NAME": "pull_request"}, ["web-build"], False),
                    ({"GITHUB_EVENT_NAME": "workflow_call"}, ["web-build"], False),
                    ({"GITHUB_EVENT_NAME": "release"}, ["web-build"], False),
                    ({"GITHUB_REF": "refs/heads/other"}, ["web-build"], False),
                    ({"GITHUB_REPOSITORY": "someone/fork"}, ["web-build"], False),
                    ({"ANIMATIX_CI_CHECKOUT_REF": "v1.0"}, ["web-build"], False),
                ):
                    policy = Path("policy.json")
                    policy.write_text(json.dumps({"event": "push", "candidate_job_ids": ids, "force_full": False}))
                    with patch.object(CONTROL.subprocess, "run", return_value=registry), patch.dict(
                        os.environ, {**base, **changes}, clear=True
                    ):
                        CONTROL.github_plan(policy)
                    result = json.loads(policy.read_text())
                    self.assertEqual(result["publish_web"], expected)
                    self.assertEqual(set(result["effective_job_ids"]) | set(result["skipped_job_ids"]), {"doc-links", "web-build"})
            finally:
                os.chdir(previous)

    def test_required_rejects_missing_skipped_cancelled_or_failed_jobs(self):
        CONTROL.require_success({"plan": {"result": "success"}, "gate": {"result": "success"}})
        for state in ("failure", "cancelled", "skipped", None):
            for job in ("plan", "gate"):
                with self.subTest(state=state, job=job), self.assertRaises(ValueError):
                    needs = {"plan": {"result": "success"}, "gate": {"result": "success"}}
                    needs[job]["result"] = state
                    CONTROL.require_success(needs)
        with self.assertRaises(ValueError):
            CONTROL.require_success({"plan": {"result": "success"}})
        with self.assertRaises(ValueError):
            CONTROL.require_success({"plan": {"result": "success"},
                                     "gate": {"result": "success"},
                                     "unexpected": {"result": "skipped"}})

    def test_environment_snapshots_are_shell_scoped_and_skip_rust_for_none(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            calls = []

            def fake_probe(command, **_kwargs):
                calls.append(command)
                return subprocess.CompletedProcess(command, 0, "fixture-output", "")

            base_env = {"GITHUB_RUN_ID": "fixture-run", "GITHUB_JOB": "control",
                        "RUNNER_OS": "Linux"}
            with patch.object(CONTROL.subprocess, "run", side_effect=fake_probe), \
                 patch.dict(os.environ, base_env, clear=True):
                none_unmarked_name = CONTROL.environment_snapshot(root, "none")
                none_unmarked = json.loads((root / none_unmarked_name).read_text())
                self.assertEqual(none_unmarked["declared_shell"], "none")
                self.assertEqual(none_unmarked["actual_shell"], "unmarked")
                self.assertEqual(none_unmarked["tools"]["rustc"],
                                 {"skipped": "gate declares shell=none"})
                self.assertEqual(none_unmarked["tools"]["cargo"],
                                 {"skipped": "gate declares shell=none"})
                self.assertFalse(any(Path(command[0]).name in {"rustc", "cargo", "rustup"}
                                     for command in calls))

                with patch.dict(os.environ, {"ANIMATIX_CI_SHELL": "nix"}):
                    none_nix_name = CONTROL.environment_snapshot(root, "none")
                    none_nix = json.loads((root / none_nix_name).read_text())
                    self.assertEqual(none_nix["actual_shell"], "nix")
                    self.assertNotEqual(none_nix_name, none_unmarked_name)
                    self.assertFalse(any(Path(command[0]).name in {"rustc", "cargo", "rustup"}
                                         for command in calls))

                    nix_name = CONTROL.environment_snapshot(root, "nix")
                    nix = json.loads((root / nix_name).read_text())
                    self.assertEqual(nix["declared_shell"], "nix")
                    self.assertEqual(nix["actual_shell"], "nix")
                    self.assertNotIn(nix_name, {none_unmarked_name, none_nix_name})
                    self.assertEqual(nix["tools"]["rustc"]["exit_code"], 0)
                    self.assertEqual(nix["tools"]["cargo"]["exit_code"], 0)
                    called = {Path(command[0]).name for command in calls}
                    self.assertTrue({"rustc", "cargo", "nix"}.issubset(called))

    def test_time_cli_passes_declared_shell_to_collector(self):
        argv = [str(SCRIPT), "time", "--gate", "probe", "--shell", "none", "--", "true"]
        with patch.object(sys, "argv", argv), \
             patch.object(CONTROL, "timed_gate", return_value=0) as timed:
            self.assertEqual(CONTROL.main(), 0)
        timed.assert_called_once_with("probe", ["--", "true"], "none")

    def test_gate_failure_remains_failure_and_is_recorded(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "timings.jsonl"
            sentinel = Path(directory) / "incorrect-success"
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "time", "--gate", "probe", "--", "bash", "-euc",
                 'false; touch "$1"', "probe", str(sentinel)],
                env={**os.environ, "ANIMATIX_GATE_TIMINGS": str(output)}, capture_output=True)
            self.assertEqual(result.returncode, 1)
            self.assertFalse(sentinel.exists())
            record = json.loads(output.read_text())
            self.assertEqual(record["exit_code"], 1)
            self.assertGreaterEqual(record["elapsed_seconds"], 0)


class ArtifactAbsenceTests(unittest.TestCase):
    class Response:
        def __init__(self, payload):
            self.payload = json.dumps(payload).encode()

        def __enter__(self):
            return self

        def __exit__(self, *_args):
            return False

        def read(self):
            return self.payload

    def lookup(self, payloads):
        requests = []

        def opener(request, timeout):
            requests.append((request, timeout))
            page = int(parse_qs(urlparse(request.full_url).query)["page"][0])
            payload = payloads[page - 1]
            if isinstance(payload, BaseException):
                raise payload
            return self.Response(payload)

        result = CONTROL.confirm_artifact_unavailable(
            "owner/repo", "12345", "web-runtime", "secret-token",
            "https://api.github.test", opener,
        )
        return result, requests

    def test_confirmed_missing_artifact_allows_fallback(self):
        result, requests = self.lookup([{"total_count": 0, "artifacts": []}])
        self.assertEqual(result, "missing")
        self.assertEqual(len(requests), 1)
        self.assertEqual(requests[0][0].get_header("Authorization"), "Bearer secret-token")

    def test_confirmed_expired_artifact_allows_fallback(self):
        result, _requests = self.lookup([{
            "total_count": 1,
            "artifacts": [{"id": 1, "name": "web-runtime", "expired": True,
                           "workflow_run": {"id": 12345}}],
        }])
        self.assertEqual(result, "expired")

    def test_present_nonexpired_artifact_blocks_fallback(self):
        with self.assertRaisesRegex(ValueError, "exists and is not expired"):
            self.lookup([{
                "total_count": 1,
                "artifacts": [{"id": 1, "name": "web-runtime", "expired": False,
                               "workflow_run": {"id": 12345}}],
            }])

    def test_api_auth_and_network_errors_block_fallback_without_leaking_token(self):
        for failure in (
            HTTPError("https://api.github.test", 401, "Unauthorized", {}, None),
            URLError("network unavailable"),
        ):
            with self.subTest(failure=type(failure).__name__), self.assertRaises(ValueError) as raised:
                self.lookup([failure])
            self.assertNotIn("secret-token", str(raised.exception))

    def test_lookup_paginates_before_confirming_absence(self):
        first_page = {
            "total_count": 101,
            "artifacts": [{"id": index, "name": f"other-{index}", "expired": False}
                          for index in range(1, 101)],
        }
        second_page = {
            "total_count": 101,
            "artifacts": [{"id": 101, "name": "web-runtime", "expired": False,
                           "workflow_run": {"id": 12345}}],
        }
        with self.assertRaisesRegex(ValueError, "exists and is not expired"):
            self.lookup([first_page, second_page])

    def test_missing_artifact_name_in_any_inventory_row_blocks_fallback(self):
        with self.assertRaisesRegex(ValueError, "malformed artifact identity/status"):
            self.lookup([{
                "total_count": 1,
                "artifacts": [{"id": 1, "expired": False}],
            }])

    def test_empty_artifact_name_in_any_inventory_row_blocks_fallback(self):
        with self.assertRaisesRegex(ValueError, "malformed artifact identity/status"):
            self.lookup([{
                "total_count": 1,
                "artifacts": [{"id": 1, "name": "", "expired": False}],
            }])

    def test_invalid_artifact_id_or_missing_expiration_status_blocks_fallback(self):
        for item in (
            {"id": 0, "name": "other", "expired": False},
            {"id": 2, "name": "other"},
        ):
            with self.subTest(item=item), self.assertRaisesRegex(
                ValueError, "malformed artifact identity/status",
            ):
                self.lookup([{"total_count": 1, "artifacts": [item]}])


if __name__ == "__main__":
    unittest.main()
