"""Focused safety and evidence tests for the opt-in experiment harness."""

from __future__ import annotations

from pathlib import Path
import fcntl
import json
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
from scripts.experiments.coverage import parse_cargo_executed_ids, parse_doc_ids


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

    def test_run_timed_keeps_interleaved_diagnostics_out_of_libtest_stdout(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            executable = root / "fake-test-executable"
            executable.write_text("fixture")
            script = (
                "import sys\n"
                "sys.stderr.write(f' Running unittests src/lib.rs ({sys.argv[1]})\\n'); sys.stderr.flush()\n"
                "print('running 1 test', flush=True)\n"
                "sys.stderr.write('MESA: Opening /dev/dri/card0 failed\\n'); sys.stderr.flush()\n"
                "print('test renderer::gpu_case ... ok', flush=True)\n"
                "print('test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out', flush=True)\n"
            )
            run = run_timed(
                "synthetic-libtest", [sys.executable, "-c", script, str(executable)],
                root, {}, root / "stdout.log", stderr_path=root / "stderr.log",
            )
            self.assertEqual(run["exit_code"], 0)
            self.assertIn("running 1 test", Path(run["log_path"]).read_text())
            self.assertNotIn("MESA", Path(run["log_path"]).read_text())
            self.assertIn("MESA", Path(run["stderr_log_path"]).read_text())
            harnesses = [{
                "executable": str(executable),
                "cases": [{"name": "renderer::gpu_case", "id": "render::gpu_case"}],
            }]
            actual, unresolved = parse_cargo_executed_ids(
                Path(run["log_path"]), harnesses, Path(run["stderr_log_path"]),
            )
            self.assertEqual(actual, ["render::gpu_case"])
            self.assertEqual(unresolved, [])

    def test_machine_lock_is_shared_across_distinct_tmpdirs(self):
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            output = ExperimentOutput(repo, Path("target/experiments/runner"), "runner", {})
            lock_path = Path("/tmp") / f"animatix-experiment-machine-{os.getuid()}.lock"
            child_code = (
                "import fcntl, sys\n"
                "with open(sys.argv[1], 'a+') as lock_file:\n"
                "    try: fcntl.flock(lock_file.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)\n"
                "    except BlockingIOError: raise SystemExit(23)\n"
            )
            with tempfile.TemporaryDirectory() as first_tmp, tempfile.TemporaryDirectory() as second_tmp:
                self.assertNotEqual(first_tmp, second_tmp)
                child_env = {**os.environ, "TMPDIR": second_tmp}
                with mock.patch.dict(os.environ, {"TMPDIR": first_tmp}):
                    with output.machine_lock():
                        blocked = subprocess.run(
                            [sys.executable, "-c", child_code, str(lock_path)],
                            env=child_env, check=False,
                        )
                        self.assertEqual(blocked.returncode, 23)
                acquired = subprocess.run(
                    [sys.executable, "-c", child_code, str(lock_path)],
                    env=child_env, check=False,
                )
                self.assertEqual(acquired.returncode, 0)

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


class CargoStreamCoverageTests(unittest.TestCase):
    def test_split_streams_pair_duplicate_names_and_mixed_doctests_by_heading_order(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            unit_executable = root / "unit" / "test-bin"
            integration_executable = root / "integration" / "test-bin"
            unit_executable.parent.mkdir()
            integration_executable.parent.mkdir()
            stdout = root / "cargo.stdout"
            stderr = root / "cargo.stderr"
            unit_block = (
                "running 1 test\n"
                "test duplicate::same_name ... ok\n"
                "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
            )
            doc_block = (
                "running 1 test\n"
                "test src/lib.rs - sample_crate::Widget (line 4) ... ok\n"
                "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
            )
            ignored_doc_block = (
                "running 1 test\n"
                "test src/lib.rs - sample_crate::Gadget (line 9) ... ignored\n"
                "test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out\n"
            )
            valid_stdout = unit_block + unit_block + doc_block + ignored_doc_block
            stdout.write_text(valid_stdout)
            stderr.write_text(
                f"Running unittests src/lib.rs ({unit_executable})\n"
                "MESA: error: Opening /dev/dri/card0 failed: Permission denied\n"
                f"Running tests/integration.rs ({integration_executable})\n"
                "Doc-tests sample_crate\n"
                "MESA: error: renderer initialization unavailable\n"
            )
            harnesses = [
                {"executable": str(unit_executable), "cases": [
                    {"name": "duplicate::same_name", "id": "unit::duplicate::same_name"},
                ]},
                {"executable": str(integration_executable), "cases": [
                    {"name": "duplicate::same_name", "id": "integration::duplicate::same_name"},
                ]},
            ]

            actual, unresolved = parse_cargo_executed_ids(stdout, harnesses, stderr)
            self.assertEqual(actual, [
                "integration::duplicate::same_name", "unit::duplicate::same_name",
            ])
            self.assertEqual(unresolved, [])
            self.assertEqual(
                parse_doc_ids([{"log_path": str(stdout), "stderr_log_path": str(stderr)}]),
                ["src/lib.rs - sample_crate::Gadget (line 9)",
                 "src/lib.rs - sample_crate::Widget (line 4)"],
            )

            stdout.write_text(unit_block + doc_block + ignored_doc_block)
            with self.assertRaises(ExperimentFailure):
                parse_cargo_executed_ids(stdout, harnesses, stderr)

            stderr_without_doc_heading = root / "cargo-no-doc-heading.stderr"
            stderr_without_doc_heading.write_text(
                f"Running unittests src/lib.rs ({unit_executable})\n"
                f"Running tests/integration.rs ({integration_executable})\n"
            )
            stdout.write_text(valid_stdout)
            with self.assertRaisesRegex(ExperimentFailure, "without a Doc-tests heading"):
                parse_cargo_executed_ids(stdout, harnesses, stderr_without_doc_heading)

            stderr_bad_order = root / "cargo-bad-order.stderr"
            stderr_bad_order.write_text(
                f"Running unittests src/lib.rs ({unit_executable})\n"
                "Doc-tests sample_crate\n"
                f"Running tests/integration.rs ({integration_executable})\n"
            )
            with self.assertRaisesRegex(ExperimentFailure, "after the doc-test phase"):
                parse_cargo_executed_ids(stdout, harnesses, stderr_bad_order)

            direct_harness = {
                "package": "sample_crate", "binary": "unit-tests", "filter": None,
                "log_path": str(stdout), "stderr_log_path": str(stderr),
            }
            self.assertEqual(parse_doc_ids([direct_harness]), [])


class LinkerEvidenceTests(unittest.TestCase):
    def test_cargo_executable_artifacts_only_coalesce_identical_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / "target"
            binary = target / "debug/probe"
            binary.parent.mkdir(parents=True)
            common = {
                "reason": "compiler-artifact", "fresh": False,
                "package_id": "path+file:///probe#0.1.0",
                "target": {"name": "probe", "kind": ["bin"]},
                "profile": {"test": False}, "executable": str(binary),
            }
            log = root / "cargo.log"
            log.write_text(json.dumps(common) + "\n" + json.dumps(common) + "\n")
            self.assertEqual(len(experiment_run._cargo_executable_artifacts(log, target)), 1)
            conflicting = {**common, "package_id": "path+file:///other#0.1.0"}
            log.write_text(json.dumps(common) + "\n" + json.dumps(conflicting) + "\n")
            with self.assertRaisesRegex(ExperimentFailure, "conflicting executable artifact identity"):
                experiment_run._cargo_executable_artifacts(log, target)

    def test_linker_output_hashes_are_cached_once_per_unique_output_path(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / "target"
            artifacts = []
            invocations = []
            outputs = []
            for index in range(3):
                linker_output = target / f"debug/deps/probe-{index}"
                cargo_output = target / f"debug/probe-{index}"
                linker_output.parent.mkdir(parents=True, exist_ok=True)
                linker_output.write_bytes(f"binary {index}".encode())
                os.link(linker_output, cargo_output)
                outputs.append(linker_output.resolve())
                artifacts.append({"path": cargo_output.resolve(), "fresh": False})
                invocations.append({"output_path": str(linker_output), "driver": "cc"})
            original_sha256 = experiment_run._sha256
            with mock.patch.object(experiment_run, "_sha256", wraps=original_sha256) as sha256:
                experiment_run._map_linker_outputs(artifacts, invocations, target)
            for output in outputs:
                output_reads = [call for call in sha256.call_args_list
                                if Path(call.args[0]).resolve() == output]
                self.assertEqual(len(output_reads), 1, f"{output} was hashed more than once")

    def test_run_cargo_build_clears_stale_audit_before_launch(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = ExperimentOutput(root, Path("target/experiments/linker"), "linker", {})
            audit = output.logs_dir / "linux-cli-warmup-system-bfd.linker-audit.jsonl"
            audit.write_text('{"stale":true}\n')
            target = output.owned_target("build-targets/linux/cli/warm/system-bfd")

            def fake_run_timed(_name, _command, _cwd, env, cargo_log, **kwargs):
                self.assertFalse(audit.exists(), "stale wrapper records survived into the new run")
                self.assertEqual(env["ANIMATIX_LINKER_AUDIT_LOG"], str(audit))
                Path(cargo_log).write_text("")
                Path(kwargs["stderr_path"]).write_text("")
                return {
                    "name": "mock", "command": [], "cwd": str(root),
                    "log_path": str(cargo_log), "stderr_log_path": str(kwargs["stderr_path"]),
                    "elapsed_seconds": 0.0, "exit_code": 0, "timed_out": False,
                    "peak_tree_rss_bytes": 0, "error": None,
                }

            arm = {"id": "system-bfd", "env": {}, "audit": {
                "callee_path": "/bin/true", "callee_sha256": "0" * 64, "argv0": "true",
            }}
            with mock.patch.object(experiment_run, "run_timed", side_effect=fake_run_timed):
                with self.assertRaisesRegex(ExperimentFailure, "produced no executable artifacts"):
                    experiment_run._run_cargo_build(
                        output, "linux", "warmup", arm, "cli", ["cargo", "build"],
                        target, None, False,
                    )
            self.assertFalse(audit.exists())

    def test_rustc_link_log_records_real_output_without_changing_default_flags(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "target/debug/deps/probe-hash"
            output.parent.mkdir(parents=True)
            output.write_text("linked")
            log = root / "cargo.log"
            log.write_text(
                ' INFO rustc_codegen_ssa::back::link LC_ALL="C" PATH="/nix/bin" '
                f'"cc" "-fuse-ld=lld" "-o" "{output}"\n'
                ' INFO rustc_codegen_ssa::back::link reporting linker output: flavor=Gnu(Yes, Yes)\n'
            )
            rows = linkers.parse_rustc_link_invocations(log, root)
            evidence = linkers.collect_rustc_link_evidence(log, [output], root)
            self.assertEqual(len(rows), 1)
            self.assertEqual(rows[0]["output_path"], str(output))
            self.assertIn("-fuse-ld=lld", rows[0]["command"])
            self.assertEqual(evidence[0]["driver"], "cc")
            with self.assertRaisesRegex(ExperimentFailure, "exactly one invocation"):
                linkers.collect_rustc_link_evidence(log, [root / "missing"], root)

    def test_warm_relink_maps_and_removes_only_actual_output_and_cargo_hardlink(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / "target"
            deps = target / "debug/deps/probe-hash"
            cargo_output = target / "debug/probe"
            deps.parent.mkdir(parents=True)
            deps.write_bytes(b"tiny cargo executable")
            os.link(deps, cargo_output)
            unrelated = target / "debug/unrelated"
            unrelated.write_text("keep")
            log = root / "cargo.log"
            log.write_text(json.dumps({
                "reason": "compiler-artifact", "fresh": False,
                "package_id": "path+file:///probe#0.1.0",
                "target": {"name": "probe", "kind": ["bin"]},
                "profile": {"test": False}, "executable": str(cargo_output),
            }) + "\n")
            linker_log = root / "linker.log"
            linker_log.write_text(
                ' INFO rustc_codegen_ssa::back::link LC_ALL="C" "cc" "-o" '
                f'"{deps}"\n'
            )
            artifacts = experiment_run._cargo_executable_artifacts(log, target)
            experiment_run._require_fresh_relinks(artifacts, "cli", "default")
            invocations = linkers.parse_rustc_link_invocations(linker_log, root)
            mappings = experiment_run._map_linker_outputs(artifacts, invocations, target)
            self.assertEqual(mappings[0]["cargo_executable"], str(cargo_output))
            self.assertEqual(mappings[0]["linker_output"], str(deps))
            self.assertEqual(mappings[0]["proof"], "same-inode-and-sha256")

            invalidated = experiment_run._invalidate_warm_outputs(target, mappings)
            self.assertEqual({Path(row["path"]) for row in invalidated}, {deps, cargo_output})
            self.assertFalse(deps.exists())
            self.assertFalse(cargo_output.exists())
            self.assertEqual(unrelated.read_text(), "keep")

    def test_measured_artifacts_reject_cargo_fresh_cache_hits(self):
        with tempfile.TemporaryDirectory() as temporary:
            artifact = {"path": Path(temporary) / "already-built", "fresh": True}
            with self.assertRaisesRegex(ExperimentFailure, "fresh must be false"):
                experiment_run._require_fresh_relinks([artifact], "workspace-tests", "mold")

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
                linker_log = build_log.with_suffix(".linker.log")
                linker_log.write_text("rustc linker log")
                return {
                    "executable_artifacts": [str(binary)],
                    "executable_sha256": [{"path": str(binary), "sha256": experiment_run._sha256(binary)}],
                    "link_output_artifacts": [{
                        "cargo_executable": str(binary), "linker_output": str(binary),
                        "sha256": experiment_run._sha256(binary),
                    }],
                    "audit_log_path": None,
                    "log_path": str(build_log),
                    "linker_log_path": str(linker_log),
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
            audit.write_text(json.dumps({
                "callee": "/tmp/ld64.mold", "callee_sha256": expected_hash,
                "argv0": "ld64.mold", "argv": [], "expanded_argv": [],
                "response_files": [], "cwd": str(root), "output_path": None,
            }) + "\n")
            result = linkers.collect_wrapper_evidence(
                audit, {"callee_sha256": expected_hash, "callee_path": str(callee), "argv0": "ld64.mold"},
            )
            self.assertEqual(result[0]["argv0"], "ld64.mold")

    def test_wrapper_audit_expands_nested_response_files_and_executes_callee(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            callee = root / "ld.bfd"
            marker = root / "callee-argv.json"
            callee.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                "with open(os.environ['ANIMATIX_TEST_CALLEE_MARKER'], 'w', encoding='utf-8') as f:\n"
                "    json.dump(sys.argv[1:], f)\n"
            )
            callee.chmod(0o755)
            wrapper = root / "ld.bfd-audit"
            expected = linkers._make_audit_wrapper(wrapper, callee, "ld.bfd")
            nested = root / "nested.rsp"
            nested.write_text("-z relro\n")
            response = root / "outer args.rsp"
            response.write_text(f'-o "build output/final binary" @{nested.name}\n')
            audit = root / "audit.jsonl"
            original_argv = ["@" + str(response)]
            env = {
                **os.environ,
                "ANIMATIX_LINKER_AUDIT_LOG": str(audit),
                "ANIMATIX_TEST_CALLEE_MARKER": str(marker),
            }
            subprocess.run(
                [sys.executable, str(wrapper), *original_argv], cwd=root, env=env, check=True,
            )

            output = root / "build output" / "final binary"
            evidence = linkers.collect_wrapper_evidence(audit, expected, [output])
            record = evidence[0]
            self.assertEqual(record["argv"], original_argv)
            self.assertEqual(record["expanded_argv"], ["-o", str(output.relative_to(root)), "-z", "relro"])
            self.assertEqual(record["output_path"], str(output))
            self.assertEqual([item["path"] for item in record["response_files"]],
                             [str(response), str(nested)])
            self.assertEqual(json.loads(marker.read_text()), original_argv)

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
