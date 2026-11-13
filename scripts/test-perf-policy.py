#!/usr/bin/env python3
"""Deterministic tests for the perf materiality classifier."""

import json
import tempfile
import unittest
from pathlib import Path

import importlib.util

SPEC = importlib.util.spec_from_file_location("perf_policy", Path(__file__).with_name("perf-policy.py"))
policy_module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(policy_module)


class PerfPolicyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.policy = policy_module.load_policy(Path(__file__).with_name("perf-policy.json"))

    def tearDown(self):
        self.temp.cleanup()

    def estimate(self, name, mean, lower=None, upper=None):
        lower = mean if lower is None else lower
        upper = mean if upper is None else upper
        path = self.root / name
        path.write_text(json.dumps({"mean": {
            "point_estimate": mean,
            "confidence_interval": {
                "confidence_level": 0.95,
                "lower_bound": lower,
                "upper_bound": upper,
            },
        }}))
        return path

    def compare(self, bench_id, base, current):
        return policy_module.classify(self.policy, bench_id, base, current, 5.0)

    def test_small_diagnostics_change_is_informational(self):
        base = self.estimate("b.json", 560, 559, 561)
        current = self.estimate("c.json", 617, 616, 618)
        result = self.compare("analyzer_diagnostics__large", base, current)
        self.assertEqual(result["role"], "signal_only")
        self.assertEqual(result["verdict"], "INFO")
        self.assertGreater(result["change_pct"], 5)

    def test_tiny_leaf_large_percentage_is_informational(self):
        base = self.estimate("b.json", 1, 0.99, 1.01)
        current = self.estimate("c.json", 2, 1.99, 2.01)
        result = self.compare("interpolate_f32", base, current)
        self.assertEqual(result["verdict"], "INFO")

    def test_material_frame_regression_blocks(self):
        base = self.estimate("b.json", 1_000_000, 999_000, 1_001_000)
        current = self.estimate("c.json", 1_300_000, 1_299_000, 1_301_000)
        result = self.compare("timeline_evaluate_1s", base, current)
        self.assertEqual(result["verdict"], "BLOCK")
        self.assertGreaterEqual(result["impact_lower_ns"], result["budget_ns"])

    def test_full_tier_demo_render_iteration_is_a_frame(self):
        rule = policy_module.role_for(self.policy, "demo_frame__tour/timing")
        self.assertEqual(rule["role"], "frame")

    def test_material_user_action_regression_blocks(self):
        base = self.estimate("b.json", 1_000_000, 999_000, 1_001_000)
        current = self.estimate("c.json", 1_300_000, 1_299_000, 1_301_000)
        result = self.compare("analyzer_update__large", base, current)
        self.assertEqual(result["verdict"], "BLOCK")

    def test_noisy_interval_is_not_claimed_as_regression(self):
        base = self.estimate("b.json", 1_000_000, 900_000, 1_100_000)
        current = self.estimate("c.json", 1_030_000, 930_000, 1_130_000)
        result = self.compare("timeline_evaluate_1s", base, current)
        self.assertEqual(result["statistical_status"], "INCONCLUSIVE")
        self.assertEqual(result["verdict"], "INCONCLUSIVE")

    def test_precise_small_delta_is_below_both_blocking_budgets(self):
        base = self.estimate("b.json", 1_000_000, 995_000, 1_005_000)
        current = self.estimate("c.json", 1_020_000, 1_015_000, 1_025_000)
        result = self.compare("timeline_evaluate_1s", base, current)
        self.assertEqual(result["statistical_status"], "NO_SIGNAL")
        self.assertEqual(result["verdict"], "OK")

    def test_overlap_at_signal_and_materiality_boundary_is_inconclusive(self):
        base = self.estimate("b.json", 1_000_000, 990_000, 1_010_000)
        current = self.estimate("c.json", 1_200_000, 1_100_000, 1_300_000)
        result = self.compare("timeline_evaluate_1s", base, current)
        self.assertEqual(result["verdict"], "INCONCLUSIVE")

    def test_large_primitive_sentinel_still_blocks(self):
        base = self.estimate("b.json", 10, 9.9, 10.1)
        current = self.estimate("c.json", 10_001, 9_999, 10_003)
        result = self.compare("property_plan_lookup_and_sample", base, current)
        self.assertEqual(result["verdict"], "BLOCK")

    def test_sublimit_sentinel_change_remains_visible_as_info(self):
        base = self.estimate("b.json", 5_000, 4_999, 5_001)
        current = self.estimate("c.json", 6_000, 5_999, 6_001)
        result = self.compare("property_plan_lookup_and_sample", base, current)
        self.assertEqual(result["statistical_status"], "CLEAR_SIGNAL")
        self.assertEqual(result["verdict"], "INFO")

    def test_missing_invalid_zero_and_unknown_inputs_fail_closed(self):
        with self.assertRaises(ValueError):
            policy_module.read_estimate(self.root / "missing.json")
        zero = self.estimate("zero.json", 0)
        valid = self.estimate("valid.json", 1)
        with self.assertRaises(ValueError):
            self.compare("timeline_evaluate_1s", zero, valid)
        with self.assertRaises(ValueError):
            self.compare("unknown_future_bench", valid, valid)
        malformed = self.root / "malformed.json"
        malformed.write_text('{"mean":{"point_estimate":1}}')
        with self.assertRaises(ValueError):
            self.compare("timeline_evaluate_1s", valid, malformed)

    def test_unknown_current_row_cannot_hide_as_uncovered_baseline(self):
        baseline_dir = self.root / "baseline"
        current_dir = self.root / "current"
        baseline_dir.mkdir()
        current_dir.mkdir()
        self.estimate("timeline_evaluate_1s.json", 10).replace(baseline_dir / "timeline_evaluate_1s.json")
        self.estimate("future_unclassified_row.json", 10).replace(current_dir / "future_unclassified_row.json")
        with self.assertRaises(ValueError):
            policy_module.validate_dirs(self.policy, baseline_dir, current_dir)

    def test_report_path_cannot_overwrite_an_estimate(self):
        directory = self.root / "inputs"
        directory.mkdir()
        raw = self.estimate("raw.json", 1).replace(directory / "raw.json")
        with self.assertRaises(ValueError):
            policy_module.validate_report_path(raw, directory)


if __name__ == "__main__":
    unittest.main()
