#!/usr/bin/env python3
"""Read-only proof that a main push starts from a fully validated CI baseline.

Uncertainty is a normal result: callers must use the full matrix unless trusted
is exactly true. Commit statuses and a successful pruned run are not proof.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import urllib.error
import urllib.parse
import urllib.request

WORKFLOW = ".github/workflows/ci.yml"
POLICY_PATHS = ("tools/ci", WORKFLOW)
MAX_PAGES = 20
MAX_ANCESTORS = 32
MAX_CANDIDATES = 5
MAX_RESPONSE = 8 * 1024 * 1024
SHA = re.compile(r"[0-9a-f]{40}\Z")
REPOSITORY = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+\Z")


class ProofError(ValueError):
    pass


def evidence(trusted=False, reason="unverified", **fields):
    return {"schema": 1, "trusted": trusted, "reason": reason, **fields}


def assess_proof(*, repository, before, head, event_name, event, checkout_ref,
                 ancestor, policy_equal, expected_jobs, run, jobs, workflow_id, jobs_attempt,
                 baseline=None):
    """Pure validation of already-fetched evidence. No external writes or IO."""
    if not isinstance(event, dict) or not isinstance(run, dict):
        return evidence(reason="malformed push/run evidence")
    anchor = before if baseline is None else baseline
    fields = {"sha": anchor, "before": before, "head": head, "run_id": run.get("id"),
              "run_attempt": run.get("run_attempt")}
    def reject(reason):
        return evidence(reason=reason, **fields)
    if event_name != "push" or checkout_ref:
        return reject("not an ordinary push; reusable/override validation requires full CI")
    if event.get("ref") != "refs/heads/main" or event.get("forced") is not False:
        return reject("not an explicitly non-forced main push")
    if not isinstance(before, str) or not isinstance(head, str) or not SHA.fullmatch(before) or not SHA.fullmatch(head) or before == "0" * 40:
        return reject("invalid before/head commit")
    if event.get("before") != before or event.get("after") != head:
        return reject("push event does not identify the requested commits")
    if not isinstance(event.get("repository"), dict) or event["repository"].get("full_name") != repository:
        return reject("push repository mismatch")
    if not ancestor:
        return reject("before commit is not an ancestor of head")
    if not policy_equal:
        return reject("CI policy differs from baseline")
    if not isinstance(expected_jobs, list) or not expected_jobs or any(not isinstance(x, str) or not x for x in expected_jobs):
        return reject("missing expected registry jobs")
    if len(expected_jobs) != len(set(expected_jobs)):
        return reject("ambiguous registry job names")
    if any(not isinstance(run.get(key), dict) or run[key].get("full_name") != repository
           for key in ("repository", "head_repository")):
        return reject("run repository mismatch")
    if type(workflow_id) is not int or run.get("path") != WORKFLOW or run.get("workflow_id") != workflow_id:
        return reject("run workflow mismatch")
    if run.get("head_sha") != anchor or run.get("head_branch") != "main" or run.get("event") != "push":
        return reject("run is not the exact push-main baseline")
    if run.get("status") != "completed" or run.get("conclusion") != "success":
        return reject("latest baseline run is not completed successfully")
    attempt = run.get("run_attempt")
    if type(attempt) is not int or attempt < 1 or type(run.get("id")) is not int:
        return reject("invalid run/attempt identity")
    if not isinstance(jobs, list) or not jobs or any(not isinstance(job, dict) for job in jobs):
        return reject("missing complete job evidence")
    ids = [job.get("id") for job in jobs]
    if any(type(ident) is not int for ident in ids) or len(ids) != len(set(ids)):
        return reject("duplicate or invalid job evidence")
    if jobs_attempt != attempt:
        return reject("jobs were not fetched from the current attempt endpoint")
    for job in jobs:
        # The attempt-specific endpoint is authoritative. GitHub does not
        # promise run_attempt on every job object; check it when present.
        if job.get("run_id") != run["id"] or job.get("run_attempt", attempt) != attempt or job.get("head_sha") != anchor:
            return reject("job belongs to another commit/run/attempt")
    for name in ["Plan", "CI Required", *expected_jobs]:
        matches = [job for job in jobs if job.get("name") == name]
        if len(matches) != 1 or matches[0].get("status") != "completed" or matches[0].get("conclusion") != "success":
            return reject(f"required baseline job missing, ambiguous or unsuccessful: {name}")
    return evidence(True, "exact baseline has complete successful registry coverage", **fields,
                    expected_jobs=["Plan", "CI Required", *expected_jobs])


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        raise ProofError("unexpected GitHub API redirect")


class GitHub:
    def __init__(self, token):
        if not token:
            raise ProofError("Actions read token unavailable")
        self.token = token
        self.opener = urllib.request.build_opener(NoRedirect)

    def get(self, path):
        if not path.startswith("/repos/"):
            raise ProofError("unexpected API path")
        request = urllib.request.Request("https://api.github.com" + path, headers={
            "Authorization": "Bearer " + self.token,
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "animatix-ci-baseline-proof",
        })
        with self.opener.open(request, timeout=15) as response:
            data = response.read(MAX_RESPONSE + 1)
        if len(data) > MAX_RESPONSE:
            raise ProofError("API response exceeds proof bound")
        result = json.loads(data)
        if not isinstance(result, dict):
            raise ProofError("invalid API response")
        return result


def fetch_all(get, path, key):
    """Fetch all pages, rejecting truncation, changing totals and duplicates."""
    result, seen, total = [], set(), None
    separator = "&" if "?" in path else "?"
    for page in range(1, MAX_PAGES + 1):
        data = get(f"{path}{separator}per_page=100&page={page}")
        if not isinstance(data, dict):
            raise ProofError("malformed API page")
        count, items = data.get("total_count"), data.get(key)
        if type(count) is not int or count < 0 or not isinstance(items, list) or len(items) > 100:
            raise ProofError("invalid paginated API evidence")
        if total is not None and count != total:
            raise ProofError("API evidence changed during pagination")
        total = count
        for item in items:
            if not isinstance(item, dict) or type(item.get("id")) is not int or item["id"] in seen:
                raise ProofError("duplicate or malformed paginated evidence")
            seen.add(item["id"])
            result.append(item)
        if len(result) == total:
            return result
        if len(result) > total or len(items) < 100:
            raise ProofError("incomplete paginated API evidence")
    raise ProofError("API pagination limit reached")


def git(repo, *args):
    return subprocess.check_output(["git", *args], cwd=repo, stderr=subprocess.DEVNULL, timeout=30)


def policy_identity(repo, ref):
    # ls-tree records content blobs, executable modes and paths. Comparing the
    # full policy subtree also detects new/deleted policy helpers and tests.
    tree = git(repo, "ls-tree", "-r", ref, "--", *POLICY_PATHS)
    if not tree or WORKFLOW.encode() not in tree or b"tools/ci/ci.sh" not in tree:
        raise ProofError("incomplete baseline policy tree")
    return hashlib.sha256(tree).hexdigest()


def prove(args, get=None):
    event = json.loads(Path(args.event_file).read_text())
    if not isinstance(event, dict):
        return evidence(reason="malformed push event")
    before = event.get("before", "")
    initial = dict(repository=args.repository, before=before, head=args.head,
                   event_name=args.event, event=event, checkout_ref=args.checkout_ref,
                   ancestor=False, policy_equal=False, expected_jobs=[], run={}, jobs=[], workflow_id=None, jobs_attempt=None)
    # Reject non-push contexts before making any API calls.
    early = assess_proof(**initial)
    if early["reason"] != "before commit is not an ancestor of head":
        return early
    if not REPOSITORY.fullmatch(args.repository):
        raise ProofError("invalid repository")
    if git(args.repo, "rev-parse", "HEAD").decode().strip() != args.head:
        raise ProofError("checkout HEAD does not match push head")
    git(args.repo, "diff", "--exit-code", "HEAD", "--", *POLICY_PATHS)
    git(args.repo, "merge-base", "--is-ancestor", before, args.head)
    current_policy = policy_identity(args.repo, args.head)
    if policy_identity(args.repo, before) != current_policy:
        return evidence(reason="CI policy differs from baseline", sha=before, head=args.head)
    matrix = json.loads(subprocess.check_output(
        ["bash", "tools/ci/ci.sh", "--matrix", "pr"], cwd=args.repo, timeout=30))
    expected = [job["name"] for job in matrix["include"]]
    get = get or GitHub(os.environ.get("GH_TOKEN", "")).get
    prefix = f"/repos/{args.repository}/actions"
    workflow = get(prefix + "/workflows/ci.yml")
    if workflow.get("path") != WORKFLOW or type(workflow.get("id")) is not int:
        raise ProofError("workflow identity mismatch")
    def verify(anchor):
        result = verify_candidate(get, prefix, anchor, workflow["id"], expected, initial)
        result["policy_digest"] = current_policy
        return result

    immediate = verify(before)
    if immediate["trusted"]:
        return immediate

    # A selectively green tip is not proof of the skipped jobs. Instead use a
    # fully green first-parent ancestor and revalidate the complete net diff
    # from that SHA. The branch listing is only a bounded discovery hint.
    ancestors = git(args.repo, "rev-list", "--first-parent", f"--max-count={MAX_ANCESTORS}", before).decode().splitlines()
    branch_query = urllib.parse.urlencode({"event": "push", "branch": "main", "per_page": 100, "page": 1})
    recent = get(prefix + "/workflows/ci.yml/runs?" + branch_query).get("workflow_runs")
    if not isinstance(recent, list) or len(recent) > 100:
        raise ProofError("malformed recent workflow runs")
    eligible = {item.get("head_sha") for item in recent
                if isinstance(item, dict) and isinstance(item.get("head_sha"), str)
                and item.get("status") == "completed" and item.get("conclusion") == "success"}
    candidates = [sha for sha in ancestors if sha != before and sha in eligible][:MAX_CANDIDATES]
    for anchor in candidates:
        if policy_identity(args.repo, anchor) != current_policy:
            continue
        result = verify(anchor)
        if result["trusted"]:
            return result
    return {**immediate, "reason": immediate["reason"] + "; no proven full ancestor in bounded search"}


def verify_candidate(get, prefix, anchor, workflow_id, expected, initial):
    query = urllib.parse.urlencode({"head_sha": anchor, "event": "push", "branch": "main"})
    runs_path = prefix + "/workflows/ci.yml/runs?" + query
    runs = fetch_all(get, runs_path, "workflow_runs")
    if not runs:
        return evidence(reason="no baseline workflow run", sha=anchor, before=initial["before"], head=initial["head"])
    # Never select an older green run for this SHA if a newer run/attempt
    # failed or is pending. A different ancestor may still be proven.
    latest = max(runs, key=lambda run: run["id"])
    run_path = prefix + f"/runs/{latest['id']}"
    run = get(run_path)
    if run.get("id") != latest["id"]:
        raise ProofError("run identity mismatch")
    attempt = run.get("run_attempt")
    if type(attempt) is not int or attempt < 1:
        raise ProofError("invalid run attempt")
    jobs = fetch_all(get, run_path + f"/attempts/{attempt}/jobs", "jobs")
    checked = get(run_path)
    if any(checked.get(key) != run.get(key) for key in ("id", "run_attempt", "status", "conclusion", "head_sha", "workflow_id", "path")):
        raise ProofError("run changed during proof")
    refreshed = fetch_all(get, runs_path, "workflow_runs")
    if not refreshed or max(item["id"] for item in refreshed) != run["id"]:
        raise ProofError("new baseline run appeared during proof")
    return assess_proof(**{**initial, "ancestor": True, "policy_equal": True,
                           "expected_jobs": expected, "workflow_id": workflow_id, "run": run,
                           "jobs": jobs, "jobs_attempt": attempt, "baseline": anchor})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--event-file", default=os.environ.get("GITHUB_EVENT_PATH", ""))
    parser.add_argument("--repository", default=os.environ.get("GITHUB_REPOSITORY", ""))
    parser.add_argument("--head", default=os.environ.get("GITHUB_SHA", ""))
    parser.add_argument("--event", default=os.environ.get("GITHUB_EVENT_NAME", ""))
    parser.add_argument("--checkout-ref", default=os.environ.get("ANIMATIX_CI_CHECKOUT_REF", ""))
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        result = prove(args)
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError, urllib.error.URLError) as error:
        # Do not print HTTP response bodies, request headers or token-bearing URLs.
        result = evidence(reason=f"baseline proof unavailable ({type(error).__name__}); use full CI")
    text = json.dumps(result, sort_keys=True, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text)
    print(text, end="")


if __name__ == "__main__":
    main()
