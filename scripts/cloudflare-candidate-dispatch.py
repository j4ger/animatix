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


def check_prior_dispatch(run_id, attempt, sha, read_api=None):
    """Only prove non-execution from complete authenticated per-attempt job history."""
    if not str(run_id).isdigit() or int(run_id) < 1 or not str(attempt).isdigit() or not 1 <= int(attempt) <= 50:
        raise ValueError('invalid workflow invocation')
    def read(path):
        return json.loads(subprocess.check_output(['gh', 'api', '--hostname', 'github.com', path], text=True))
    read = read_api or read
    for previous in range(1, int(attempt)):
        base = f'/repos/j4ger/animatix/actions/runs/{run_id}/attempts/{previous}'
        run = read(base)
        if run.get('id') != int(run_id) or run.get('run_attempt') != previous or run.get('head_sha') != sha \
                or run.get('head_branch') != c.BRANCH or run.get('event') != 'push' \
                or run.get('path') != '.github/workflows/ci.yml' or run.get('status') != 'completed' \
                or run.get('repository', {}).get('id') != 1185977116:
            raise ValueError('previous workflow identity/completion unavailable')
        jobs, total = [], None
        for page in range(1, 101):
            result = read(base+f'/jobs?per_page=100&page={page}')
            count, batch = result.get('total_count'), result.get('jobs')
            if type(count) is not int or not 1 <= count <= 10000 or not isinstance(batch, list) \
                    or not 1 <= len(batch) <= 100 or (total is not None and total != count):
                raise ValueError('incomplete previous jobs inventory')
            total = count
            jobs.extend(batch)
            if len(jobs) >= total:
                break
        if len(jobs) != total or len({job.get('id') for job in jobs}) != total:
            raise ValueError('incomplete or duplicate previous jobs')
        for job in jobs:
            if job.get('run_id') != int(run_id) or job.get('head_sha') != sha or job.get('status') != 'completed':
                raise ValueError('previous job identity/completion unavailable')
            if job.get('name') != 'Dispatch isolated Cloudflare candidate':
                continue
            if job.get('conclusion') == 'skipped':
                continue
            steps = [step for step in job.get('steps', [])
                     if step.get('name') == 'Trigger exact-SHA candidate build after publication']
            if len(steps) != 1 or steps[0].get('status') != 'completed' or steps[0].get('conclusion') != 'skipped':
                raise ValueError('previous dispatch may have executed; use read-only resume')


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
        if not args.resume_build:
            check_prior_dispatch(os.environ.get('GITHUB_RUN_ID'), os.environ.get('GITHUB_RUN_ATTEMPT'), sha)
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
