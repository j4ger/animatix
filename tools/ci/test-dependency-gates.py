#!/usr/bin/env python3
"""Unit tests for the staged dependency graph checks."""

import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).with_name("dependency-gates.py")
SPEC = importlib.util.spec_from_file_location("dependency_gates", SCRIPT)
GATES = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = GATES
SPEC.loader.exec_module(GATES)


def make_repo(lock_names):
    temporary = tempfile.TemporaryDirectory(prefix="animatix-dependency-gates-")
    root = Path(temporary.name)
    packages = lock_names.items() if isinstance(lock_names, dict) else ((name, "0.1.0") for name in lock_names)
    lock = "version = 4\n\n" + "\n\n".join(
        f'[[package]]\nname = "{name}"\nversion = "{version}"'
        for name, version in packages
    ) + "\n"
    (root / "Cargo.lock").write_text(lock, encoding="utf-8")
    return temporary, root


def tree(*packages):
    rows = []
    for item in packages:
        name, version = item if isinstance(item, tuple) else (item, "0.1.0")
        rows.append(f"{name} v{version}\t")
    return "\n".join(rows) + "\n"


def make_renderer_repo(extra_parent=False):
    temporary, repo = make_repo(["animatix-web"])
    lockfile = repo / "Cargo.lock"
    with lockfile.open("a", encoding="utf-8") as lock:
        if extra_parent:
            lock.write('dependencies = ["vello_cpu"]\n')
        for name, version, dependencies in [
            ("epaint", "0.37.0", '["vello_cpu"]'),
            ("vello_cpu", "0.2.0", '["vello_common", "glifo"]'),
            ("glifo", "0.3.0", '["vello_common"]'),
            ("vello_common", "0.2.0", "[]"),
        ]:
            lock.write(f'\n[[package]]\nname = "{name}"\nversion = "{version}"\n')
            lock.write(f'source = "{GATES.CRATES_IO}"\ndependencies = {dependencies}\n')
    return temporary, repo


class DependencyGateTests(unittest.TestCase):
    def test_minimal_engine_is_scoped_to_normal_engine_graph_not_workspace_lock(self):
        temporary, repo = make_repo(["animatix", "animatix-core", "vello", "vello_hybrid"])
        self.addCleanup(temporary.cleanup)
        command = []

        def run(args, **kwargs):
            command.extend(args)
            return subprocess.CompletedProcess(args, 0, tree("animatix", "animatix-core"), "")

        with patch.object(GATES.subprocess, "run", side_effect=run):
            result = GATES.run_stage("minimal-engine", repo, cargo="cargo-test-double")
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["graph_findings"], [])
        self.assertEqual(result["lockfile_findings"], [])
        self.assertIn("--no-default-features", command)
        self.assertIn("--edges", command)
        self.assertEqual(command[command.index("--edges") + 1], "normal")
        self.assertEqual(command[command.index("--target") + 1], "all")
        self.assertIn("--locked", command)
        self.assertEqual(command[command.index("--package") + 1], "animatix")

    def test_minimal_target_override_does_not_change_full_stage_default(self):
        temporary, repo = make_repo(["animatix", "animatix-core"])
        self.addCleanup(temporary.cleanup)

        def run(args, **kwargs):
            return subprocess.CompletedProcess(args, 0, tree("animatix", "animatix-core"), "")

        with patch.object(GATES.subprocess, "run", side_effect=run):
            minimal = GATES.run_stage(
                "minimal-engine", repo, cargo="cargo-test-double",
                minimal_target="x86_64-unknown-linux-gnu",
            )
            full = GATES.run_stage("full-no-legacy", repo, cargo="cargo-test-double")
        self.assertEqual(
            minimal["command"][minimal["command"].index("--target") + 1],
            "x86_64-unknown-linux-gnu",
        )
        self.assertIn("target x86_64-unknown-linux-gnu", minimal["scope"])
        self.assertEqual(full["command"][full["command"].index("--target") + 1], "all")

    def test_wasm_graph_allows_vello_gpu_but_rejects_legacy_only_when_reachable(self):
        temporary, repo = make_repo(["animatix-web", "vello", "vello_gpu", "vello_hybrid"])
        self.addCleanup(temporary.cleanup)

        def run(args, **kwargs):
            return subprocess.CompletedProcess(
                args, 0, tree("animatix-web", ("vello_gpu", "0.4.0"), "wgpu"), "",
            )

        with patch.object(GATES.subprocess, "run", side_effect=run):
            result = GATES.run_stage("wasm-no-legacy", repo, cargo="cargo-test-double")
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["graph_findings"], [])
        self.assertEqual(result["lockfile_findings"], [])
        command = result["command"]
        self.assertEqual(command[command.index("--target") + 1], "wasm32-unknown-unknown")
        self.assertEqual(command[command.index("--edges") + 1], "normal,build")

    def test_full_stage_checks_graph_and_entire_lockfile(self):
        temporary, repo = make_repo(["animatix", "animatix-core", "vello"])
        self.addCleanup(temporary.cleanup)

        def run(args, **kwargs):
            return subprocess.CompletedProcess(args, 0, tree("animatix", "animatix-core"), "")

        with patch.object(GATES.subprocess, "run", side_effect=run):
            result = GATES.run_stage("full-no-legacy", repo, cargo="cargo-test-double")
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["graph_findings"], [])
        self.assertEqual(result["lockfile_findings"], ["vello@0.1.0"])
        self.assertTrue(result["command"].count("--workspace"))
        self.assertIn("--all-features", result["command"])
        self.assertEqual(
            result["command"][result["command"].index("--edges") + 1], "normal,build,dev",
        )
        self.assertEqual(result["command"][result["command"].index("--target") + 1], "all")

    def test_full_stage_detects_git_hybrid_dependency_in_graph(self):
        temporary, repo = make_repo(["animatix", "animatix-core"])
        self.addCleanup(temporary.cleanup)

        def run(args, **kwargs):
            return subprocess.CompletedProcess(
                args, 0, tree("animatix", "vello_hybrid"), "",
            )

        with patch.object(GATES.subprocess, "run", side_effect=run):
            result = GATES.run_stage("full-no-legacy", repo, cargo="cargo-test-double")
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["graph_findings"], ["vello_hybrid@0.1.0"])

    def test_full_stage_allows_only_the_upstream_vello_gpu_04_stack(self):
        temporary, repo = make_repo({
            "animatix": "0.1.0",
            "animatix-core": "0.1.0",
            "vello_gpu": "0.4.0",
            "vello_gpu_shaders": "0.4.0",
            "vello_common": "0.4.0",
        })
        self.addCleanup(temporary.cleanup)

        def run(args, **kwargs):
            return subprocess.CompletedProcess(
                args, 0,
                tree("animatix", "animatix-core", ("vello_gpu", "0.4.0"),
                     ("vello_gpu_shaders", "0.4.0"), ("vello_common", "0.4.0")),
                "",
            )

        with patch.object(GATES.subprocess, "run", side_effect=run):
            result = GATES.run_stage("full-no-legacy", repo, cargo="cargo-test-double")
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["findings"], [])

    def test_old_vello_common_versions_are_not_confused_with_upstream_04(self):
        temporary, repo = make_repo(["animatix-web", "vello_gpu", "vello_common"])
        self.addCleanup(temporary.cleanup)

        def run(args, **kwargs):
            return subprocess.CompletedProcess(
                args, 0, tree("animatix-web", ("vello_gpu", "0.4.0"), "vello_common"), "",
            )

        with patch.object(GATES.subprocess, "run", side_effect=run):
            result = GATES.run_stage("wasm-no-legacy", repo, cargo="cargo-test-double")
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["graph_findings"], ["vello_common@0.1.0"])

    def test_workspace_renderer_approves_current_cpu_paths_without_changing_strict_audit(self):
        temporary, repo = make_renderer_repo()
        self.addCleanup(temporary.cleanup)
        rows = tree("animatix-web", ("vello_gpu", "0.4.0"), ("vello_cpu", "0.2.0"))
        result = subprocess.CompletedProcess(["cargo"], 0, rows, "")
        with patch.object(GATES.subprocess, "run", return_value=result):
            approved = GATES.run_stage("workspace-renderer", repo)
            strict = GATES.run_stage("full-no-legacy", repo)
        self.assertEqual(approved["status"], "passed")
        self.assertEqual(strict["status"], "failed")
        self.assertEqual(strict["graph_findings"], ["vello_cpu@0.2.0"])
        self.assertEqual(approved["reviewed_cpu_packages"], strict["lockfile_findings"])

    def test_reviewed_cpu_version_cannot_gain_a_first_party_parent(self):
        temporary, repo = make_renderer_repo(extra_parent=True)
        self.addCleanup(temporary.cleanup)
        rows = tree("animatix-web", ("vello_cpu", "0.2.0"))
        result = subprocess.CompletedProcess(["cargo"], 0, rows, "")
        with patch.object(GATES.subprocess, "run", return_value=result):
            graph = GATES.run_stage("workspace-renderer", repo)
        self.assertEqual(graph["status"], "failed")
        self.assertEqual(graph["findings"], ["vello_cpu@0.2.0"])

    def test_workspace_renderer_rejects_old_cpu_package_even_if_only_in_lockfile(self):
        temporary, repo = make_repo({"animatix-web": "0.1.0", "vello_common": "0.0.9"})
        self.addCleanup(temporary.cleanup)
        result = subprocess.CompletedProcess(["cargo"], 0, tree("animatix-web"), "")
        with patch.object(GATES.subprocess, "run", return_value=result):
            graph = GATES.run_stage("workspace-renderer", repo)
        self.assertEqual(graph["status"], "failed")
        self.assertEqual(graph["graph_findings"], [])
        self.assertEqual(graph["lockfile_findings"], ["vello_common@0.0.9"])

    def test_wasm_renderer_requires_both_backends_and_excludes_native_features(self):
        temporary, repo = make_renderer_repo()
        self.addCleanup(temporary.cleanup)
        rows = tree("animatix-web", ("vello_gpu", "0.4.0"))
        for features, findings in [
            ("webgpu,webgl", []),
            ("vulkan,webgpu", ["Wasm wgpu enables native backend: vulkan", "Wasm wgpu feature missing: webgl"]),
        ]:
            with self.subTest(features=features):
                output = rows + f"wgpu v30.0.1\t{features} (*)\n"
                result = subprocess.CompletedProcess(["cargo"], 0, output, "")
                with patch.object(GATES.subprocess, "run", return_value=result):
                    graph = GATES.run_stage("wasm-renderer", repo)
                self.assertEqual(graph["findings"], findings)

    def test_wasm_renderer_rejects_cpu_packages_even_with_both_browser_backends(self):
        temporary, repo = make_renderer_repo()
        self.addCleanup(temporary.cleanup)
        rows = tree("animatix-web", ("vello_gpu", "0.4.0"),
                    ("vello_cpu", "0.0.8"), ("vello_cpu", "0.2.0"))
        result = subprocess.CompletedProcess(["cargo"], 0, rows + "wgpu v30.0.1\twebgpu,webgl\n", "")
        with patch.object(GATES.subprocess, "run", return_value=result):
            graph = GATES.run_stage("wasm-renderer", repo)
        self.assertEqual(graph["status"], "failed")
        self.assertEqual(graph["findings"], ["vello_cpu@0.0.8", "vello_cpu@0.2.0"])

    def test_locked_graph_must_leave_lockfile_byte_identical(self):
        temporary, repo = make_repo(["animatix", "animatix-core"])
        self.addCleanup(temporary.cleanup)

        def run(args, **kwargs):
            (repo / "Cargo.lock").write_text("rewritten\n", encoding="utf-8")
            return subprocess.CompletedProcess(args, 0, tree("animatix", "animatix-core"), "")

        with patch.object(GATES.subprocess, "run", side_effect=run):
            result = GATES.run_stage("minimal-engine", repo, cargo="cargo-test-double")
        self.assertEqual(result["status"], "failed")
        self.assertFalse(result["lockfile_unchanged"])
        self.assertIn("Cargo.lock changed despite --locked", result["findings"])

    def test_tree_output_requires_the_machine_delimiter(self):
        with self.assertRaises(GATES.DependencyGateError):
            GATES.parse_tree_output("animatix v0.1.0\n")

    def test_cargo_failure_is_not_misreported_as_a_clean_graph(self):
        temporary, repo = make_repo(["animatix"])
        self.addCleanup(temporary.cleanup)
        failure = subprocess.CompletedProcess(["cargo"], 101, "", "lockfile changed")
        with patch.object(GATES.subprocess, "run", return_value=failure):
            with self.assertRaisesRegex(GATES.DependencyGateError, "cargo tree failed"):
                GATES.run_stage("minimal-engine", repo)


if __name__ == "__main__":
    unittest.main()
