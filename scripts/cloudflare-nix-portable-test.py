#!/usr/bin/env python3
"""No-network regression checks for the portable probe."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("probe", Path(__file__).with_name("cloudflare-nix-portable.py"))
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)


class ProbeTest(unittest.TestCase):
    def test_hash_mismatch(self):
        with tempfile.TemporaryDirectory() as root:
            binary = Path(root) / "binary"
            binary.write_bytes(b"not executable")
            with self.assertRaises(RuntimeError):
                p.verify(binary)

    def test_environment(self):
        with patch.dict(p.os.environ, {"SECRET": "do-not-inherit", "NP_DEBUG": "2", "NP_RUN": "bad"}):
            env = p.environment(Path("/tmp/probe"))
        self.assertNotIn("SECRET", env)
        self.assertNotIn("NP_DEBUG", env)
        self.assertNotIn("NP_RUN", env)
        self.assertEqual(env["NP_RUNTIME"], "nix")
        self.assertEqual(env["NP_CONF_SANDBOX"], "true")

    def test_no_space(self):
        with patch.object(p.shutil, "disk_usage") as disk, patch.object(p.bounded, "run") as run:
            disk.return_value.free = 0
            self.assertEqual(p.main(), 1)
            run.assert_not_called()

    def test_always_stop_and_order(self):
        for fail in (False, True):
            with tempfile.TemporaryDirectory() as root, patch.object(p.tempfile, "mkdtemp", return_value=root), \
                    patch.object(p.shutil, "disk_usage") as disk, patch.object(p, "verify"), \
                    patch.object(p.bounded, "run") as run:
                disk.return_value.free = 10 * 1024**3
                def fake(args, work, started):
                    if args[0] == "curl":
                        (Path(root) / "nix-portable").touch()
                    elif fail:
                        raise RuntimeError("runtime denied")
                run.side_effect = fake
                self.assertEqual(p.main(), 1)
                calls = [c.args[0] for c in run.call_args_list]
                self.assertIn("--version", calls[1])
                if not fail:
                    self.assertIn("--no-write-lock-file", calls[2])
                    self.assertIn("develop", calls[2])
                else:
                    self.assertEqual(len(calls), 2)


if __name__ == "__main__":
    unittest.main()
