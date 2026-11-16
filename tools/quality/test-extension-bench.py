#!/usr/bin/env python3
"""No timings or Cargo builds: fixtures test health-result validation/isolation."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("extension_results", HERE / "extension-bench-results.py")
results = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(results)


def est(point=50, lower=45, upper=55):
    return {"point_estimate": point, "standard_error": 1,
            "confidence_interval": {"confidence_level": 0.95,
                                    "lower_bound": lower, "upper_bound": upper}}


def fixture(home, name, slope=True):
    new = home / name / "new"
    new.mkdir(parents=True)
    (new / "benchmark.json").write_text(json.dumps({
        "group_id": name, "function_id": None, "value_str": None, "throughput": None,
        "full_id": name, "directory_name": name, "title": name,
    }))
    (new / "sample.json").write_text(json.dumps({
        "sampling_mode": "Linear" if slope else "Flat",
        "iters": [10.0, 20.0], "times": [500.0, 1000.0],
    }))
    (new / "estimates.json").write_text(json.dumps({
        **{key: est() for key in ("mean", "median", "median_abs_dev", "std_dev")},
        "slope": est() if slope else None,
    }))


class ResultTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.home = Path(self.temp.name) / "criterion"
        for name in results.EXPECTED:
            fixture(self.home, name)

    def edit(self, filename, change, name=results.EXPECTED[0]):
        path = self.home / name / "new" / filename
        data = json.loads(path.read_text())
        change(data)
        path.write_text(json.dumps(data))

    def test_complete_four_samples_and_exact_lower_bound(self):
        self.edit("estimates.json", lambda data: data.update(slope=est(11000, 10000, 12000)))
        report = results.validate(self.home, 10000)
        self.assertTrue(report["passed"])
        self.assertFalse(report["historical_comparison"])
        self.assertEqual(len(report["rows"]), 4)
        self.edit("estimates.json", lambda data: data.update(slope=est(11000, 10000.001, 12000)))
        self.assertFalse(results.validate(self.home, 10000)["passed"])

    def test_slope_preferred_and_mean_used_when_null(self):
        self.edit("estimates.json", lambda data: data.update(mean=est(20000, 19000, 21000)))
        self.assertTrue(results.validate(self.home, 10000)["passed"])
        self.edit("estimates.json", lambda data: data.update(slope=None))
        self.edit("sample.json", lambda data: data.update(sampling_mode="Flat"))
        self.assertFalse(results.validate(self.home, 10000)["passed"])

    def test_negative_lower_bound_preserves_original_semantics(self):
        self.edit("estimates.json", lambda data: data.update(slope=est(50, -1, 100)))
        self.assertTrue(results.validate(self.home, 10000)["passed"])

    def test_empty_missing_and_extra_inventory(self):
        with self.assertRaises(ValueError):
            results.validate(Path(self.temp.name) / "absent")
        directory = self.home / results.EXPECTED[-1] / "new"
        shutil.rmtree(directory)
        with self.assertRaisesRegex(ValueError, "inventory mismatch"):
            results.validate(self.home)
        fixture(self.home, results.EXPECTED[-1])
        fixture(self.home, "unexpected")
        with self.assertRaisesRegex(ValueError, "extra="):
            results.validate(self.home)

    def test_missing_corrupt_and_symlinked_file(self):
        path = self.home / results.EXPECTED[-1] / "new" / "sample.json"
        good = path.read_text()
        path.unlink()
        with self.assertRaises(ValueError):
            results.validate(self.home)
        path.write_text("{")
        with self.assertRaises(ValueError):
            results.validate(self.home)
        old = Path(self.temp.name) / "old-sample.json"
        old.write_text(good)
        path.unlink()
        path.symlink_to(old)
        with self.assertRaises(ValueError):
            results.validate(self.home)

    def test_bad_identity_and_nonfinite_estimates(self):
        self.edit("benchmark.json", lambda data: data.update(full_id="old"))
        with self.assertRaisesRegex(ValueError, "identity"):
            results.validate(self.home)
        self.edit("benchmark.json", lambda data: data.update(full_id=results.EXPECTED[0]))
        for value in [float("nan"), float("inf"), True, "5"]:
            self.edit("estimates.json", lambda data: data.update(slope=est(value)))
            with self.assertRaises(ValueError):
                results.validate(self.home)

    def test_invalid_samples_and_intervals(self):
        for change in [lambda d: d.update(sampling_mode=[]),
                       lambda d: d.update(iters=[]), lambda d: d.update(times=[1]),
                       lambda d: d.update(iters=[0, 2]), lambda d: d.update(iters=[1.5, 2]),
                       lambda d: d.update(times=[float("inf"), 2])]:
            path = self.home / results.EXPECTED[0] / "new" / "sample.json"
            original = path.read_text()
            self.edit("sample.json", change)
            with self.assertRaises(ValueError):
                results.validate(self.home)
            path.write_text(original)
        self.edit("estimates.json", lambda data: data.update(slope=est(50, 60, 40)))
        with self.assertRaises(ValueError):
            results.validate(self.home)

    def test_evidence_is_bounded_allowlisted_and_retains_failure_status(self):
        run = Path(self.temp.name) / "run"
        run.mkdir()
        (run / "build.log").write_bytes(b"x" * 100000)
        (run / "summary.json").write_bytes(b"x" * 100000)
        (run / "unrequested-env.txt").write_text("private environment")
        evidence = results.stage_evidence(run, 7)
        manifest = json.loads((evidence / "evidence.json").read_text())
        self.assertEqual(manifest["exit_code"], 7)
        self.assertEqual(len(manifest["files"]), 2)
        self.assertTrue(all(f["bytes"] == 65536 and f["truncated"] for f in manifest["files"]))
        self.assertFalse((evidence / "unrequested-env.txt").exists())
        self.assertTrue((evidence / "summary.json.truncated.txt").exists())


class RunnerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        tools = self.root / "tools/quality"
        tools.mkdir(parents=True)
        for name in ["extension-bench.sh", "extension-bench-results.py"]:
            shutil.copyfile(HERE / name, tools / name)
        binary = self.root / "bin"
        binary.mkdir()
        fake = binary / "cargo"
        # A fake Cargo executable proves the shell's contract without a compiler
        # or performance measurement. It writes the actual Criterion JSON shape.
        fake.write_text(f'''#!{sys.executable}
import importlib.util, os, pathlib, sys
spec=importlib.util.spec_from_file_location("fixtures", {str(Path(__file__).resolve())!r})
m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
assert "CARGO_CRITERION_PORT" not in os.environ
assert os.environ["CARGO_TARGET_DIR"] == "compile-cache-unchanged"
if os.environ.get("FAIL_CARGO"): sys.exit(7)
if "--no-run" in sys.argv: sys.exit(0)
assert "--noplot" in sys.argv and "--save-baseline" in sys.argv
home=pathlib.Path(os.environ["CRITERION_HOME"])
names=m.results.EXPECTED[:1] if "vm" in sys.argv else m.results.EXPECTED[1:]
for name in names:
    if os.environ.get("OMIT_LAST") and name == m.results.EXPECTED[-1]: continue
    m.fixture(home,name)
''')
        fake.chmod(0o755)
        self.env = {**os.environ, "PATH": str(binary) + os.pathsep + os.environ["PATH"],
                    "CARGO_TARGET_DIR": "compile-cache-unchanged", "CARGO_CRITERION_PORT": "12345",
                    "CRITERION_HOME": str(self.root / "stale-override")}

    def run_gate(self, **overrides):
        return subprocess.run(["bash", "tools/quality/extension-bench.sh", "--quick",
                               "--max-plan-ns", "10000"], cwd=self.root,
                              env={**self.env, **overrides}, capture_output=True, text=True)

    def test_old_cache_untouched_and_each_run_is_unique(self):
        stale = self.root / "target/criterion/old/base"
        stale.mkdir(parents=True)
        (stale / "estimates.json").write_text("partial old result; no sample")
        for _ in range(2):
            result = self.run_gate()
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        summaries = list((self.root / "target/ci").glob("ext-bench.*/summary.json"))
        self.assertEqual(len(summaries), 2)
        self.assertTrue(all(json.loads(p.read_text())["passed"] for p in summaries))
        self.assertFalse((stale / "sample.json").exists())
        self.assertEqual((stale / "estimates.json").read_text(), "partial old result; no sample")
        self.assertFalse((self.root / "stale-override").exists())

    def test_zero_exit_cargo_with_missing_timeline_sample_fails(self):
        result = self.run_gate(OMIT_LAST="1")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("inventory mismatch", result.stdout)

    def test_cargo_failure_propagates_through_tee(self):
        result = self.run_gate(FAIL_CARGO="1")
        self.assertEqual(result.returncode, 7)
        evidence = next((self.root / "target/ci").glob("ext-bench.*/evidence/evidence.json"))
        self.assertEqual(json.loads(evidence.read_text())["exit_code"], 7)


if __name__ == "__main__":
    unittest.main()
