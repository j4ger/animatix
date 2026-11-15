#!/usr/bin/env python3
"""Prepare a permit from this CI run's original SDK artifact, without publishing."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import zipfile


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main(permit=None):
    permit = permit or load('cloudflare-permit')
    consumer = load('cloudflare-sdk-consumer')
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    if os.environ.get('NEEDS_CI_RESULT') != 'success' or commit != os.environ['GITHUB_SHA']:
        raise RuntimeError('CI acceptance/checkout mismatch')
    if os.environ['GITHUB_REF'] != permit.REF or os.environ['GITHUB_REPOSITORY'] != permit.REPOSITORY:
        raise RuntimeError('experiment publisher only')
    run_id = int(os.environ['GITHUB_RUN_ID'])
    result = json.loads(subprocess.check_output(['gh', 'api', f'repos/{permit.REPOSITORY}/actions/runs/{run_id}/artifacts?name=web-runtime&per_page=100'], text=True))
    matches = [a for a in result['artifacts'] if a['name'] == 'web-runtime' and not a['expired']]
    if len(matches) != 1:
        raise RuntimeError('expected one original SDK artifact')
    artifact = matches[0]
    if not 0 < artifact['size_in_bytes'] <= 64*1024**2:
        raise RuntimeError('SDK archive exceeds limit')
    origin = artifact['workflow_run']
    if not (origin['id'] == run_id and origin['head_sha'] == commit
            and origin['head_repository_id'] == origin['repository_id'] == permit.REPOSITORY_ID):
        raise RuntimeError('artifact source mismatch')
    out = Path('target/cloudflare-permit')
    out.mkdir(parents=True, exist_ok=False)
    archive = out / 'sdk.zip'
    with archive.open('wb') as stream:
        subprocess.run(['gh', 'api', f'repos/{permit.REPOSITORY}/actions/artifacts/{artifact["id"]}/zip'], stdout=stream, check=True, timeout=120)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    if artifact['digest'] != 'sha256:' + digest or archive.stat().st_size != artifact['size_in_bytes']:
        raise RuntimeError('original artifact digest/size mismatch')
    with tempfile.TemporaryDirectory() as stage:
        consumer.unpack(archive, stage)
        subprocess.run(['node', 'web/tools/sdk.mjs', 'verify', stage, '--release', '--current', '--commit', commit], check=True)
        manifest = json.loads((Path(stage) / 'runtime-manifest.json').read_text())
        if manifest['build']['source_dirty']:
            raise RuntimeError('dirty SDK source')
    metadata = dict(repository_id=int(os.environ['GITHUB_REPOSITORY_ID']), repository=permit.REPOSITORY,
                    ref=permit.REF, workflow=permit.WORKFLOW, site_sha=commit, sdk_source_sha=commit,
                    producer_sha=commit, run_id=run_id, run_attempt=int(os.environ['GITHUB_RUN_ATTEMPT']),
                    needs_ci_result='success', artifact_id=artifact['id'], archive_sha256=digest,
                    archive_size_bytes=archive.stat().st_size)
    (out / 'permit.json').write_text(json.dumps(permit.emit(metadata), sort_keys=True) + '\n')
    print('Original same-SHA CI SDK verified; unsigned permit prepared for attestation')


if __name__ == '__main__':
    main()
