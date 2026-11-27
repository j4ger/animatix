#!/usr/bin/env python3
"""Proof is fail-closed: exact Actions attempts, not aggregate green statuses."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
from urllib.parse import parse_qs, urlsplit
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("baseline", Path(__file__).with_name("main-baseline.py"))
b = importlib.util.module_from_spec(spec)
spec.loader.exec_module(b)
BEFORE, HEAD = "a" * 40, "b" * 40
REPO = "j4ger/animatix"


def fixture():
    run = {"id": 200, "run_attempt": 2, "repository": {"full_name": REPO},
           "head_repository": {"full_name": REPO}, "path": b.WORKFLOW,
           "workflow_id": 10, "head_sha": BEFORE, "head_branch": "main", "event": "push",
           "status": "completed", "conclusion": "success"}
    jobs = [{"id": index, "run_id": 200, "run_attempt": 2, "head_sha": BEFORE,
             "name": name, "status": "completed", "conclusion": "success"}
            for index, name in enumerate(["Plan", "CI Required", "Fast Checks (ubuntu-latest)", "Web Build (ubuntu-latest)"], 1)]
    return dict(repository=REPO, before=BEFORE, head=HEAD, event_name="push",
                event={"before": BEFORE, "after": HEAD, "ref": "refs/heads/main", "forced": False,
                       "repository": {"full_name": REPO}}, checkout_ref="", ancestor=True,
                policy_equal=True, expected_jobs=[job["name"] for job in jobs[2:]],
                run=run, jobs=jobs, workflow_id=10, jobs_attempt=2)


class ProofTests(unittest.TestCase):
    def test_complete_latest_attempt_is_trusted(self):
        result = b.assess_proof(**fixture())
        self.assertTrue(result["trusted"])
        self.assertEqual(result["run_id"], 200)
        self.assertEqual(result["run_attempt"], 2)

    def test_attempt_endpoint_binds_jobs_when_api_omits_optional_attempt(self):
        args = fixture()
        for job in args["jobs"]: del job["run_attempt"]
        self.assertTrue(b.assess_proof(**args)["trusted"])
        args["jobs_attempt"] = 1
        self.assertFalse(b.assess_proof(**args)["trusted"])

    def test_malformed_event_run_or_job_evidence_falls_back(self):
        for field, value in (("event", []), ("run", []), ("jobs", [None]), ("before", [])):
            args = fixture(); args[field] = value
            with self.subTest(field=field):
                self.assertFalse(b.assess_proof(**args)["trusted"])
        for field in ("event", "run"):
            args = fixture(); args[field]["repository"] = None
            self.assertFalse(b.assess_proof(**args)["trusted"])
        args = fixture(); args["run"]["head_repository"] = None
        self.assertFalse(b.assess_proof(**args)["trusted"])

    def test_event_binding_force_override_and_ancestry(self):
        for field, value in [("event_name", "pull_request"), ("event_name", "workflow_call"),
                             ("checkout_ref", HEAD), ("ancestor", False), ("policy_equal", False),
                             ("before", "0" * 40), ("head", "not-a-sha")]:
            args = fixture(); args[field] = value
            with self.subTest(field=field, value=value):
                self.assertFalse(b.assess_proof(**args)["trusted"])
        for field, value in [("before", HEAD), ("after", BEFORE), ("forced", True),
                             ("forced", None), ("ref", "refs/heads/feature"), ("repository", {"full_name": "other/repo"})]:
            args = fixture(); args["event"][field] = value
            with self.subTest(field=field):
                self.assertFalse(b.assess_proof(**args)["trusted"])

    def test_run_mismatch_failure_cancel_and_pending_are_not_proof(self):
        for field, value in [("repository", {"full_name": "other/repo"}),
                             ("head_repository", {"full_name": "fork/repo"}),
                             ("path", ".github/workflows/other.yml"), ("workflow_id", 99),
                             ("head_sha", HEAD), ("head_branch", "other"), ("event", "pull_request"),
                             ("conclusion", "failure"), ("conclusion", "cancelled"),
                             ("status", "in_progress"), ("run_attempt", None)]:
            args = fixture(); args["run"][field] = value
            with self.subTest(field=field, value=value):
                self.assertFalse(b.assess_proof(**args)["trusted"])

    def test_pruned_missing_duplicate_or_unsuccessful_job_is_not_proof(self):
        for operation in ("missing", "duplicate", "skipped", "failure", "old-attempt", "other-run", "other-sha"):
            args = fixture()
            if operation == "missing": args["jobs"].pop()
            elif operation == "duplicate": args["jobs"].append(copy.deepcopy(args["jobs"][-1]))
            elif operation in ("skipped", "failure"): args["jobs"][-1]["conclusion"] = operation
            elif operation == "old-attempt": args["jobs"][-1]["run_attempt"] = 1
            elif operation == "other-run": args["jobs"][-1]["run_id"] = 201
            else: args["jobs"][-1]["head_sha"] = HEAD
            with self.subTest(operation=operation):
                self.assertFalse(b.assess_proof(**args)["trusted"])

    def test_paginated_evidence_is_complete(self):
        calls = []
        def get(path):
            calls.append(path)
            items = [{"id": i} for i in (range(100) if path.endswith("&page=1") else range(100, 137))]
            return {"total_count": 137, "jobs": items}
        jobs = b.fetch_all(get, "/repos/o/r/actions/runs/1/attempts/2/jobs", "jobs")
        self.assertEqual(len(jobs), 137)
        self.assertEqual(len(calls), 2)

    def test_incomplete_changing_duplicate_or_bounded_pages_fail(self):
        for kind in ("short", "changing", "duplicate", "bounded"):
            calls = []
            def get(path):
                page = len(calls); calls.append(path)
                if kind == "short": return {"total_count": 2, "jobs": [{"id": 1}]}
                count = 201 if kind == "changing" and page else 200
                if kind == "bounded": count = 100 * (b.MAX_PAGES + 1)
                start = 0 if kind == "duplicate" else page * 100
                return {"total_count": count, "jobs": [{"id": i} for i in range(start, start + 100)]}
            with self.subTest(kind=kind), self.assertRaises(b.ProofError):
                b.fetch_all(get, "/repos/o/r/actions/runs?branch=main", "jobs")

    def test_api_errors_never_become_success(self):
        with self.assertRaises(OSError):
            b.fetch_all(lambda path: (_ for _ in ()).throw(OSError("offline")), "/repos/o/r/jobs", "jobs")
        with self.assertRaises(b.ProofError): b.GitHub("")

    def test_unsafe_context_and_policy_change_never_fetch_actions(self):
        import tempfile
        with tempfile.TemporaryDirectory() as temp:
            event_path = Path(temp) / "event.json"
            event_path.write_text(json.dumps(fixture()["event"]))
            args = SimpleNamespace(repo=Path(temp), event_file=str(event_path), repository=REPO,
                                   head=HEAD, event="workflow_call", checkout_ref="")
            def forbidden(path): self.fail("unexpected API call: " + path)
            self.assertFalse(b.prove(args, forbidden)["trusted"])
            args.event = "push"
            with patch.object(b, "git", return_value=HEAD.encode()), patch.object(b, "policy_identity", side_effect=["new", "old"]):
                result = b.prove(args, forbidden)
                self.assertFalse(result["trusted"])
                self.assertIn("policy differs", result["reason"])

    def test_walks_first_parent_after_selective_or_pending_push(self):
        anchor, side = "c" * 40, "d" * 40
        f = fixture()
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "event.json"
            path.write_text(json.dumps(f["event"]))
            args = SimpleNamespace(repo=Path(temporary), event_file=str(path), repository=REPO,
                                   head=HEAD, event="push", checkout_ref="")

            def git(_repo, *cmd):
                if cmd[0] == "rev-parse":
                    return HEAD.encode()
                if cmd[0] == "rev-list":
                    self.assertIn("--first-parent", cmd)
                    self.assertIn(f"--max-count={b.MAX_ANCESTORS}", cmd)
                    return f"{BEFORE}\n{anchor}\n".encode()
                return b""

            for outcome in ("selective", "pending", "older-policy", "older-selective", "older-attempt"):
                checked = []
                def get(path):
                    parsed = urlsplit(path)
                    query = parse_qs(parsed.query)
                    if parsed.path.endswith("/workflows/ci.yml"):
                        return {"id": 10, "path": b.WORKFLOW}
                    if parsed.path.endswith("/workflows/ci.yml/runs"):
                        if "head_sha" not in query:
                            return {"workflow_runs": [
                                {"id": 300, "head_sha": side, "status": "completed", "conclusion": "success"},
                                {"id": 100, "head_sha": anchor, "status": "completed", "conclusion": "success"},
                            ]}
                        sha = query["head_sha"][0]
                        checked.append(sha)
                        ident = {BEFORE: 200, anchor: 100}[sha]
                        return {"total_count": 1, "workflow_runs": [{"id": ident}]}
                    if parsed.path.endswith("/runs/200"):
                        return {**f["run"], **({"status": "in_progress", "conclusion": None}
                                               if outcome == "pending" else {})}
                    if parsed.path.endswith("/runs/100"):
                        return {**f["run"], "id": 100, "head_sha": anchor}
                    if "/runs/200/attempts/2/jobs" in parsed.path:
                        jobs = f["jobs"][:-1]
                    elif "/runs/100/attempts/2/jobs" in parsed.path:
                        jobs = [{**job, "run_id": 100, "head_sha": anchor,
                                 "run_attempt": 1 if outcome == "older-attempt" else 2}
                                for job in f["jobs"]]
                        if outcome == "older-selective":
                            jobs.pop()
                    else:
                        self.fail(path)
                    return {"total_count": len(jobs), "jobs": jobs}

                def policy(_repo, sha):
                    return "old" if sha == anchor and outcome == "older-policy" else "current"

                with self.subTest(outcome=outcome), patch.object(b, "git", side_effect=git), \
                     patch.object(b, "policy_identity", side_effect=policy), \
                     patch.object(b.subprocess, "check_output", return_value=json.dumps(
                         {"include": [{"name": name} for name in f["expected_jobs"]]}).encode()):
                    proof = b.prove(args, get)
                    self.assertEqual(proof["trusted"], outcome in ("selective", "pending"))
                    self.assertNotIn(side, checked)
                    if outcome == "older-policy":
                        self.assertNotIn(anchor, checked)
                    if proof["trusted"]:
                        self.assertEqual((proof["sha"], proof["before"], proof["head"]), (anchor, BEFORE, HEAD))

    def test_fetches_latest_run_and_attempt_and_rechecks_race(self):
        import tempfile
        f = fixture()
        with tempfile.TemporaryDirectory() as temp:
            event_path = Path(temp) / "event.json"
            event_path.write_text(json.dumps(f["event"]))
            args = SimpleNamespace(repo=Path(temp), event_file=str(event_path), repository=REPO,
                                   head=HEAD, event="push", checkout_ref="")
            for outcome in ("success", "latest-failed", "attempt-race", "new-run", "wrong-head", "nonancestor"):
                calls, list_calls, detail_calls = [], 0, 0
                def get(path):
                    nonlocal list_calls, detail_calls
                    calls.append(path)
                    if path.endswith("/workflows/ci.yml"):
                        return {"id": 10, "path": b.WORKFLOW}
                    if "/runs?" in path:
                        list_calls += 1
                        newest = 201 if outcome == "new-run" and list_calls > 1 else 200
                        return {"total_count": 2, "workflow_runs": [{"id": 100}, {"id": newest}]}
                    if path.endswith("/runs/200"):
                        detail_calls += 1
                        run = copy.deepcopy(f["run"])
                        if outcome == "latest-failed": run["conclusion"] = "failure"
                        if outcome == "attempt-race" and detail_calls > 1: run["run_attempt"] = 3
                        return run
                    if "/attempts/2/jobs?" in path:
                        return {"total_count": len(f["jobs"]), "jobs": f["jobs"]}
                    self.fail(path)
                def git(repo, *cmd):
                    if cmd[0] == "rev-parse": return (HEAD if outcome != "wrong-head" else BEFORE).encode()
                    if cmd[0] == "merge-base" and outcome == "nonancestor": raise b.subprocess.CalledProcessError(1, cmd)
                    return b""
                with self.subTest(outcome=outcome), patch.object(b, "git", side_effect=git), patch.object(b, "policy_identity", return_value="same"), patch.object(b.subprocess, "check_output", return_value=json.dumps({"include": [{"name": x} for x in f["expected_jobs"]]}).encode()):
                    if outcome in ("attempt-race", "new-run", "wrong-head", "nonancestor"):
                        with self.assertRaises((b.ProofError, b.subprocess.CalledProcessError)): b.prove(args, get)
                    else:
                        self.assertEqual(b.prove(args, get)["trusted"], outcome == "success")
                    self.assertFalse(any("runs/100" in path for path in calls))


if __name__ == "__main__":
    unittest.main()
