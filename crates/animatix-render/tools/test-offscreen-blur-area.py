#!/usr/bin/env python3
import os
import signal
import time
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("driver", Path(__file__).with_name("offscreen-blur-area.py"))
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)


def fixture(size, blur):
    rect = f"box: Rect, size: ({size}, {size}), color: (1, 1, 1, 1), opacity: 1, stroke_width: 0"
    actor = f"panel: Filter, at: (256, 256), opacity: 1 {{\nsoft: Blur, radius: 6\n{rect}\n}}" if blur else f"{rect}, at: (256, 256)"
    return f"config {{ canvas_size: (512, 512) }}\n#0s\nbackground: Rect, at: (256, 256), size: (512, 512), color: (0, 0, 0, 1), opacity: 1, stroke_width: 0\n{actor}\n"


def result():
    return {"schema": 1, "warmup": 4, "columns": ["host_evaluate_encode_submit_ns", "completion_map_cpu_copy_ns", "total_completed_frame_ns"], "workload": "offscreen_blur_area_v1", "width": 512, "height": 512, "queue_depth": 1, "radius": 6,
            "class": "hardware_reported", "adapter": {"device_type": "DiscreteGpu", "backend": "Vulkan"},
            "cases": [{"size": s, "mode": m, "source": fixture(s, m == "blur"), "samples_ns": [[1, 2, 3]]}
                      for s in (64, 128, 256) for m in ("plain", "blur")]}


class Contract(unittest.TestCase):
    def test_valid_six_and_hashes(self):
        value = result()
        driver.validate(value, 1, False)
        self.assertTrue(all(len(c["source_sha256"]) == 64 for c in value["cases"]))

    def test_missing_duplicate_malformed_or_invalid_duration_fails(self):
        mutations = [lambda x: x["cases"].pop(), lambda x: x["cases"].__setitem__(0, x["cases"][1]),
                     lambda x: x["cases"][0].__setitem__("samples_ns", []),
                     lambda x: x["cases"][0].__setitem__("samples_ns", [[1, 2, 4]]),
                     lambda x: x["cases"][0].__setitem__("samples_ns", [[1, float("nan"), 3]]),
                     lambda x: x.__setitem__("queue_depth", 2),
                     lambda x: x.__setitem__("warmup", 0), lambda x: x.__setitem__("schema", 0),
                     lambda x: x.__setitem__("columns", [])]
        for mutation in mutations:
            value = result()
            mutation(value)
            with self.assertRaises(ValueError): driver.validate(value, 1, False)

    def test_software_requires_opt_in(self):
        value = result()
        value["class"] = "software"
        value["adapter"]["device_type"] = "Cpu"
        with self.assertRaises(ValueError): driver.validate(value, 1, False)
        driver.validate(value, 1, True)

    def test_optimized_python_rejects_bad_input(self):
        script = f"import runpy; m=runpy.run_path({str(Path(__file__).with_name('offscreen-blur-area.py'))!r}); m['validate']({result()!r} | {{'queue_depth': 2}}, 1, False)"
        completed = subprocess.run([sys.executable, "-O", "-c", script], capture_output=True)
        self.assertNotEqual(completed.returncode, 0)
        self.assertIn(b"ValueError", completed.stderr)

    def test_sigterm_reaps_nested_build_and_records_interrupted(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            pidfile = root / "nested.pid"
            cargo = root / "cargo"
            cargo.write_text(f"#!{sys.executable}\nimport subprocess,time\np=subprocess.Popen([{sys.executable!r},'-c','import time; time.sleep(60)'])\nopen({str(pidfile)!r},'w').write(str(p.pid))\ntime.sleep(60)\n")
            cargo.chmod(0o755)
            rustc = root / "rustc"
            rustc.write_text("#!/bin/sh\necho test-rustc\n")
            rustc.chmod(0o755)
            command = [sys.executable, str(Path(__file__).with_name("offscreen-blur-area.py")), "--output-root", str(root / "runs")]
            process = subprocess.Popen(command, env={**os.environ, "RUSTFLAGS": "private-test-value-not-for-output", "PATH": str(root) + os.pathsep + os.environ["PATH"]}, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                deadline = time.monotonic() + 10
                while not pidfile.exists() and process.poll() is None and time.monotonic() < deadline:
                    time.sleep(0.02)
                self.assertTrue(pidfile.exists(), "nested process did not start")
                process.send_signal(signal.SIGTERM)
                process.communicate(timeout=5)
                self.assertNotEqual(process.returncode, 0)
                metadata = list((root / "runs").glob("*/metadata.json"))
                self.assertEqual(json.loads(metadata[0].read_text())["status"], "interrupted")
                self.assertNotIn("private-test-value-not-for-output", metadata[0].read_text())
                self.assertEqual(len(json.loads(metadata[0].read_text())["environment_value_sha256"]["RUSTFLAGS"]), 64)
                pid = int(pidfile.read_text())
                status = Path(f"/proc/{pid}/stat")
                # A killed orphan may await init reaping, but must not run.
                deadline = time.monotonic() + 2
                while status.exists() and status.read_text().split()[2] != "Z" and time.monotonic() < deadline:
                    time.sleep(0.02)
                self.assertTrue(not status.exists() or status.read_text().split()[2] == "Z")
            finally:
                if process.poll() is None:
                    process.kill()
                    process.communicate()

    def test_bounded_timeout_and_no_log_overwrite(self):
        with tempfile.TemporaryDirectory() as directory:
            out, err = Path(directory) / "out", Path(directory) / "err"
            with self.assertRaises(subprocess.TimeoutExpired):
                driver.bounded([sys.executable, "-c", "import time; time.sleep(60)"], 0.05, out, err)
            with self.assertRaises(FileExistsError):
                driver.bounded([sys.executable, "-c", "pass"], 1, out, err)


if __name__ == "__main__": unittest.main()
