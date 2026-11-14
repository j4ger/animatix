#!/usr/bin/env python3
"""Unit tests for CI job selection and fail-safe behavior."""

import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).with_name("ci-plan.py")
SPEC = importlib.util.spec_from_file_location("ci_plan", SCRIPT)
PLAN = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PLAN)

JOB_IDS = [
    "fast-checks", "doc-links", "eparts-linux", "eparts-macos",
    "eparts-windows", "check-lint", "test-examples", "render-scenes",
    "benchmarks", "web-check", "web-build",
]


def write_package(root, path, name, deps=None):
    directory = root / path
    directory.mkdir(parents=True, exist_ok=True)
    lines = ["[package]", f'name = "{name}"', 'version = "0.1.0"', 'edition = "2024"']
    if deps:
        lines.extend(["", "[dependencies]"])
        for dependency, target in deps.items():
            lines.append(f'{dependency} = {{ path = "../{target}" }}')
    (directory / "Cargo.toml").write_text("\n".join(lines) + "\n")


class PlannerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.repo = Path(self.temporary.name)
        members = [
            "crates/animatix-core", "crates/animatix", "crates/animatix-web",
            "crates/animatix-gui", "crates/eparts", "crates/renamed-consumer",
            "tools/animatix-web-build",
        ]
        manifest = ["[workspace]", "members = ["]
        manifest.extend(f'  "{member}",' for member in members)
        manifest.extend([
            "]", "", "[workspace.dependencies]",
            'animatix-core = { path = "crates/animatix-core" }',
            'core_alias = { package = "animatix-core", path = "crates/animatix-core" }',
        ])
        (self.repo / "Cargo.toml").write_text("\n".join(manifest) + "\n")
        write_package(self.repo, "crates/animatix-core", "animatix-core")
        write_package(self.repo, "crates/animatix", "animatix", {"animatix-core": "animatix-core"})
        write_package(self.repo, "crates/animatix-web", "animatix-web", {"animatix": "animatix"})
        write_package(self.repo, "crates/animatix-gui", "animatix-gui", {"animatix-core": "animatix-core"})
        write_package(self.repo, "crates/eparts", "eparts")
        write_package(self.repo, "tools/animatix-web-build", "animatix-web-build")
        renamed = self.repo / "crates/renamed-consumer"
        renamed.mkdir(parents=True)
        (renamed / "Cargo.toml").write_text(
            '[package]\nname = "renamed-consumer"\nversion = "0.1.0"\nedition = "2024"\n\n'
            '[dependencies]\ncore_alias = { workspace = true }\n'
        )
        self.roots, self.dependencies, self.reverse = PLAN.load_workspace(self.repo)
        self.jobs = [{"id": identifier, "name": identifier} for identifier in JOB_IDS]

    def tearDown(self):
        self.temporary.cleanup()

    def plan(self, paths, *, event="pull_request", base="base", enable=True, jobs=None):
        classification = PLAN.classify_paths(paths, self.roots, self.repo, self.reverse)
        all_jobs = jobs or self.jobs
        return classification, PLAN.make_plan_from_classification(
            classification, event, base, "head", all_jobs, enable
        )

    def test_workspace_paths_and_renamed_inherited_dependency_resolve_from_root(self):
        affected = PLAN.reverse_closure({"animatix-core"}, self.reverse)
        self.assertEqual(affected, {
            "animatix-core", "animatix", "animatix-web", "animatix-gui", "renamed-consumer",
        })
        self.assertIn("animatix-core", self.dependencies["renamed-consumer"])

    def test_build_dev_optional_and_target_dependencies_all_reach_consumers(self):
        manifest = self.repo / "crates/animatix-gui/Cargo.toml"
        for section, optional in (
            ("dependencies", True), ("dev-dependencies", False),
            ("build-dependencies", False),
            ("target.'cfg(windows)'.dependencies", False),
            ("target.'cfg(unix)'.build-dependencies", False),
            ("target.'cfg(windows)'.dev-dependencies", False),
        ):
            with self.subTest(section=section):
                write_package(self.repo, "crates/animatix-gui", "animatix-gui")
                with manifest.open("a") as stream:
                    extra = ", optional = true" if optional else ""
                    stream.write(f'\n[{section}]\nanimatix-core = {{ path = "../animatix-core"{extra} }}\n')
                _, _, reverse = PLAN.load_workspace(self.repo)
                self.assertIn("animatix-gui", PLAN.reverse_closure({"animatix-core"}, reverse))
                self.assertNotIn("animatix-core", PLAN.reverse_closure({"animatix-gui"}, reverse))

    def test_eparts_selects_gui_and_every_widget_platform_without_web(self):
        write_package(self.repo, "crates/animatix-gui", "animatix-gui", {"eparts": "eparts"})
        self.roots, _, self.reverse = PLAN.load_workspace(self.repo)
        _, plan = self.plan(["crates/eparts/src/lib.rs"])
        self.assertTrue({"eparts-linux", "eparts-macos", "eparts-windows", "test-examples"}
                        <= set(plan["candidate_job_ids"]))
        self.assertNotIn("web-build", plan["candidate_job_ids"])

    def test_git_diff_preserves_both_rename_paths_and_deleted_paths(self):
        def git(*args):
            return subprocess.check_output(["git", "-C", str(self.repo), *args], text=True).strip()
        git("init", "-q")
        git("config", "user.name", "Planner Test")
        git("config", "user.email", "planner@example.invalid")
        old = self.repo / "crates/animatix-gui/src/old.rs"
        old.parent.mkdir(parents=True)
        old.write_text("// fixture\n")
        deleted = self.repo / "docs/deleted.md"
        deleted.parent.mkdir()
        deleted.write_text("fixture\n")
        git("add", ".")
        git("commit", "-qm", "initial")
        base = git("rev-parse", "HEAD")
        new = self.repo / "crates/animatix-web/src/moved.rs"
        new.parent.mkdir(parents=True)
        old.rename(new)
        deleted.unlink()
        git("add", "-A")
        git("commit", "-qm", "rename and delete")
        self.assertEqual(set(PLAN.git_changes(self.repo, base, "HEAD")), {
            "crates/animatix-gui/src/old.rs", "crates/animatix-web/src/moved.rs", "docs/deleted.md",
        })

    def test_diff_failure_falls_back_to_the_complete_registry(self):
        with patch.object(PLAN, "git_changes", side_effect=PLAN.PlanError("diff failed")):
            plan = PLAN.make_plan(self.repo, "pull_request", "base", "head", self.jobs, True)
        self.assertTrue(plan["force_full"])
        self.assertEqual(plan["job_ids"], JOB_IDS)

    def test_gui_change_does_not_select_web_jobs(self):
        _, plan = self.plan(["crates/animatix-gui/src/app.rs"])
        selected = set(plan["candidate_job_ids"])
        self.assertEqual(selected, {"doc-links", "fast-checks", "check-lint", "test-examples"})
        self.assertNotIn("web-check", selected)
        self.assertNotIn("web-build", selected)
        self.assertNotIn("render-scenes", selected)

    def test_core_dependency_change_selects_reverse_closure_jobs(self):
        _, plan = self.plan(["crates/animatix-core/src/lib.rs"])
        selected = set(plan["candidate_job_ids"])
        self.assertTrue({"test-examples", "render-scenes", "web-check", "web-build"} <= selected)

    def test_web_content_selects_sync_tests_and_site_build_without_wasm_check(self):
        _, plan = self.plan(["web/index.html"])
        selected = set(plan["candidate_job_ids"])
        self.assertEqual(selected, {"doc-links", "fast-checks", "test-examples", "web-build"})
        self.assertNotIn("web-check", selected)

    def test_examples_select_content_tests_and_render_jobs(self):
        _, plan = self.plan(["examples/basics/scene.amx"])
        selected = set(plan["candidate_job_ids"])
        self.assertEqual(selected, {"doc-links", "fast-checks", "test-examples", "render-scenes"})
        self.assertNotIn("web-build", selected)

    def test_nested_web_build_fixture_has_its_own_web_build_owner(self):
        _, plan = self.plan(["tools/animatix-web-build/fixtures/split-probe/src/lib.rs"])
        selected = set(plan["candidate_job_ids"])
        self.assertEqual(selected, {
            "doc-links", "fast-checks", "check-lint", "test-examples", "web-build",
        })

    def test_web_scene_demo_and_font_assets_also_render_scenes(self):
        for path in ("web/scenes/intro.amx", "web/demos/example.amx", "web/fonts/index.json"):
            with self.subTest(path=path):
                _, plan = self.plan([path])
                selected = set(plan["candidate_job_ids"])
                self.assertIn("render-scenes", selected)
                self.assertIn("web-build", selected)
        _, html_plan = self.plan(["web/index.html"])
        self.assertNotIn("render-scenes", html_plan["candidate_job_ids"])

    def test_docs_only_runs_always_on_links_gate(self):
        classification, plan = self.plan(["docs/architecture.md"])
        self.assertFalse(classification["full_reason"])
        self.assertTrue(classification["docs_only"])
        self.assertEqual(plan["candidate_job_ids"], ["doc-links"])

    def test_property_reference_sources_add_generator_check(self):
        for path in ("docs/properties.md", "docs/property-reference-groups.json"):
            with self.subTest(path=path):
                _, plan = self.plan([path])
                self.assertEqual(set(plan["candidate_job_ids"]), {"doc-links", "check-lint"})

    def test_workspace_lock_nix_ci_and_package_manifests_force_full(self):
        for path in (
            "Cargo.toml", "Cargo.lock", "flake.nix", "flake.lock", "rust-toolchain.toml",
            "crates/animatix-gui/Cargo.toml", "tools/animatix-web-build/Cargo.toml",
            ".cargo/config.toml", ".github/workflows/ci.yml", "nix/shell.nix",
            "scripts/ci.sh", "scripts/ci-plan.py", "LICENSE", "LICENSE-MIT", "LICENSE-APACHE",
        ):
            with self.subTest(path=path):
                classification, plan = self.plan([path])
                self.assertTrue(classification["full_reason"])
                self.assertEqual(plan["candidate_job_ids"], JOB_IDS)

    def test_unknown_paths_and_unknown_registry_ids_force_full(self):
        for path in ("new-top-level-config.toml", "scripts/new-build-step.sh"):
            with self.subTest(path=path):
                _, plan = self.plan([path])
                self.assertEqual(plan["candidate_job_ids"], JOB_IDS)
        _, plan = self.plan(
            ["docs/architecture.md"], jobs=self.jobs + [{"id": "new-unclassified-job", "name": "new"}],
        )
        self.assertEqual(plan["candidate_job_ids"], JOB_IDS + ["new-unclassified-job"])
        self.assertTrue(plan["force_full"])

    def test_unknown_base_and_non_pr_events_force_full(self):
        for event, base in (
            ("push", "base"), ("workflow_call", "base"), ("pull_request", ""),
            ("pull_request", "0000000000000000000000000000000000000000"),
        ):
            with self.subTest(event=event, base=base):
                _, plan = self.plan(["docs/architecture.md"], event=event, base=base)
                self.assertEqual(plan["candidate_job_ids"], JOB_IDS)

    def test_shadow_mode_runs_full_candidate_remains_visible(self):
        classification = PLAN.classify_paths(["docs/architecture.md"], self.roots, self.repo)
        shadow = PLAN.make_plan_from_classification(
            classification, "pull_request", "base", "head", self.jobs, False
        )
        self.assertEqual(shadow["mode"], "shadow")
        self.assertEqual(shadow["job_ids"], JOB_IDS)
        self.assertEqual(shadow["candidate_job_ids"], ["doc-links"])

    def test_dot_paths_preserve_hidden_directories_and_reject_traversal(self):
        self.assertEqual(PLAN.normalize_repo_path("./.github/workflows/ci.yml"), ".github/workflows/ci.yml")
        self.assertEqual(PLAN.normalize_repo_path(".cargo/config.toml"), ".cargo/config.toml")
        for path in ("/tmp/a", "C:\\repo\\file", "../outside", "crates/../Cargo.lock", "foo\\..\\secret"):
            with self.subTest(path=path), self.assertRaises(PLAN.PlanError):
                PLAN.normalize_repo_path(path)
        for path in ("./.github/workflows/ci.yml", ".cargo/config.toml"):
            _, plan = self.plan([path])
            self.assertEqual(plan["candidate_job_ids"], JOB_IDS)

    def test_unknown_member_pattern_is_not_silently_ignored(self):
        manifest = self.repo / "Cargo.toml"
        manifest.write_text('[workspace]\nmembers = ["crates/*"]\n')
        with self.assertRaises(PLAN.PlanError):
            PLAN.load_workspace(self.repo)


if __name__ == "__main__":
    unittest.main()
