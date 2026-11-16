#!/usr/bin/env python3
"""Consume an exact successful CI SDK; this experiment ALWAYS exits 1."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import signal
import stat
import subprocess
import tempfile
import time
import urllib.request
import zipfile

REPOSITORY = "j4ger/animatix"
LIMIT = 600
RESERVE = 3 * 1024**3


class HTTPSOnly(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        if not newurl.startswith("https://"):
            raise RuntimeError("non-HTTPS redirect")
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def validate_run(run, artifact, commit, run_id):
    if not (run.get("id") == run_id and run.get("workflow_id") == 281504146
            and run.get("head_sha") == commit and run.get("head_branch") == "main"
            and run.get("event") == "push" and run.get("status") == "completed"
            and run.get("conclusion") == "success" and run.get("path") == ".github/workflows/ci.yml"
            and run.get("repository", {}).get("full_name") == REPOSITORY):
        raise RuntimeError("not a successful trusted same-SHA main CI run")
    origin = artifact.get("workflow_run", {})
    if not (artifact.get("name") == "web-runtime" and artifact.get("expired") is False
            and origin.get("id") == run_id and origin.get("head_sha") == commit
            and origin.get("head_repository_id") == origin.get("repository_id") == 1185977116
            and re.fullmatch(r"sha256:[0-9a-f]{64}", artifact.get("digest", ""))):
        raise RuntimeError("artifact provenance mismatch")
    return artifact["digest"].removeprefix("sha256:")


def offline_proof_digest(commit, run_id, artifact_id):
    # Controller independently checked this exact successful CI run, downloaded
    # its archive and verified GitHub's digest. Not a production trust mechanism.
    if (commit, run_id, artifact_id) != (
            "f42c2cf33d923875041f23c86498a7b2f2a45ae3", 43685448863, 14150447856):
        raise RuntimeError("offline proof is restricted to its reviewed fixture")
    return "0b187f151c29265f1f4a886bcc3046b7f2d742e6cbc1a0948620d94daac7254c"


def unpack(archive, target):
    with zipfile.ZipFile(archive) as z:
        entries = z.infolist()
        if len(entries) > 1000 or sum(x.file_size for x in entries) > 256 * 1024**2:
            raise RuntimeError("archive bounds exceeded")
        names = set()
        for x in entries:
            p = PurePosixPath(x.filename)
            mode = x.external_attr >> 16
            if (not x.filename or p.is_absolute() or ".." in p.parts or "\\" in x.filename
                    or x.filename in names or stat.S_ISLNK(mode)
                    or (stat.S_IFMT(mode) not in (0, stat.S_IFREG, stat.S_IFDIR))):
                raise RuntimeError("unsafe archive entry")
            names.add(x.filename)
        z.extractall(target)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--commit", required=True)
    parser.add_argument("--run", type=int, required=True)
    parser.add_argument("--artifact", type=int, required=True)
    parser.add_argument("--offline-proof", action="store_true", help="Only the controller-verified fixed experiment tuple")
    parser.add_argument("--site-manifest-sha", help="Proof-only trusted manifest SHA from the verified CI ZIP")
    args = parser.parse_args()
    if args.site_manifest_sha and not re.fullmatch(r"[0-9a-f]{64}", args.site_manifest_sha):
        raise ValueError("invalid manifest SHA")
    if not re.fullmatch(r"[0-9a-f]{40}", args.commit) or min(args.run, args.artifact) <= 0:
        raise ValueError("invalid identifiers")
    repo = Path.cwd()
    if subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip() != args.commit:
        raise RuntimeError("checkout SHA mismatch")
    if subprocess.check_output(["git", "status", "--porcelain"], text=True).strip():
        raise RuntimeError("dirty checkout")
    if shutil.disk_usage(repo).free < 4 * 1024**3:
        raise RuntimeError("insufficient initial space")
    work = Path(tempfile.mkdtemp(prefix="animatix-sdk-consumer-"))
    started = time.monotonic()
    original_env = dict(os.environ)
    # Resolve the platform version-manager shim before changing HOME. Children
    # use the actual selected Node bin directory, not the shim's private config.
    node = Path(subprocess.check_output(["node", "-p", "process.execPath"], text=True, timeout=15).strip()).resolve()
    if not node.is_absolute() or not node.is_file():
        raise RuntimeError("selected Node executable unavailable")
    if subprocess.check_output([str(node), "--version"], text=True, timeout=15).strip() != "v24.14.0":
        raise RuntimeError("expected platform NODE_VERSION=24.14.0")
    # Retain PATH for platform-selected Node, but never inherited secrets/overrides.
    clean = {"PATH": str(node.parent) + os.pathsep + os.environ.get("PATH", os.defpath), "HOME": str(work),
             "TMPDIR": str(work), "LANG": "C.UTF-8", "SITE_BASE": "/",
             "ASTRO_TELEMETRY_DISABLED": "1", "npm_config_cache": str(work / "npm")}
    def budget():
        if time.monotonic() - started > LIMIT or shutil.disk_usage(repo).free < RESERVE:
            raise RuntimeError("time/disk budget reached")
    def fetch(url, max_bytes):
        budget()
        request = urllib.request.Request(url, headers={"User-Agent": "animatix-sdk-experiment", "Accept": "application/vnd.github+json"})
        with urllib.request.build_opener(HTTPSOnly()).open(request, timeout=30) as response:
            if not response.url.startswith("https://"):
                raise RuntimeError("non-HTTPS redirect")
            data = response.read(max_bytes + 1)
        if len(data) > max_bytes:
            raise RuntimeError("download bounds exceeded")
        budget()
        return data
    def run(command):
        budget()
        child = subprocess.Popen(command, start_new_session=True, env=clean)
        try:
            while child.poll() is None:
                time.sleep(2)
                budget()
        except BaseException:
            os.killpg(child.pid, signal.SIGTERM)
            try:
                child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
            raise
        if child.returncode:
            raise RuntimeError("command failed: " + command[0])
    try:
        os.environ.clear()
        os.environ.update(clean)
        if args.offline_proof:
            if args.site_manifest_sha:
                raise RuntimeError("offline proof requires Release ZIP transport")
            expected = offline_proof_digest(args.commit, args.run, args.artifact)
            print("CONTROLLED_OFFLINE_FIXTURE_SELECTED_NOT_PRODUCTION_AUTH", flush=True)
        else:
            api = "https://api.github.com/repos/" + REPOSITORY
            ci = json.loads(fetch(f"{api}/actions/runs/{args.run}", 2*1024**2))
            artifact = json.loads(fetch(f"{api}/actions/artifacts/{args.artifact}", 1024**2))
            expected = validate_run(ci, artifact, args.commit, args.run)
            print("SAME_SHA_SUCCESSFUL_CI_PROVENANCE_PASS", flush=True)
        stage = work / "runtime"
        if args.site_manifest_sha:
            # Experimental transport only. Caller obtained this hash from the
            # GitHub CI ZIP whose platform digest was independently verified.
            base = "https://amx.j4ger.dev/runtime/"
            raw = fetch(base + "runtime-manifest.json", 1024**2)
            if hashlib.sha256(raw).hexdigest() != args.site_manifest_sha:
                raise RuntimeError("public manifest differs from pinned CI manifest")
            manifest = json.loads(raw)
            files = manifest["files"]
            if len(files) > 1000 or sum(x["size_bytes"] for x in files.values()) > 256*1024**2:
                raise RuntimeError("SDK bounds exceeded")
            stage.mkdir()
            (stage / "runtime-manifest.json").write_bytes(raw)
            for name, info in files.items():
                if not name or any(not re.fullmatch(r"[A-Za-z0-9_.-]+", part) or part in (".", "..") for part in name.split("/")):
                    raise RuntimeError("unsafe SDK filename")
                data = fetch(base + name, min(info["size_bytes"], 32*1024**2))
                if len(data) != info["size_bytes"] or hashlib.sha256(data).hexdigest() != info["sha256"]:
                    raise RuntimeError("public SDK file differs from CI")
                (stage / name).parent.mkdir(parents=True, exist_ok=True)
                (stage / name).write_bytes(data)
            print("PINNED_CI_MANIFEST_AND_ALL_PUBLIC_FILES_PASS", flush=True)
        else:
            url = f"https://github.com/{REPOSITORY}/releases/download/experiment-sdk-{args.commit}/web-runtime-{args.artifact}.zip"
            archive = work / "sdk.zip"
            data = fetch(url, 64*1024**2)
            if hashlib.sha256(data).hexdigest() != expected:
                raise RuntimeError("published ZIP differs from CI artifact digest")
            archive.write_bytes(data)
            print("CI_ARCHIVE_DIGEST_PASS", flush=True)
            unpack(archive, stage)
        run(["node", "web/tools/sdk.mjs", "verify", str(stage), "--release", "--current", "--commit", args.commit])
        target = repo / "web/runtime"
        if target.exists() or target.is_symlink():
            raise RuntimeError("refuse existing runtime")
        shutil.copytree(stage, target)
        run(["bash", "scripts/site-artifact.sh", "_site"])
        run(["python3", "scripts/cloudflare-site-check.py", "_site"])
        print("SAME_SHA_SDK_AND_ASTRO_PASS", flush=True)
    except Exception as error:
        print("SDK_CONSUMER_BLOCKED:", str(error), flush=True)
    finally:
        os.environ.clear()
        os.environ.update(original_env)
        print("elapsed_seconds=", round(time.monotonic()-started), "free_mib=", shutil.disk_usage(repo).free//1024**2, flush=True)
        print("Intentional experiment stop; no deployment", flush=True)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
