#!/usr/bin/env python3
"""Hermetic regression tests for the observational phase wrapper."""
import concurrent.futures
from datetime import datetime, timedelta
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
from unittest import mock
import unittest

SCRIPT = Path(__file__).with_name("phase-timing.py")
spec = importlib.util.spec_from_file_location("phase_timing", SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class PhaseTimingTests(unittest.TestCase):
    def invoke(self, directory, code, extra=None):
        env = {**os.environ, "ANIMATIX_PHASE_TIMINGS_DIR": str(directory), **(extra or {})}
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--gate", "test", "--phase", "run",
             "--kind", "execution", "--", sys.executable, "-c", code],
            env=env, capture_output=True, timeout=15,
        )

    def test_output_and_arguments_are_unchanged(self):
        with tempfile.TemporaryDirectory() as directory:
            result = self.invoke(directory, 'import os; os.write(1,b"hello\\x00world"); os.write(2,b"error\\n")')
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stdout, b"hello\x00world")
            self.assertEqual(result.stderr, b"error\n")
            data = json.loads(next(Path(directory).glob("*.json")).read_text())
            self.assertEqual(data["exit_code"], 0)
            self.assertIsNone(data["signal"])
            self.assertGreaterEqual(data["duration_seconds"], 0)
            self.assertEqual(set(data), {"schema", "gate", "phase", "kind", "duration_seconds", "exit_code", "signal", "environment", "started_at", "finished_at"})

    def test_timestamps_and_bounded_metadata(self):
        env = {"CARGO_BUILD_JOBS": "9999", "CARGO_INCREMENTAL": "1"}
        for profile in ("DEV", "TEST", "RELEASE", "BENCH"):
            for key, value in {"DEBUG": "line-directives-only", "OPT_LEVEL": "3",
                               "INCREMENTAL": "false", "CODEGEN_UNITS": "9999",
                               "LTO": "false"}.items():
                env[f"CARGO_PROFILE_{profile}_{key}"] = value
        with tempfile.TemporaryDirectory() as directory:
            result = self.invoke(directory, 'pass', env)
            self.assertEqual(result.returncode, 0)
            payload = next(Path(directory).glob('*.json')).read_bytes()
            self.assertLess(len(payload), 8192)
            data = json.loads(payload)
            start = datetime.fromisoformat(data['started_at'])
            end = datetime.fromisoformat(data['finished_at'])
            self.assertEqual(start.utcoffset(), timedelta(0))
            self.assertEqual(end.utcoffset(), timedelta(0))
            self.assertLessEqual(start, end)
            self.assertEqual(data['environment'], env)

    def test_wrapper_cancellation_is_forwarded(self):
        with tempfile.TemporaryDirectory() as directory:
            ready = Path(directory) / 'ready'
            child_code = (
                'import pathlib,time; '
                f'pathlib.Path({str(ready)!r}).touch(); time.sleep(30)'
            )
            process = subprocess.Popen(
                [sys.executable, str(SCRIPT), '--gate', 'test', '--phase', 'wait',
                 '--kind', 'execution', '--', sys.executable, '-c', child_code],
                env={**os.environ, 'ANIMATIX_PHASE_TIMINGS_DIR': directory},
                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
            try:
                deadline = time.monotonic() + 10
                while not ready.exists() and time.monotonic() < deadline:
                    time.sleep(0.01)
                self.assertTrue(ready.exists(), 'child did not become ready')
                process.terminate()
                stdout, stderr = process.communicate(timeout=5)
                self.assertEqual(process.returncode, -signal.SIGTERM)
                self.assertEqual((stdout, stderr), (b'', b''))
                data = json.loads(next(Path(directory).glob('*.json')).read_text())
                self.assertEqual(data['signal'], signal.SIGTERM)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.communicate()

    @unittest.skipUnless(os.name == 'posix', 'process-group forwarding is POSIX-only')
    def test_nested_process_tree_and_single_group_delivery(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            # Both levels count TERM. The command reaps its child before exiting,
            # so the descendant PID must disappear, not merely become a zombie.
            descendant = (
                'import os,signal,time,pathlib; '
                f'root=pathlib.Path({directory!r}); '
                'signal.signal(signal.SIGTERM,lambda *_: '
                '(root.joinpath("nested-signals").open("a").write("term\\n"),exit(0))); '
                'root.joinpath("nested-pid").write_text(str(os.getpid())); '
                'time.sleep(30)'
            )
            command = (
                'import os,signal,time,pathlib,subprocess,sys; '
                f'root=pathlib.Path({directory!r}); '
                'signal.signal(signal.SIGTERM,lambda *_: '
                'root.joinpath("parent-signals").open("a").write("term\\n")); '
                f'child=subprocess.Popen([sys.executable,"-c",{descendant!r}]); '
                'child.wait(); time.sleep(0.1)'
            )
            process = subprocess.Popen(
                [sys.executable, str(SCRIPT), '--gate', 'test', '--phase', 'tree',
                 '--kind', 'execution', '--', sys.executable, '-c', command],
                env={**os.environ, 'ANIMATIX_PHASE_TIMINGS_DIR': directory},
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True,
            )
            try:
                deadline = time.monotonic() + 10
                while not (root / 'nested-pid').exists() and time.monotonic() < deadline:
                    time.sleep(0.01)
                self.assertTrue((root / 'nested-pid').exists())
                pid = int((root / 'nested-pid').read_text())
                # Simulate runner cancellation of the wrapper's process group.
                os.killpg(process.pid, signal.SIGTERM)
                stdout, stderr = process.communicate(timeout=5)
                self.assertEqual(process.returncode, 0, stderr)
                self.assertEqual((stdout, stderr), (b'', b''))
                for name in ('nested-signals', 'parent-signals'):
                    self.assertEqual((root / name).read_text().splitlines(), ['term'])
                with self.assertRaises(ProcessLookupError):
                    os.kill(pid, 0)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.communicate()

    def test_signal_before_child_assignment_is_not_lost(self):
        child = mock.Mock()
        child.wait.return_value = 0
        def spawn(_command, **_kwargs):
            signal.getsignal(signal.SIGTERM)(signal.SIGTERM, None)
            return child
        with mock.patch.object(module.subprocess, 'Popen', side_effect=spawn), mock.patch.object(module, 'record'), mock.patch.object(module.os, 'killpg', create=True) as killpg:
            self.assertEqual(module.main(['--gate', 'test', '--phase', 'run',
                                         '--kind', 'execution', '--', 'ignored']), 0)
            if os.name == "posix":
                killpg.assert_called_once_with(child.pid, signal.SIGTERM)
            else:
                child.send_signal.assert_called_once_with(signal.SIGTERM)

    def test_failure_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            self.assertEqual(self.invoke(directory, 'raise SystemExit(37)').returncode, 37)
            self.assertEqual(json.loads(next(Path(directory).glob('*.json')).read_text())["exit_code"], 37)

    def test_child_signal_is_preserved(self):
        for sig in (signal.SIGTERM, signal.SIGKILL, signal.SIGINT):
            with self.subTest(sig=sig), tempfile.TemporaryDirectory() as directory:
                result = self.invoke(directory, f'import os,signal; signal.signal(signal.SIGINT,signal.SIG_DFL); os.kill(os.getpid(),{sig})')
                self.assertEqual(result.returncode, -sig)
                data = json.loads(next(Path(directory).glob('*.json')).read_text())
                self.assertEqual(data['signal'], sig)
                self.assertIsNone(data['exit_code'])

    def test_concurrent_invocations_are_unique(self):
        with tempfile.TemporaryDirectory() as directory:
            with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
                results = list(pool.map(lambda _: self.invoke(directory, 'pass'), range(16)))
            self.assertTrue(all(r.returncode == 0 for r in results))
            files = list(Path(directory).glob('*.json'))
            self.assertEqual(len(files), 16)
            for path in files:
                self.assertEqual(json.loads(path.read_text())['exit_code'], 0)

    def test_failed_record_does_not_mask_child(self):
        with tempfile.TemporaryDirectory() as directory:
            blocked = Path(directory) / 'file'
            blocked.write_text('original')
            for code in (0, 19):
                result = self.invoke(blocked, f'print("output"); raise SystemExit({code})')
                self.assertEqual(result.returncode, code)
                self.assertEqual(result.stdout, b'output\n')
                self.assertEqual(result.stderr, b'warning: phase timing artifact could not be recorded\n')
            self.assertEqual(blocked.read_text(), 'original')

    def test_environment_is_strictly_allowlisted(self):
        self.assertEqual(module.safe_environment({
            'GH_TOKEN': 'secret', 'CARGO_REGISTRIES_PRIVATE_TOKEN': 'secret',
            'CARGO_BUILD_JOBS': '8', 'CARGO_INCREMENTAL': '1',
            'CARGO_PROFILE_DEV_DEBUG': 'line-tables-only',
            'CARGO_PROFILE_TEST_OPT_LEVEL': '3',
            'CARGO_PROFILE_DEV_LTO': 'thin',
            'CARGO_TARGET_DIR': '/secret/path',
            'CARGO_PROFILE_DEV_DEBUG_EXTRA': 'secret',
        }), {'CARGO_BUILD_JOBS': '8', 'CARGO_INCREMENTAL': '1',
             'CARGO_PROFILE_DEV_DEBUG': 'line-tables-only',
             'CARGO_PROFILE_TEST_OPT_LEVEL': '3', 'CARGO_PROFILE_DEV_LTO': 'thin'})
        for value in ('/secret/path', 'secret', '9999999999999', '-1', '2\nTOKEN=x', None):
            self.assertEqual(module.safe_environment({'CARGO_BUILD_JOBS': value,
                'CARGO_PROFILE_DEV_DEBUG': value, 'CARGO_INCREMENTAL': value}), {})

    def test_secret_never_enters_artifact(self):
        with tempfile.TemporaryDirectory() as directory:
            result = self.invoke(directory, 'secret="TOKEN_DO_NOT_RECORD"', {'GH_TOKEN': 'TOKEN_DO_NOT_RECORD'})
            self.assertEqual(result.returncode, 0)
            self.assertNotIn('TOKEN_DO_NOT_RECORD', next(Path(directory).glob('*.json')).read_text())

    def test_missing_command_exit(self):
        with tempfile.TemporaryDirectory() as directory:
            result = subprocess.run([sys.executable, str(SCRIPT), '--gate', 'test', '--phase', 'run', '--kind', 'mixed', '--', '/no/such/command'], env={**os.environ, 'ANIMATIX_PHASE_TIMINGS_DIR': directory}, capture_output=True)
            self.assertEqual(result.returncode, 127)
            self.assertNotIn(b'/no/such/command', result.stderr)


if __name__ == '__main__':
    unittest.main()
