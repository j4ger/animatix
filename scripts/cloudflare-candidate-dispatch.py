#!/usr/bin/env python3
"""Trigger a fixed configuration at the signed SHA. Never edit a trigger or activate."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import time

spec = importlib.util.spec_from_file_location('control', Path(__file__).with_name('cloudflare-control.py'))
c = importlib.util.module_from_spec(spec)
spec.loader.exec_module(c)


def dispatch(api, config, sha, state_path, wait=time.sleep, clock=time.monotonic):
    c.load('cloudflare-candidate').policy().sha(sha)
    triggers = api('GET', '/builds/workers/'+config['worker_tag']+'/triggers')
    matches = [t for t in triggers if t.get('trigger_uuid') == config['trigger_uuid']]
    if len(matches) != 1:
        raise ValueError('dedicated build trigger missing')
    c.validate_trigger(matches[0], config)
    variables = api('GET', '/builds/triggers/'+config['trigger_uuid']+'/environment_variables')
    for key, value in [('NODE_VERSION', '24.14.0'), ('SKIP_DEPENDENCY_INSTALL', '1')]:
        if variables.get(key, {}).get('value') != value or variables[key].get('is_secret') is not False:
            raise ValueError('required non-secret build variable missing')
    state_path = Path(state_path)
    state_path.parent.mkdir(parents=True, exist_ok=True)
    # Exclusive marker survives a local retry. A new CI job must inspect retained
    # recovery evidence; neither transient job storage nor concurrency is a fence.
    with state_path.open('x') as out:
        out.write(json.dumps({'sha': sha, 'trigger_uuid': config['trigger_uuid'], 'status': 'post-may-have-started'})+'\n')
        out.flush()
        os.fsync(out.fileno())
    build = api('POST', '/builds/triggers/'+config['trigger_uuid']+'/builds',
                {'branch': c.BRANCH, 'commit_hash': sha})
    build_id = c.identifier(build['build_uuid'])
    state_path.write_bytes(c.canonical({'sha': sha, 'build_uuid': build_id, 'status': 'waiting'}))
    return inspect_build(api, config, sha, build_id, state_path, wait, clock)


def inspect_build(api, config, sha, build_id, state_path, wait=time.sleep, clock=time.monotonic):
    c.identifier(build_id)
    state_path = Path(state_path)
    state_path.parent.mkdir(parents=True, exist_ok=True)
    deadline = clock()+900
    while clock() < deadline:
        state = api('GET', '/builds/builds/'+build_id)
        c.validate_build(state, config, sha)
        if state['status'] == 'stopped':
            if state.get('build_outcome') != 'success':
                raise RuntimeError('candidate build failed; inspect recorded build UUID')
            receipt = c.receipt_from_logs(api('GET', '/builds/builds/'+build_id+'/logs'))
            if receipt.get('source_sha') != sha or receipt.get('worker') != config['worker']:
                raise ValueError('upload receipt source/worker mismatch')
            c.identifier(receipt['version_id'])
            result = {'sha': sha, 'build_uuid': build_id, 'status': 'candidate-uploaded', 'receipt': receipt}
            state_path.write_bytes(c.canonical(result))
            return result
        wait(15)
    # No automatic POST retry, trigger mutation, activation or ambiguous cancel.
    raise RuntimeError('candidate wait timed out; resume inspection using recorded build UUID')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--resume-build', help='Read-only continuation; never sends a new POST')
    args = parser.parse_args()
    config = c.target()
    sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    if os.environ.get('GITHUB_ACTIONS') == 'true':
        if os.environ.get('GITHUB_SHA') != sha or os.environ.get('GITHUB_REF') != 'refs/heads/'+c.BRANCH \
                or os.environ.get('GITHUB_REPOSITORY') != 'j4ger/animatix':
            raise ValueError('dispatch workflow source mismatch')
        if os.environ.get('GITHUB_RUN_ATTEMPT') != '1' and not args.resume_build:
            raise ValueError('rerun cannot establish whether previous POST executed; use read-only resume')
    elif not args.resume_build:
        raise ValueError('fresh dispatch is CI-only; local recovery requires --resume-build')
    api = c.API(config, os.environ.get('CLOUDFLARE_EXPERIMENT_TOKEN'))
    path = 'target/cloudflare-candidate-dispatch.json'
    result = (inspect_build(api, config, sha, args.resume_build, path) if args.resume_build
              else dispatch(api, config, sha, path))
    print('CANDIDATE_DISPATCH_PASS', result['build_uuid'], result['receipt']['version_id'])


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        print('CANDIDATE_DISPATCH_BLOCKED', str(error) if isinstance(error, (ValueError, RuntimeError)) else type(error).__name__)
        raise SystemExit(1)
