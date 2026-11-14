"""Pure tests for real Actions job timing and topology verdicts."""

import importlib.util
from datetime import datetime, timedelta, timezone
from pathlib import Path
import unittest

SCRIPT = Path(__file__).with_name("topology.py")
SPEC = importlib.util.spec_from_file_location("topology", SCRIPT)
TOPOLOGY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TOPOLOGY)

SHA = "a" * 40


def timestamp(seconds):
    return (datetime(2026, 11, 14, tzinfo=timezone.utc) + timedelta(seconds=seconds)).isoformat().replace("+00:00", "Z")


def job(identifier, name, start, end, *, conclusion="success", run_id=7, steps=None):
    return {
        "id": identifier, "run_id": run_id, "head_sha": SHA, "name": name,
        "conclusion": conclusion, "created_at": timestamp(start),
        "started_at": timestamp(start), "completed_at": timestamp(end),
        "steps": steps or [],
    }


def baseline_jobs(fail=False):
    return [
        job(1, "Plan", 0, 10),
        job(2, "Check & Lint (ubuntu-latest)", 10, 70,
            steps=[
                {"name": "Install Nix", "started_at": timestamp(10), "completed_at": timestamp(12)},
                {"name": "Gate", "started_at": timestamp(12), "completed_at": timestamp(60)},
            ]),
        job(3, "Test & Examples (ubuntu-latest)", 10, 100,
            conclusion="failure" if fail else "success"),
        job(4, "CI Required", 100, 105),
    ]


class TopologyTests(unittest.TestCase):
    def test_actions_job_timing_critical_path_and_step_buckets(self):
        result = TOPOLOGY.analyze_jobs(baseline_jobs(fail=True), "baseline", "fixture.json")
        self.assertTrue(result["complete"], result["issues"])
        self.assertEqual(result["critical_path_seconds"], 105)
        self.assertEqual(result["critical_path_jobs"], [
            "Plan", "CI Required",
        ])
        self.assertEqual(result["inferred_active_path_seconds"], 105)
        self.assertEqual(result["inferred_active_path_jobs"], [
            "Plan", "Test & Examples (ubuntu-latest)", "CI Required",
        ])
        self.assertEqual(result["wall_seconds"], 105)
        self.assertEqual(result["runner_minutes"], 165 / 60)
        self.assertEqual(result["setup_step_seconds"], 2)
        self.assertEqual(result["gate_step_seconds"], 48)
        self.assertEqual(result["first_failure_job"], "Test & Examples (ubuntu-latest)")
        self.assertEqual(result["first_failure_seconds"], 100)

    def test_shared_cli_critical_path_contains_producer_and_consumer(self):
        jobs = [
            job(1, "Plan", 0, 10),
            job(2, "Fast Checks (ubuntu-latest)", 10, 25),
            job(3, "Experimental CLI Producer", 10, 60),
            job(4, "Experimental Shared CLI Consumer", 60, 80),
            job(5, "CI Required", 80, 85),
        ]
        for row in jobs:
            row["created_at"] = timestamp(0)
        result = TOPOLOGY.analyze_jobs(jobs, "shared-cli")
        self.assertEqual(result["critical_path_seconds"], 85)
        self.assertEqual(result["critical_path_jobs"], ["Plan", "CI Required"])
        self.assertEqual(result["inferred_active_path_jobs"], [
            "Plan", "Experimental CLI Producer", "Experimental Shared CLI Consumer", "CI Required",
        ])
        self.assertEqual(result["inferred_active_path_seconds"], 85)

    def test_incomplete_and_source_mismatch_reject_adoption(self):
        baseline = [TOPOLOGY.analyze_jobs(baseline_jobs(), "baseline")]
        missing = TOPOLOGY.compare({"baseline": baseline, "grouped": [], "shared-cli": []}, min_runs=1)
        self.assertEqual(missing["grouped"]["status"], "INCOMPLETE")
        self.assertEqual(missing["grouped"]["adoption"], "reject")

        grouped_jobs = baseline_jobs()
        grouped_jobs[1]["head_sha"] = "b" * 40
        grouped_jobs[1]["name"] = "Native Grouped (ubuntu-latest)"
        grouped = [TOPOLOGY.analyze_jobs(grouped_jobs, "grouped")]
        verdict = TOPOLOGY.compare({"baseline": baseline, "grouped": grouped}, min_runs=1)
        self.assertEqual(verdict["grouped"]["status"], "INCOMPLETE")
        self.assertTrue(any("same verified source" in reason for reason in verdict["grouped"]["reasons"]))

    def test_nightly_jobs_are_filtered_to_the_selected_reusable_workflow(self):
        selected = [dict(row, name="ci_baseline / " + row["name"]) for row in baseline_jobs()]
        expected_skips = [
            job(20, "ci_baseline / Experimental CLI Producer", 0, 0, conclusion="skipped"),
            job(21, "ci_baseline / Experimental Shared CLI Consumer", 0, 0, conclusion="skipped"),
        ]
        for row in expected_skips:
            row["created_at"] = row["started_at"] = row["completed_at"] = None
        unrelated = [
            job(10, "check / Check for changes", 0, 40),
            job(11, "ci_grouped / Plan", 0, 20),
            job(12, "ci_shared_cli / Experimental CLI Producer", 0, 50),
        ]
        result = TOPOLOGY.analyze_jobs(selected + expected_skips + unrelated, "baseline")
        self.assertEqual(result["job_count"], 4)
        self.assertTrue(result["complete"], result["issues"])
        self.assertTrue(result["successful"], result["issues"])
        self.assertEqual(result["critical_path_jobs"], [
            "ci_baseline / Plan", "ci_baseline / CI Required",
        ])
        self.assertEqual(result["inferred_active_path_jobs"], [
            "ci_baseline / Plan", "ci_baseline / Test & Examples (ubuntu-latest)",
            "ci_baseline / CI Required",
        ])

    def test_prefixed_reusable_run_must_match_requested_topology(self):
        jobs = [job(1, "ci_grouped / Plan", 0, 10)]
        with self.assertRaisesRegex(ValueError, "subtree"):
            TOPOLOGY.analyze_jobs(jobs, "baseline")

    def test_shared_cli_selected_producer_and_consumer_must_succeed(self):
        jobs = [
            job(1, "Plan", 0, 10),
            job(2, "Experimental CLI Producer", 10, 10, conclusion="skipped"),
            job(3, "Experimental Shared CLI Consumer", 10, 10, conclusion="success"),
            job(4, "CI Required", 10, 15),
        ]
        jobs[1]["created_at"] = jobs[1]["started_at"] = jobs[1]["completed_at"] = None
        result = TOPOLOGY.analyze_jobs(jobs, "shared-cli")
        self.assertFalse(result["complete"])
        self.assertFalse(result["successful"])
        self.assertIn("shared CLI producer job did not succeed", result["issues"])

    def test_incomplete_run_keeps_observed_metrics_but_cannot_pass(self):
        jobs = baseline_jobs()
        jobs = jobs[:-1]
        record = TOPOLOGY.analyze_jobs(jobs, "baseline")
        self.assertFalse(record["complete"])
        self.assertIsNone(TOPOLOGY.median([record], "critical_path_seconds"))
        self.assertEqual(TOPOLOGY.median([record], "inferred_active_path_seconds"), 100)

    def test_failed_candidate_run_cannot_pass_thresholds(self):
        baseline = [TOPOLOGY.analyze_jobs(baseline_jobs(), "baseline")]
        candidate_jobs = [
            job(1, "Plan", 0, 10),
            job(2, "Native Grouped (ubuntu-latest)", 10, 80, conclusion="failure"),
            job(3, "CI Required", 80, 85, conclusion="failure"),
        ]
        candidate = [TOPOLOGY.analyze_jobs(candidate_jobs, "grouped")]
        verdict = TOPOLOGY.compare(
            {"baseline": baseline, "grouped": candidate}, min_runs=1,
            min_wall_improvement=0, max_runner_regression=1,
        )
        self.assertEqual(verdict["grouped"]["status"], "REJECT")
        self.assertTrue(any("workflow runs failed" in reason for reason in verdict["grouped"]["reasons"]))

    def test_thresholds_are_median_based_and_failure_feedback_is_reported(self):
        baseline = [TOPOLOGY.analyze_jobs(baseline_jobs(), "baseline") for _ in range(3)]
        grouped_jobs = [
            job(1, "Plan", 0, 10),
            job(2, "Native Grouped (ubuntu-latest)", 10, 80),
            job(3, "CI Required", 80, 85),
        ]
        grouped = [TOPOLOGY.analyze_jobs(grouped_jobs, "grouped") for _ in range(3)]
        verdict = TOPOLOGY.compare({"baseline": baseline, "grouped": grouped}, min_runs=3)
        self.assertEqual(verdict["grouped"]["status"], "PASS_THRESHOLDS")
        self.assertEqual(verdict["grouped"]["critical_path_improvement"], 20 / 105)
        self.assertFalse(verdict["grouped"]["failure_feedback_observed"])


if __name__ == "__main__":
    unittest.main()
