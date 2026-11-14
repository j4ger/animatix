"""Focused safety and evidence tests for the opt-in experiment harness."""

from __future__ import annotations

from pathlib import Path
import os
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
from scripts.experiments import run as experiment_run

from scripts.experiments.common import (
    ExperimentFailure, ExperimentOutput, run_timed, source_fingerprint,
)
from scripts.experiments import linkers


class ExperimentOutputTests(unittest.TestCase):
    def test_only_marked_target_can_be_reset(self):
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            output = ExperimentOutput(repo, Path("target/experiments/linker"), "linker", {})
            target = output.owned_target("build-targets/linux/cold/mold")
            sentinel = target / "sentinel"
            sentinel.write_text("owned")
            reset = output.owned_target("build-targets/linux/cold/mold", reset=True)
            self.assertFalse((reset / "sentinel").exists())
            self.assertTrue((reset / ".animatix-owned-target.json").exists())

    def test_unmarked_nonempty_target_is_never_deleted(self):
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            output = ExperimentOutput(repo, Path("target/experiments/linker"), "linker", {})
            target = output.directory / "target-unowned"
            target.mkdir()
            keep = target / "keep"
            keep.write_text("user data")
            with self.assertRaises(ExperimentFailure):
                output.owned_target("target-unowned", reset=True)
            self.assertEqual(keep.read_text(), "user data")

    def test_existing_unmarked_output_is_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            directory = repo / "target/experiments/linker"
            directory.mkdir(parents=True)
            keep = directory / "user-file"
            keep.write_text("user data")
            with self.assertRaises(ExperimentFailure):
                ExperimentOutput(repo, Path("target/experiments/linker"), "linker", {})
            self.assertTrue(keep.exists())

    def test_recorded_output_cannot_mix_runs_or_source_revisions(self):
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            output = ExperimentOutput(repo, Path("target/experiments/runner"), "runner", {})
            output.append_record({"source": "first-run", "revision": "abc"})
            with self.assertRaisesRegex(ExperimentFailure, "fresh namespace"):
                ExperimentOutput(repo, Path("target/experiments/runner"), "runner", {})
            self.assertIn("first-run", output.records_path.read_text(encoding="utf-8"))

    def test_run_timed_captures_logs_environment_exit_and_timeout(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            command = [sys.executable, "-c", "import os; print(os.environ['ANIMATIX_SAMPLE'])"]
            result = run_timed(
                "success", command, root, {"ANIMATIX_SAMPLE": "captured"}, root / "logs/success.log",
            )
            self.assertEqual(result["exit_code"], 0)
            self.assertGreaterEqual(result["elapsed_seconds"], 0)
            self.assertIn("captured", (root / "logs/success.log").read_text())
            self.assertIn("peak_tree_rss_bytes", result)

            timed_out = run_timed(
                "timeout", [sys.executable, "-c", "import time; time.sleep(30)"],
                root, {}, root / "logs/timeout.log", timeout_seconds=0.2,
            )
            self.assertTrue(timed_out["timed_out"])
            self.assertNotEqual(timed_out["exit_code"], 0)

    def test_source_fingerprint_tracks_working_tree_content(self):
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            subprocess.run(["git", "init", "-q", str(repo)], check=True)
            subprocess.run(["git", "-C", str(repo), "config", "user.email", "test@example.invalid"], check=True)
            subprocess.run(["git", "-C", str(repo), "config", "user.name", "Harness Test"], check=True)
            source = repo / "source.txt"
            source.write_text("initial")
            subprocess.run(["git", "-C", str(repo), "add", "source.txt"], check=True)
            subprocess.run(["git", "-C", str(repo), "commit", "-qm", "initial"], check=True)
            before = source_fingerprint(repo)
            source.write_text("modified")
            after = source_fingerprint(repo)
            self.assertNotEqual(before["workspace_sha256"], after["workspace_sha256"])


class LinkerEvidenceTests(unittest.TestCase):
    def test_linker_workloads_are_independent_and_keep_mac_package_profile(self):
        linux_cli = experiment_run._workload_command("cli", "linux")
        linux_gui = experiment_run._workload_command("gui", "linux")
        tests = experiment_run._workload_command("workspace-tests", "linux")
        self.assertIn("animatix-cli", linux_cli)
        self.assertIn("animatix-gui", linux_gui)
        self.assertIn("--workspace", tests)
        self.assertNotIn("--workspace", linux_cli)
        self.assertNotIn("--workspace", linux_gui)
        mac_cli = experiment_run._workload_command("cli", "macos")
        mac_gui = experiment_run._workload_command("gui", "macos")
        self.assertIn("dist", mac_cli)
        self.assertIn("dist", mac_gui)
        self.assertNotIn("--profile", experiment_run._workload_command("workspace-tests", "macos"))

    def test_linker_workload_selection_accepts_all_cli_and_gate_environment(self):
        self.assertEqual(experiment_run._selected_linker_workloads(),
                         ["cli", "gui", "workspace-tests"])
        self.assertEqual(experiment_run._selected_linker_workloads(
            environment_value="workspace-tests,cli"), ["cli", "workspace-tests"])
        self.assertEqual(experiment_run._selected_linker_workloads(
            ["gui"], "workspace-tests"), ["gui"])
        with self.assertRaises(ExperimentFailure):
            experiment_run._selected_linker_workloads(environment_value="cli,unknown")
        with self.assertRaises(ExperimentFailure):
            experiment_run._selected_linker_workloads(["cli", "cli"])

    def test_single_workload_job_builds_and_stages_both_compatibility_apps(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = ExperimentOutput(root, Path("target/experiments/linker"), "linker", {})
            arms = [{"id": "mold", "env": {"RUSTFLAGS": ""}}]
            targets = {"mold": {}}
            calls = []

            def fake_build(_output, _platform, _phase, arm, workload, _command,
                           target, _round, _timed):
                calls.append(workload)
                binary = target / "debug" / ("animatix" if workload == "cli" else "animatix-gui")
                binary.parent.mkdir(parents=True, exist_ok=True)
                binary.write_bytes(f"{workload} binary".encode())
                build_log = root / "logs" / f"{workload}.log"
                build_log.parent.mkdir(parents=True, exist_ok=True)
                build_log.write_text("compatibility build")
                return {
                    "executable_artifacts": [str(binary)],
                    "executable_sha256": [{"path": str(binary), "sha256": experiment_run._sha256(binary)}],
                    "audit_log_path": None,
                    "log_path": str(build_log),
                }

            failures = []
            with mock.patch.object(experiment_run, "_run_cargo_build", side_effect=fake_build):
                experiment_run._ensure_compatibility_targets(
                    output, "linux", arms, targets, failures,
                )
            self.assertEqual(calls, ["cli", "gui"])
            self.assertFalse(failures)
            self.assertEqual(set(targets["mold"]), {"cli", "gui"})
            for item in targets["mold"].values():
                staged = Path(item["binary_path"])
                self.assertTrue(staged.is_file())
                self.assertFalse(item["measured_build"])
                self.assertEqual(Path(item["link_output_path"]).name,
                                 Path(item["binary_path"]).name)
                self.assertEqual(item["sha256"], experiment_run._sha256(staged))
            self.assertFalse((output.directory / "compatibility-build-targets/linux/cli/mold/debug/animatix").exists())

    def test_cli_and_gui_compatibility_select_exact_profile_binary(self):
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary)
            cli = target / "debug" / "animatix"
            gui = target / "debug" / "animatix-gui"
            same_named_dep = target / "debug" / "deps" / "animatix"
            for path in (cli, gui, same_named_dep):
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("binary")
            self.assertEqual(
                experiment_run._select_workload_executables(
                    [same_named_dep, cli], target, "linux", "cli",
                ),
                [cli.resolve()],
            )
            self.assertEqual(
                experiment_run._select_workload_executables(
                    [same_named_dep, gui], target, "linux", "gui",
                ),
                [gui.resolve()],
            )
            mac_cli = target / "dist" / "animatix"
            mac_cli.parent.mkdir(parents=True)
            mac_cli.write_text("binary")
            self.assertEqual(
                experiment_run._select_workload_executables(
                    [cli, mac_cli], target, "macos", "cli",
                ),
                [mac_cli.resolve()],
            )

    def test_revision_requires_full_sha(self):
        self.assertEqual(linkers.validate_revision(linkers.MOLD_SOURCE_REV), linkers.MOLD_SOURCE_REV)
        with self.assertRaises(ExperimentFailure):
            linkers.validate_revision("main")
        with self.assertRaises(ExperimentFailure):
            linkers.validate_revision("1234")

    def test_ld64_alias_is_preserved_as_a_path_component(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            actual = root / "lld"
            actual.write_text("binary")
            alias = root / "ld64.lld"
            alias.symlink_to(actual)
            lexical = linkers._absolute_keep_alias(alias)
            self.assertEqual(lexical.name, "ld64.lld")
            self.assertNotEqual(lexical.name, lexical.resolve().name)

    def test_linux_arms_use_nix_mold_and_disable_default_lld_feature(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = ExperimentOutput(root, Path("target/experiments/linker"), "linker", {})
            tools = {}
            for name in ("clang", "ld.bfd", "mold"):
                path = root / name
                path.write_text("#!/bin/sh\necho test-tool\n")
                path.chmod(0o755)
                tools[name] = str(path)
            with mock.patch.object(linkers.shutil, "which", side_effect=lambda name: tools.get(name)):
                arms, inventory, failures = linkers.linker_arms("linux", output)
            self.assertEqual([arm["id"] for arm in arms], ["default", "system-bfd", "mold"])
            self.assertFalse(failures)
            self.assertEqual(inventory["nix_mold"]["path"], tools["mold"])
            for arm in arms[1:]:
                self.assertIn("-C linker-features=-lld", arm["env"]["RUSTFLAGS"])
                self.assertIsNone(arm["env"]["CARGO_ENCODED_RUSTFLAGS"])
            self.assertEqual(arms[1]["audit"]["callee_path"], tools["ld.bfd"])
            self.assertEqual(arms[2]["audit"]["callee_path"], tools["mold"])
            self.assertIn("--ld-path=", arms[2]["env"]["RUSTFLAGS"])

    def test_macos_arms_keep_ld64_names_and_create_argv0_alias(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = ExperimentOutput(root, Path("target/experiments/linker"), "linker", {})
            clang = root / "clang"
            clang.write_text("#!/bin/sh\necho clang\n")
            clang.chmod(0o755)
            apple_ld = root / "apple-ld"
            apple_ld.write_text("#!/bin/sh\necho apple-ld\n")
            apple_ld.chmod(0o755)
            lld_real = root / "lld"
            lld_real.write_text("#!/bin/sh\necho lld\n")
            lld_real.chmod(0o755)
            lld_alias = root / "ld64.lld"
            lld_alias.symlink_to(lld_real)
            mold = root / "mold"
            mold.write_text("#!/bin/sh\necho mold\n")
            mold.chmod(0o755)
            original_run = subprocess.run

            def fake_run(command, *args, **kwargs):
                if command[:2] == ["xcrun", "--find"]:
                    return subprocess.CompletedProcess(command, 0, str(apple_ld) + "\n", "")
                return original_run(command, *args, **kwargs)

            with mock.patch.object(linkers.shutil, "which", side_effect=lambda name: str(clang) if name == "clang" else None), \
                 mock.patch.object(linkers, "_find_macos_lld", return_value=linkers._absolute_keep_alias(lld_alias)), \
                 mock.patch.object(linkers.subprocess, "run", side_effect=fake_run):
                arms, inventory, failures = linkers.linker_arms("macos", output, mold)
            self.assertEqual([arm["id"] for arm in arms], ["apple", "ld64-lld", "ld64-mold"])
            self.assertFalse(failures)
            self.assertIn("--ld-path=", arms[1]["env"]["RUSTFLAGS"])
            self.assertIn("--ld-path=", arms[2]["env"]["RUSTFLAGS"])
            self.assertEqual(Path(inventory["ld64_lld"]["path"]).name, "ld64.lld")
            self.assertTrue((output.directory / "linkers/ld64.mold").is_symlink())
            self.assertEqual(arms[2]["audit"]["argv0"], "ld64.mold")
            self.assertEqual(arms[1]["audit"]["argv0"], "ld64.lld")

    def test_elf_linker_evidence_reads_artifact_comment(self):
        with tempfile.TemporaryDirectory() as temporary:
            artifact = Path(temporary) / "test-bin"
            artifact.write_text("fixture")
            completed = subprocess.CompletedProcess(
                ["readelf"], 0, "String dump of section '.comment':\n [ 0] LLD 21.1.8\n", "",
            )
            with mock.patch.object(linkers.shutil, "which", return_value="readelf"), \
                 mock.patch.object(linkers.subprocess, "run", return_value=completed):
                evidence = linkers.collect_elf_linker_evidence([artifact], "lld")
            self.assertEqual(evidence[0]["detected"], "lld")

    def test_gcc_comment_is_not_mistaken_for_gnu_linker_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            artifact = Path(temporary) / "test-bin"
            artifact.write_text("fixture")
            completed = subprocess.CompletedProcess(
                ["readelf"], 0, "String dump of section '.comment':\n [ 0] GCC: (GNU) 15.2.0\n", "",
            )
            with mock.patch.object(linkers.shutil, "which", return_value="readelf"), \
                 mock.patch.object(linkers.subprocess, "run", return_value=completed), \
                 self.assertRaises(ExperimentFailure):
                linkers.collect_elf_linker_evidence([artifact], "gnu")

    def test_wrapper_audit_checks_real_callee_hash_and_argv0(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            callee = root / "ld64.mold"
            callee.write_text("binary")
            expected_hash = linkers._file_sha256(callee)
            audit = root / "audit.jsonl"
            audit.write_text(
                '{"callee":"/tmp/ld64.mold","callee_sha256":"' + expected_hash +
                '","argv0":"ld64.mold","argv":[]}\n'
            )
            result = linkers.collect_wrapper_evidence(
                audit, {"callee_sha256": expected_hash, "callee_path": str(callee), "argv0": "ld64.mold"},
            )
            self.assertEqual(result[0]["argv0"], "ld64.mold")

    def test_wrapper_audit_requires_each_expected_artifact_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            callee = root / "ld.bfd"
            callee.write_text("#!/bin/sh\nexit 0\n")
            callee.chmod(0o755)
            wrapper = root / "wrapper"
            expected = linkers._make_audit_wrapper(wrapper, callee, "ld.bfd")
            output_a = root / "target/debug/deps/test-a"
            output_b = root / "target/debug/deps/test-b"
            audit = root / "audit.jsonl"
            env = {**os.environ, "ANIMATIX_LINKER_AUDIT_LOG": str(audit)}
            subprocess.run([sys.executable, str(wrapper), "-o", str(output_a)], env=env, check=True)
            with self.assertRaisesRegex(ExperimentFailure, "test-b"):
                linkers.collect_wrapper_evidence(audit, expected, [output_a, output_b])
            subprocess.run([sys.executable, str(wrapper), "-o", str(output_b)], env=env, check=True)
            evidence = linkers.collect_wrapper_evidence(audit, expected, [output_a, output_b])
            self.assertEqual({row["output_path"] for row in evidence}, {str(output_a), str(output_b)})

    def test_gnu_bfd_unknown_comment_is_accepted_only_with_exact_wrapper_proof(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            callee = root / "ld.bfd"
            callee.write_text("#!/bin/sh\nexit 0\n")
            callee.chmod(0o755)
            wrapper = root / "ld.bfd-audit"
            expected = linkers._make_audit_wrapper(wrapper, callee, "ld.bfd")
            artifact = root / "target/debug/deps/workspace-test"
            artifact.parent.mkdir(parents=True)
            artifact.write_text("ELF fixture")
            readelf_result = subprocess.CompletedProcess(
                ["readelf"], 0, "There is no .comment section in this file.\n", "",
            )
            with mock.patch.object(linkers.shutil, "which", return_value="readelf"), \
                 mock.patch.object(linkers.subprocess, "run", return_value=readelf_result):
                elf = linkers.collect_elf_linker_evidence([artifact], "gnu", allow_unknown=True)
            self.assertEqual(elf[0]["detected"], "unknown")

            audit_log = root / "audit.jsonl"
            env = {**os.environ, "ANIMATIX_LINKER_AUDIT_LOG": str(audit_log)}
            subprocess.run([sys.executable, str(wrapper), "-o", str(artifact)], env=env, check=True)
            proof = linkers.collect_wrapper_evidence(audit_log, expected, [artifact])
            self.assertEqual(proof[0]["output_path"], str(artifact))

            with self.assertRaisesRegex(ExperimentFailure, "did not link every expected artifact"):
                linkers.collect_wrapper_evidence(audit_log, expected, [root / "other-test"])


if __name__ == "__main__":
    unittest.main()
