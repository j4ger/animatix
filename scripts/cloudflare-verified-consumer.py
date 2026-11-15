#!/usr/bin/env python3
"""Offline-signed, experiment-only SDK consumer. Never deploys; always exits 1."""
import hashlib
import importlib.util
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time
import urllib.request


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def interrupted(signum, frame):
    raise RuntimeError("consumer interrupted")


def main(policy=None, candidate=False):
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    policy = policy or load('cloudflare-permit')
    transport = load('cloudflare-sdk-consumer')
    bounded = load('cloudflare-nix-bootstrap')
    bounded.TIME_LIMIT = 600
    repo = Path.cwd()
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    policy.sha(commit)
    if subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip():
        raise RuntimeError('dirty source checkout')
    node = Path(subprocess.check_output(['node', '-p', 'process.execPath'], text=True, timeout=15).strip()).resolve()
    if subprocess.check_output([str(node), '--version'], text=True, timeout=15).strip() != 'v24.14.0':
        raise RuntimeError('NODE_VERSION must be 24.14.0')
    work = Path(tempfile.mkdtemp(prefix='animatix-signed-sdk-'))
    if shutil.disk_usage(work).free < 4*1024**3:
        raise RuntimeError('insufficient initial space')
    started = time.monotonic()
    old = dict(os.environ)
    clean = dict(PATH=str(node.parent)+os.pathsep+old.get('PATH', os.defpath), HOME=str(work),
                 TMPDIR=str(work), XDG_CACHE_HOME=str(work/'cache'), GH_CONFIG_DIR=str(work/'gh-config'),
                 LANG='C.UTF-8', SITE_BASE='/', ASTRO_TELEMETRY_DISABLED='1', npm_config_cache=str(work/'npm'))
    def latest():
        ref = subprocess.check_output(['git', 'ls-remote', 'https://github.com/j4ger/animatix.git', policy.REF], text=True, timeout=25).split()
        if len(ref) != 2:
            raise RuntimeError('cannot determine current experiment head')
        policy.accept_result(ref[0], commit)
        return ref[0]
    def fetch(name, limit):
        if time.monotonic()-started > 600 or shutil.disk_usage(work).free < 3*1024**3:
            raise RuntimeError('time/disk budget reached')
        prefix = getattr(policy, 'RELEASE_PREFIX', 'cf-candidate')
        url = f'https://github.com/j4ger/animatix/releases/download/{prefix}-{commit}/{name}'
        with urllib.request.build_opener(transport.HTTPSOnly()).open(url, timeout=30) as response:
            data = response.read(limit+1)
        if len(data)>limit:
            raise RuntimeError('download exceeds limit')
        path = work/name
        path.write_bytes(data)
        return path
    try:
        os.environ.clear()
        os.environ.update(clean)
        latest()
        permit_path = fetch('permit.json', 1024*1024)
        bundle = fetch('permit.sigstore.json', 4*1024**2)
        bounded.run(['bash', 'scripts/cloudflare-attestation-tools.sh', str(work/'tools')], work, started)
        gh = work/'tools/gh_2.104.0_linux_amd64/bin/gh'
        # Authenticate using the caller's compiled policy, never artifact-selected policy.
        permit = policy.verify(permit_path, bundle, '.github/cloudflare/trusted-root.jsonl',
                               gh, commit, latest())
        print('SIGNED_BUILD_ONLY_PERMIT_PASS', flush=True)
        archive = fetch('sdk.zip', 64*1024**2)
        sdk = permit['sdk']
        if archive.stat().st_size != sdk['archive_size_bytes'] or hashlib.sha256(archive.read_bytes()).hexdigest() != sdk['archive_sha256']:
            raise RuntimeError('SDK archive differs from signed permit')
        stage = work/'runtime'
        transport.unpack(archive, stage)
        bounded.run(['node', 'web/tools/sdk.mjs', 'verify', str(stage), '--release', '--current', '--commit', commit], work, started)
        target = repo/'web/runtime'
        if target.exists() or target.is_symlink():
            raise RuntimeError('refuse existing SDK')
        shutil.copytree(stage, target)
        bounded.run(['bash', 'scripts/site-artifact.sh', '_site'], work, started)
        bounded.run(['python3', 'scripts/cloudflare-site-check.py', '_site'], work, started)
        latest()
        print('SIGNED_SDK_ASTRO_BUILD_ONLY_PASS' if not candidate else 'SIGNED_CANDIDATE_BUILD_PASS', flush=True)
        if candidate:
            receipt = repo/'target/cloudflare-candidate'
            receipt.mkdir(parents=True, exist_ok=False)
            shutil.copyfile(permit_path, receipt/'permit.json')
            shutil.copyfile(bundle, receipt/'permit.sigstore.json')
            shutil.copyfile(archive, receipt/'sdk.zip')
            return 0
    except Exception as error:
        print('SIGNED_CONSUMER_BLOCKED:', str(error), flush=True)
    finally:
        os.environ.clear()
        os.environ.update(old)
        print('elapsed_seconds=', round(time.monotonic()-started), flush=True)
        print('Candidate build only; activation is separate' if candidate else 'Intentional experiment stop; no deployment', flush=True)
    return 1


if __name__ == '__main__':
    raise SystemExit(main())
