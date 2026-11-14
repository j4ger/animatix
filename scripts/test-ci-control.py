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
from unittest.mock import patch

from ci_topology import artifact, transform

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
                    policy.write_text(json.dumps({"candidate_job_ids": ["doc-links"], "force_full": force}))
                    with patch.object(CONTROL.subprocess, "run", return_value=registry), patch.dict(
                        os.environ, {"ANIMATIX_CI_SELECTION": selection, "ANIMATIX_CI_TOPOLOGY": "baseline"}
                    ):
                        CONTROL.github_plan(policy)
                    self.assertEqual(json.loads(policy.read_text())["effective_job_ids"], expected)
                    matrix = json.loads(Path("target/ci/matrix.json").read_text())
                    self.assertEqual([row["id"] for row in matrix["include"]], expected)
            finally:
                os.chdir(previous)

    def test_topologies_preserve_registered_work_and_reject_mixed_shells(self):
        jobs = [{"id": identifier, "gate": gates, "shell": "nix", "os": "ubuntu-latest"}
                for identifier, gates in (("check-lint", "check lint"),
                                           ("test-examples", "test examples"),
                                           ("render-scenes", "render-smoke"),
                                           ("web", "web-build"))]
        grouped, special = transform(jobs, "grouped")
        self.assertEqual(special, "")
        self.assertEqual({gate for row in jobs for gate in row["gate"].split()},
                         {gate for row in grouped for gate in row["gate"].split()})
        shared, special = transform(jobs, "shared-cli")
        self.assertEqual(special, "render-smoke")
        self.assertNotIn("render-scenes", {row["id"] for row in shared})
        self.assertNotIn("test-examples", {row["id"] for row in shared})
        self.assertEqual({gate for row in jobs for gate in row["gate"].split()},
                         {gate for row in shared for gate in row["gate"].split()}
                         | set(special.split()) | set(jobs[1]["gate"].split()))
        jobs[0]["shell"] = "web"
        with self.assertRaises(ValueError):
            transform(jobs, "grouped")

    def test_selected_artifact_jobs_cannot_be_skipped(self):
        needs = {name: {"result": "success"} for name in
                 ("plan", "gate", "native_cli", "shared_render")}
        CONTROL.require_success(needs, "shared-cli")
        needs["shared_render"]["result"] = "skipped"
        with self.assertRaises(ValueError):
            CONTROL.require_success(needs, "shared-cli")

    def test_artifact_rejects_mismatched_source_or_binary(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.write_bytes(b"test executable")
            destination = root / "artifact"
            with patch("ci_topology.identity", return_value={"commit": "one"}):
                artifact("stage-cli", source, destination)
                artifact("verify-cli", destination, ".")
            with patch("ci_topology.identity", return_value={"commit": "two"}):
                with self.assertRaises(ValueError):
                    artifact("verify-cli", destination, ".")
            (destination / "animatix").write_bytes(b"corrupt")
            with patch("ci_topology.identity", return_value={"commit": "one"}):
                with self.assertRaises(ValueError):
                    artifact("verify-cli", destination, ".")

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


if __name__ == "__main__":
    unittest.main()
