#!/usr/bin/env python3
"""Contract tests only: no cleanup, package installation or real CI execution."""
import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch

SPEC = importlib.util.spec_from_file_location("prep", Path(__file__).with_name("ci-prep-experiment.py"))
PREP = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PREP)
ROWS = [dict(id=job, name=job, os="ubuntu-latest", shell="web-ci" if job == "web-build" else "nix",
             stage="pr", gate="check lint" if job == "check-lint" else "test", setup="nix")
        for job in sorted(PREP.JOBS)]
ENV = {"ANIMATIX_PREP_JOB": "check-lint", "ANIMATIX_PREP_ARM": "b",
       "ANIMATIX_PREP_ROUND": "cold", "GITHUB_ACTIONS": "true",
       "RUNNER_ENVIRONMENT": "github-hosted", "RUNNER_OS": "Linux",
       "GITHUB_REPOSITORY": "j4ger/animatix", "GITHUB_REF": "refs/heads/ci-prep-experiment/20261116"}
CTX = {"job": "check-lint", "arm": "b", "round": "cold"}


class PreparationContracts(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "flake.lock").write_text("pinned lock")
        self.root_patch = patch.object(PREP, "ROOT", self.root)
        self.root_patch.start()
        self.addCleanup(self.root_patch.stop)
        self.env_patch = patch.dict(os.environ, ENV, clear=True)
        self.env_patch.start()
        self.addCleanup(self.env_patch.stop)

    def records(self):
        return [json.loads(p.read_text()) for p in (self.root / "target/ci/prep-experiment").glob("*.json")]

    def test_context_rejects_invalid_values(self):
        for key in ("ANIMATIX_PREP_JOB", "ANIMATIX_PREP_ARM", "ANIMATIX_PREP_ROUND"):
            with self.subTest(key=key), patch.dict(os.environ, {key: "secret\ninjection"}):
                with self.assertRaises(ValueError):
                    PREP.context()

    def test_pilot_and_critical_matrix(self):
        for scope, expected in (("pilot", 3), ("critical", 9)):
            with self.subTest(scope=scope), patch.dict(os.environ, {
                    "ANIMATIX_PREP_ACTION": "matrix", "ANIMATIX_PREP_SCOPE": scope}), \
                    patch.object(PREP, "matrix_rows", return_value=ROWS), contextlib.redirect_stdout(io.StringIO()) as out:
                self.assertEqual(PREP.main(), 0)
                result = json.loads(out.getvalue())["include"]
                self.assertEqual(len(result), expected)
                self.assertEqual({r["arm"] for r in result}, PREP.ARMS)
                for item in result:
                    row = next(row for row in ROWS if row["id"] == item["id"])
                    self.assertEqual({k: v for k, v in item.items() if k != "arm"}, row)

    def test_scope_rejects_unknown(self):
        with patch.dict(os.environ, {"ANIMATIX_PREP_SCOPE": "all"}):
            with self.assertRaises(ValueError):
                PREP.experiment_scope()

    def test_registered_matrix_uses_read_only_registry(self):
        with patch.object(PREP.subprocess, "check_output", return_value=json.dumps({"include": ROWS})) as call:
            self.assertEqual(PREP.matrix_rows(), ROWS)
            self.assertEqual(call.call_args.args[0], ["bash", "tools/ci/ci.sh", "--matrix", "pr"])

    def test_invalid_registry_rejected(self):
        malformed = [ROWS[:2], ROWS + [ROWS[0]],
                     [{**r, "stage": "release"} for r in ROWS],
                     [{**r, "shell": "native"} for r in ROWS],
                     [{**r, "gate": "check; publish"} for r in ROWS]]
        for rows in malformed:
            with self.subTest(rows=rows), patch.object(PREP.subprocess, "check_output", return_value=json.dumps({"include": rows})):
                with self.assertRaises(ValueError):
                    PREP.matrix_rows()

    def test_matrix_hint_mismatch_rejected(self):
        for key in ("GATE", "GATE_SHELL", "ANIMATIX_PREP_GATE", "ANIMATIX_PREP_SHELL"):
            with self.subTest(key=key), patch.object(PREP, "matrix_rows", return_value=ROWS), patch.dict(os.environ, {key: "wrong"}):
                with self.assertRaises(ValueError):
                    PREP.registered_job(CTX)

    def test_cleanup_guard_rejects_unsafe_context(self):
        for key, bad in (("GITHUB_REF", "refs/heads/main"), ("GITHUB_REF", "refs/pull/1/merge"),
                         ("GITHUB_REF", "refs/heads/ci-prep-experiment/"),
                         ("RUNNER_ENVIRONMENT", "self-hosted"), ("RUNNER_OS", "Windows"),
                         ("GITHUB_ACTIONS", "false"), ("GITHUB_REPOSITORY", "other/repo")):
            with self.subTest(key=key, bad=bad), patch.dict(os.environ, {key: bad}), \
                    patch.object(PREP.platform, "system", return_value="Linux"):
                with self.assertRaises(ValueError):
                    PREP.cleanup_guard()

    def test_cleanup_ref_does_not_accept_newline(self):
        with patch.dict(os.environ, {"GITHUB_REF": ENV["GITHUB_REF"] + "\nrefs/heads/main"}):
            with self.assertRaises(ValueError):
                PREP.cleanup_guard()

    def test_cleanup_high_space_skips_every_mutation(self):
        with patch.object(PREP, "cleanup_guard"), patch.object(PREP, "disk", return_value={"available_bytes": PREP.MIN_FREE}), \
                patch.object(PREP.subprocess, "run") as run:
            self.assertEqual(PREP.cleanup(CTX), 0)
            run.assert_not_called()
            self.assertFalse(self.records()[0]["selected"])

    def test_baseline_always_uses_exact_production_allowlist(self):
        with patch.object(PREP, "cleanup_guard"), patch.object(PREP, "disk", return_value={"available_bytes": PREP.MIN_FREE * 2}), \
                patch.object(PREP.subprocess, "run") as run:
            self.assertEqual(PREP.cleanup({**CTX, "arm": "a"}), 0)
            self.assertEqual(run.call_args_list[0].args[0], ["sudo", "rm", "-rf", "--", *PREP.CLEAN_PATHS])
            self.assertEqual(run.call_args_list[1].args[0], ["sudo", "apt-get", "clean"])
            self.assertTrue(all(c.kwargs["check"] for c in run.call_args_list))

    def test_low_space_cleans_and_requires_threshold(self):
        for after, status in ((PREP.MIN_FREE, 0), (PREP.MIN_FREE - 1, 1)):
            with self.subTest(after=after), patch.object(PREP, "cleanup_guard"), \
                    patch.object(PREP, "disk", side_effect=[{"available_bytes": 1}, {"available_bytes": after}]), \
                    patch.object(PREP.subprocess, "run") as run:
                self.assertEqual(PREP.cleanup(CTX), status)
                self.assertEqual(run.call_count, 2)

    def test_cleanup_command_failure_propagates(self):
        with patch.object(PREP, "cleanup_guard"), patch.object(PREP, "disk", return_value={"available_bytes": 1}), \
                patch.object(PREP.subprocess, "run", side_effect=subprocess.CalledProcessError(7, "mock")):
            with self.assertRaises(subprocess.CalledProcessError):
                PREP.cleanup(CTX)
            self.assertEqual(self.records()[0]["exit_code"], 1)

    def test_disk_failure_never_triggers_deletion(self):
        with patch.object(PREP, "cleanup_guard"), patch.object(PREP, "disk", side_effect=OSError()), \
                patch.object(PREP.subprocess, "run") as run:
            with self.assertRaises(OSError):
                PREP.cleanup(CTX)
            run.assert_not_called()

    def test_identity_tracks_actual_toolchain_and_lock(self):
        row = ROWS[0]
        def collect(rust=b"rustc 1", cargo=b"cargo 1"):
            with patch.object(PREP, "registered_job", return_value=row), \
                    patch.object(PREP.subprocess, "check_output", side_effect=[rust, cargo]) as call, \
                    contextlib.redirect_stdout(io.StringIO()) as out:
                self.assertEqual(PREP.identity(CTX), 0)
                self.assertEqual(call.call_args_list[0].args[0], PREP.nix_command(row, ["rustc", "-Vv"]))
                self.assertEqual(call.call_args_list[1].args[0], PREP.nix_command(row, ["cargo", "-Vv"]))
                return out.getvalue().strip()
        first = collect()
        self.assertEqual(first, collect())
        self.assertNotEqual(first, collect(rust=b"rustc 2"))
        self.assertNotEqual(first, collect(cargo=b"cargo 2"))
        (self.root / "flake.lock").write_text("new lock")
        self.assertNotEqual(first, collect())

    def test_identity_tracks_job_and_shell(self):
        values = []
        for job, shell in (("check-lint", "nix"), ("test-examples", "nix"), ("check-lint", "web-ci")):
            with patch.object(PREP, "registered_job", return_value={"shell": shell}), \
                    patch.object(PREP.subprocess, "check_output", return_value=b"same compiler"), \
                    contextlib.redirect_stdout(io.StringIO()) as out:
                PREP.identity({**CTX, "job": job})
                values.append(out.getvalue())
        self.assertEqual(len(set(values)), 3)

    def test_snapshot_records_no_environment(self):
        with patch.dict(os.environ, {"SECRET_TOKEN": "unpublishable", "ANIMATIX_PREP_LABEL": "after-cache"}):
            PREP.snapshot(CTX)
        evidence = self.records()[0]
        self.assertEqual(evidence["label"], "after-cache")
        self.assertNotIn("unpublishable", json.dumps(evidence))
        self.assertNotIn(str(self.root), json.dumps(evidence))
        self.assertEqual(set(evidence) - {"schema", "action", "recorded_at", *CTX},
                         {"label", "disk", "cpu_count", "system", "machine", "cache_hit"})

    def test_snapshot_invalid_label_rejected(self):
        with patch.dict(os.environ, {"ANIMATIX_PREP_LABEL": "secret"}):
            with self.assertRaises(ValueError):
                PREP.snapshot(CTX)

    def test_single_line_output(self):
        output = self.root / "output"
        with patch.dict(os.environ, {"GITHUB_OUTPUT": str(output)}):
            PREP.output_value("identity", "abc")
            with self.assertRaises(ValueError):
                PREP.output_value("identity", "abc\ndef=bad")
        self.assertEqual(output.read_text(), "identity=abc\n")

    def test_run_preserves_gate_sequence_and_exit(self):
        for code in (0, 7, -9):
            child = Mock(pid=123, wait=Mock(return_value=code))
            with self.subTest(code=code), patch.object(PREP, "registered_job", return_value=ROWS[0]), \
                    patch.object(PREP.subprocess, "Popen", return_value=child) as launch:
                self.assertEqual(PREP.run_gate(CTX), code if code >= 0 else 128 - code)
                self.assertEqual(launch.call_args.args[0], PREP.nix_command(ROWS[0], ["tools/ci/ci.sh", "gate", ROWS[0]["gate"]]))
                self.assertTrue(launch.call_args.kwargs["start_new_session"])

    def test_run_samples_low_water_mark(self):
        child = Mock(wait=Mock(side_effect=[subprocess.TimeoutExpired("mock", 5), 0]))
        with patch.object(PREP, "registered_job", return_value=ROWS[0]), \
                patch.object(PREP.subprocess, "Popen", return_value=child), \
                patch.object(PREP, "disk", side_effect=[{"available_bytes": 30}, {"available_bytes": 10}, {"available_bytes": 20}]):
            self.assertEqual(PREP.run_gate(CTX), 0)
        self.assertEqual(self.records()[0]["minimum_available_bytes"], 10)
        self.assertEqual(self.records()[0]["disk_samples"], 3)

    def test_run_disk_sampling_error_is_visible(self):
        child = Mock(wait=Mock(return_value=7))
        with patch.object(PREP, "registered_job", return_value=ROWS[0]), \
                patch.object(PREP.subprocess, "Popen", return_value=child), \
                patch.object(PREP, "disk", side_effect=OSError()):
            self.assertEqual(PREP.run_gate(CTX), 7)
        self.assertEqual(self.records()[0]["disk_sample_errors"], 2)
        self.assertIsNone(self.records()[0]["minimum_available_bytes"])

    def test_run_record_failure_does_not_mask_gate_status(self):
        child = Mock(wait=Mock(return_value=7))
        with patch.object(PREP, "registered_job", return_value=ROWS[0]), \
                patch.object(PREP.subprocess, "Popen", return_value=child), \
                patch.object(PREP, "record", side_effect=OSError()), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(PREP.run_gate(CTX), 7)

    def test_run_forwards_signal_and_restores_handler(self):
        original = signal.getsignal(signal.SIGTERM)
        child = Mock(pid=123)
        def interrupted(**_kwargs):
            signal.getsignal(signal.SIGTERM)(signal.SIGTERM, None)
            return 0
        child.wait.side_effect = interrupted
        with patch.object(PREP, "registered_job", return_value=ROWS[0]), \
                patch.object(PREP.subprocess, "Popen", return_value=child), patch.object(PREP.os, "killpg") as kill:
            self.assertEqual(PREP.run_gate(CTX), 143)
            kill.assert_called_once_with(123, signal.SIGTERM)
        self.assertIs(signal.getsignal(signal.SIGTERM), original)

    def test_preflight_budget_and_allowlisted_output(self):
        for size, expected in ((0, 0), (5 * 1024**3, 0), (5 * 1024**3 + 1, 1)):
            with self.subTest(size=size), patch.object(PREP, "cleanup_guard"), \
                    patch.object(PREP.subprocess, "check_output", return_value=json.dumps({
                        "total_active_caches_size_in_bytes": size, "total_active_caches_count": 2,
                        "secret": "do-not-print"})), contextlib.redirect_stdout(io.StringIO()) as out:
                self.assertEqual(PREP.preflight(), expected)
                self.assertNotIn("do-not-print", out.getvalue())

    def test_preflight_unknown_or_bad_usage_fails_closed(self):
        for value in ({}, {"total_active_caches_size_in_bytes": True, "total_active_caches_count": 2},
                      {"total_active_caches_size_in_bytes": -1, "total_active_caches_count": 2}):
            with self.subTest(value=value), patch.object(PREP, "cleanup_guard"), \
                    patch.object(PREP.subprocess, "check_output", return_value=json.dumps(value)):
                with self.assertRaises(ValueError):
                    PREP.preflight()


    def test_snapshot_cache_false_remains_false(self):
        with patch.dict(os.environ, {"ANIMATIX_PREP_CACHE_HIT": "false",
                                    "GITHUB_SHA": "a" * 40, "GITHUB_RUN_ID": "1234",
                                    "GITHUB_RUN_ATTEMPT": "2", "ImageOS": "ubuntu26",
                                    "ImageVersion": "20261111.196"}):
            PREP.snapshot(CTX)
        value = self.records()[0]
        self.assertEqual(value["cache_hit"], "false")
        self.assertEqual(value["run_attempt"], "2")
        self.assertEqual(value["sha"], "a" * 40)
        self.assertEqual(value["image_version"], "20261111.196")

    def test_snapshot_rejects_unknown_metadata(self):
        for key in ("GITHUB_SHA", "GITHUB_RUN_ID", "ImageOS", "ANIMATIX_PREP_CACHE_HIT"):
            with self.subTest(key=key), patch.dict(os.environ, {key: "do not upload secret"}):
                with self.assertRaises(ValueError):
                    PREP.snapshot(CTX)


class WorkflowIsolation(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root = Path(__file__).resolve().parents[2]
        cls.workflow = "\n".join(line for line in (cls.root / ".github/workflows/ci-prep-experiment.yml").read_text().splitlines()
                                 if not line.lstrip().startswith("#"))

    def block(self, name, next_name):
        return self.workflow.split("\n  " + name + ":", 1)[1].split("\n  " + next_name + ":", 1)[0]

    def test_no_production_workflow_edits(self):
        # Snapshot of the production workflow reviewed at 0dd37ec7. Any change
        # requires separate review; the pilot must not alter publication policy.
        content = (self.root / ".github/workflows/ci.yml").read_bytes()
        self.assertEqual(hashlib.sha256(content).hexdigest(), "05345b1dcf14108bc6f3da07cba76c7cc95c7d42309c9abf3b279a0a8c79cecf")

    def test_trigger_and_permissions_are_isolated(self):
        prefix = self.workflow.split("\njobs:", 1)[0]
        self.assertIn("branches: [ci-prep-experiment/20261116]", prefix)
        self.assertIn("permissions:\n  contents: read\n  actions: read", prefix)
        self.assertNotIn("pull_request:", prefix)
        self.assertNotIn("workflow_run:", prefix)
        self.assertNotIn("workflow_call:", prefix)
        self.assertNotIn(": write", self.workflow)
        self.assertNotIn("secrets.", self.workflow)
        self.assertNotIn("id-token:", self.workflow)
        self.assertNotIn("attestations:", self.workflow)
        self.assertEqual(self.workflow.count("permissions:"), 1)
        self.assertNotIn("uses: ./.github/workflows/ci.yml", self.workflow)
        self.assertNotIn("gate cloudflare", self.workflow)
        self.assertNotIn("git push", self.workflow)

    def test_cold_and_warm_share_cache_namespace_not_writes(self):
        cold = self.block("cold", "warm")
        warm = self.block("warm", "result")
        for text in (cold, warm):
            self.assertIn("matrix: ${{ fromJson(needs.plan.outputs.matrix) }}", text)
            self.assertIn("prefix-key: prep-v1-${{ github.run_id }}-${{ github.run_attempt }}", text)
            self.assertIn("shared-key: ${{ matrix.arm == 'c' && format('grouped-{0}', matrix.id) || 'baseline' }}", text)
            self.assertIn("key: ${{ matrix.arm == 'c' && steps.identity.outputs.identity || matrix.shell }}", text)
            self.assertIn("cache-workspace-crates: false", text)
            self.assertIn("cache-on-failure: false", text)
            self.assertIn("skipPush: true", text)
            self.assertNotIn("authToken:", text)
            self.assertIn("ANIMATIX_PREP_ACTION: run", text)
            self.assertIn("ANIMATIX_PREP_JOB: ${{ matrix.id }}", text)
        self.assertIn("save-if: true", cold)
        self.assertIn("save-if: false", warm)
        self.assertIn("needs: [plan, cold]", warm)

    def test_guarded_plan_and_fail_closed_result(self):
        plan = self.block("plan", "cold")
        self.assertIn("github.ref == 'refs/heads/ci-prep-experiment/20261116'", plan)
        self.assertIn("ANIMATIX_PREP_ACTION: preflight", plan)
        self.assertIn("ANIMATIX_PREP_ACTION: matrix", plan)
        self.assertIn("needs: [plan, cold, warm]", self.workflow)
        self.assertIn('test "$PLAN" = success && test "$COLD" = success && test "$WARM" = success', self.workflow)


if __name__ == "__main__":
    unittest.main()
