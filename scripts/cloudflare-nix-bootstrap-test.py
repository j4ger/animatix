#!/usr/bin/env python3
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import MagicMock, patch

spec = importlib.util.spec_from_file_location("bootstrap", Path(__file__).with_name("cloudflare-nix-bootstrap.py"))
b = importlib.util.module_from_spec(spec)
spec.loader.exec_module(b)


class BootstrapTest(unittest.TestCase):
    def test_direct_stage_invocation_refused(self):
        stage = Path(__file__).with_name("cloudflare-nix-rootless-stage.sh")
        result = subprocess.run(["bash", str(stage), "/", str(stage.parent.parent.resolve()),
                                 "version", os.readlink("/proc/self/ns/user"),
                                 os.readlink("/proc/self/ns/mnt"), str(os.getuid())],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn("BLOCKED", result.stderr)

    def test_bad_download_never_extracted(self):
        def fake_run(args, work, started):
            (work / "nix.tar.xz").write_bytes(b"wrong archive")
        with tempfile.TemporaryDirectory() as directory, \
             patch.object(b.tempfile, "mkdtemp", return_value=directory), \
             patch.object(b.shutil, "which", return_value="present"), \
             patch.object(b.shutil, "disk_usage", return_value=MagicMock(free=10**10)), \
             patch.object(b, "run", side_effect=fake_run) as run:
            self.assertEqual(b.main(), 1)
            self.assertEqual(run.call_count, 1)
            self.assertEqual(run.call_args.args[0][0], "curl")

    def test_success_and_namespace_failure_both_prevent_deploy(self):
        for fail in [True, False]:
            def fake_run(args, work, started):
                if args[0] == "tar":
                    unpacked = work / f"nix-{b.VERSION}-x86_64-linux"
                    (unpacked / "store").mkdir(parents=True)
                    (unpacked / ".reginfo").write_text("")
                if args[0] == "unshare" and fail:
                    raise RuntimeError("unshare denied")
            with tempfile.TemporaryDirectory() as directory, \
                 patch.object(b.tempfile, "mkdtemp", return_value=directory), \
                 patch.object(b.shutil, "which", return_value="present"), \
                 patch.object(b.shutil, "disk_usage", return_value=MagicMock(free=10**10)), \
                 patch.object(b, "verify_archive"), patch.object(b, "run", side_effect=fake_run):
                self.assertEqual(b.main(), 1)

    def test_bad_hash_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "nix.tar.xz"
            archive.write_bytes(b"untrusted bytes")
            with self.assertRaisesRegex(RuntimeError, "SHA256 mismatch"):
                b.verify_archive(archive)

    def test_missing_tool_stops_before_download(self):
        with patch.object(b.shutil, "which", return_value=None), patch.object(b, "run") as run:
            self.assertEqual(b.main(), 1)
            run.assert_not_called()

    def test_low_space_stops_before_download(self):
        with patch.object(b.shutil, "which", return_value="present"), \
             patch.object(b.shutil, "disk_usage", return_value=MagicMock(free=0)), \
             patch.object(b, "run") as run:
            self.assertEqual(b.main(), 1)
            run.assert_not_called()

    def test_budget_kills_process_group(self):
        for expired in [True, False]:
            process = MagicMock(pid=123, returncode=None)
            process.poll.return_value = None
            with patch.object(b.subprocess, "Popen", return_value=process) as popen, \
                 patch.object(b.time, "sleep"), \
                 patch.object(b.time, "monotonic", return_value=1000 if expired else 1), \
                 patch.object(b.shutil, "disk_usage", return_value=MagicMock(free=10**10 if expired else 0)), \
                 patch.object(b.os, "killpg") as kill:
                with self.assertRaisesRegex(RuntimeError, "resource budget"):
                    b.run(["example"], "/tmp", 0)
                popen.assert_called_once_with(["example"], start_new_session=True)
                kill.assert_called_once_with(123, b.signal.SIGTERM)

    def test_failed_command_propagates(self):
        process = MagicMock(returncode=7)
        process.poll.return_value = 7
        with patch.object(b.subprocess, "Popen", return_value=process):
            with self.assertRaisesRegex(RuntimeError, "exit=7"):
                b.run(["example"], "/tmp", 0)

    def test_stubborn_process_is_killed(self):
        process = MagicMock(pid=123)
        process.poll.return_value = None
        process.wait.side_effect = [subprocess.TimeoutExpired("example", 10), 0]
        with patch.object(b.subprocess, "Popen", return_value=process), \
             patch.object(b.time, "sleep"), patch.object(b.time, "monotonic", return_value=1000), \
             patch.object(b.os, "killpg") as kill:
            with self.assertRaises(RuntimeError):
                b.run(["example"], "/tmp", 0)
            self.assertEqual(kill.call_args_list[-1].args, (123, b.signal.SIGKILL))


if __name__ == "__main__":
    unittest.main()
