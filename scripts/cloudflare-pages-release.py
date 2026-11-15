#!/usr/bin/env python3
"""Sole Pages publisher. Durable send intent survives cancelled Actions runners.

The GitHub concurrency group serializes live invocations, not commit order. The
journal blocks all subsequent sends after an uncertain POST. No external actor
may update deploy/pages or trigger deployments while this publisher is enabled.
"""
import base64
import json
import os
import subprocess
import sys
import time
import urllib.request
from pathlib import Path
import importlib.util


def load():
    spec = importlib.util.spec_from_file_location('pages', Path(__file__).with_name('cloudflare-pages.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


PAGES = load()
REPO = 'j4ger/animatix'
STATE_BRANCH = 'deploy/pages-state'
STATE_PATH = '.pages-release-state.json'
TERMINAL = {'success', 'failure', 'canceled'}


class API:
    def gh(self, path, method='GET', data=None):
        args = ['gh', 'api', '--method', method, f'repos/{REPO}/{path}']
        if data is not None:
            args += ['--input', '-']
        raw = subprocess.check_output(args, input=json.dumps(data) if data is not None else None,
                                      text=True, timeout=60)
        return json.loads(raw) if raw.strip() else None

    def cf(self, path='', method='GET', data=None):
        url = f'https://api.cloudflare.com/client/v4/accounts/{PAGES.policy().ACCOUNT}/pages/projects/{PAGES.PROJECT}' + path
        payload = None
        headers = {'Authorization': 'Bearer ' + os.environ['CLOUDFLARE_PAGES_TOKEN']}
        if data is not None:
            # Pages Git deployment creation uses multipart, not JSON. Only the
            # fixed release branch is supplied; commit_hash is metadata only.
            boundary = 'animatix-pages-release-boundary'
            if set(data) != {'branch'} or data['branch'] != PAGES.RELEASE_BRANCH:
                raise RuntimeError('invalid deployment request fields')
            payload = ''.join(
                f'--{boundary}\r\nContent-Disposition: form-data; name="{key}"\r\n\r\n{value}\r\n'
                for key, value in data.items()).encode() + f'--{boundary}--\r\n'.encode()
            headers['Content-Type'] = 'multipart/form-data; boundary=' + boundary
        request = urllib.request.Request(url, method=method, data=payload, headers=headers)
        # No retries on writes: response loss may mean deployment was accepted.
        with urllib.request.urlopen(request, timeout=60) as response:
            result = json.load(response)
        if result.get('success') is not True:
            raise RuntimeError('Cloudflare operation failed')
        return result['result']

    def head(self, branch):
        return self.gh('git/ref/heads/' + branch)['object']['sha']

    def journal(self):
        result = self.gh(f'contents/{STATE_PATH}?ref={STATE_BRANCH}')
        return json.loads(base64.b64decode(result['content'])), result['sha']

    def save(self, state, revision):
        data = {
            'message': 'chore(ci): record Pages release transaction',
            'branch': STATE_BRANCH,
            'content': base64.b64encode((json.dumps(state, sort_keys=True) + '\n').encode()).decode()
        }
        if revision is not None:
            data['sha'] = revision
        return self.gh('contents/' + STATE_PATH, 'PUT', data)['content']['sha']

    def deployments(self):
        # Read every page. Never treat a truncated inventory as idle.
        results = []
        for page in range(1, 101):
            batch = self.cf(f'/deployments?per_page=100&page={page}')
            results.extend(batch)
            if len(batch) < 100:
                return results
        raise RuntimeError('deployment inventory exceeds bound')


def validate_project(project):
    cfg = project['source']['config']
    if (project['production_branch'] != PAGES.RELEASE_BRANCH
            or cfg['production_branch'] != PAGES.RELEASE_BRANCH
            or cfg['production_deployments_enabled'] is not False
            or cfg['preview_deployment_setting'] != 'none'):
        raise RuntimeError('Pages automatic builds must be disabled')


def deployment_status(deployment, sha):
    if (deployment['environment'] != 'production'
            or deployment['deployment_trigger']['metadata']['commit_hash'] != sha
            or deployment['deployment_trigger']['metadata']['branch'] != PAGES.RELEASE_BRANCH
            or deployment.get('is_skipped')):
        raise RuntimeError('deployment identity mismatch')
    return deployment['latest_stage']['status']


def active(deployment):
    return not deployment.get('is_skipped') and deployment['latest_stage']['status'] not in TERMINAL


def reconcile(api, state, rev, sleep=time.sleep):
    phase = state['phase']
    if phase == 'intent':
        raise RuntimeError('unknown POST outcome: frozen; no safe receipt correlation is available')
    if phase == 'blocked':
        raise RuntimeError('previous deployment failed: operator recovery required')
    if phase != 'waiting':
        return state, rev
    for _ in range(180):
        deployment = api.cf('/deployments/' + state['deployment_id'])
        status = deployment_status(deployment, state['sha'])
        if status in TERMINAL:
            if status == 'success':
                canonical = api.cf()['canonical_deployment']
                if not canonical or canonical['id'] != state['deployment_id']:
                    raise RuntimeError('successful deployment is not canonical; freeze on drift')
                state = dict(state, phase='complete')
            else:
                state = dict(state, phase='blocked', failure=status)
            rev = api.save(state, rev)
            if state['phase'] == 'blocked':
                raise RuntimeError('Pages deployment failed; journal retained')
            return state, rev
        sleep(10)
    raise RuntimeError('Pages wait timed out; journal retained for next invocation')


def release(api, sha):
    PAGES.policy().sha(sha)
    validate_project(api.cf())
    state, rev = api.journal()
    if state.get('schema') != 1 or state.get('phase') not in {'ready', 'complete', 'preparing', 'intent', 'waiting', 'blocked'}:
        raise RuntimeError('invalid release journal')
    state, rev = reconcile(api, state, rev)
    # No external send can occur in preparing. A later candidate may recover
    # this phase only if the release ref still has one of its recorded values.
    if state['phase'] == 'preparing' and api.head(PAGES.RELEASE_BRANCH) not in {state['sha'], state['previous_sha']}:
        raise RuntimeError('preparatory branch drift')
    if state['phase'] == 'complete':
        canonical = api.cf()['canonical_deployment']
        if not canonical or canonical['id'] != state['deployment_id']:
            raise RuntimeError('canonical deployment drift')
    if api.head('main') != sha:
        print('Superseded CI result; no release started')
        return
    if state['phase'] == 'complete' and state['sha'] == sha:
        print('Already deployed')
        return
    if any(active(d) for d in api.deployments()):
        raise RuntimeError('unowned active deployment; refusing to advance release branch')
    old = api.head(PAGES.RELEASE_BRANCH)
    if old != sha:
        comparison = api.gh(f'compare/{old}...{sha}')
        if comparison['status'] != 'ahead':
            raise RuntimeError('release must be a forward descendant')
    state = {'schema': 1, 'phase': 'preparing', 'sha': sha, 'previous_sha': old}
    rev = api.save(state, rev)
    if old != sha:
        # GitHub rejects a non-fast-forward; sole writer and durable intent are
        # the release lock. This is not advertised as a ref-CAS API.
        api.gh('git/refs/heads/' + PAGES.RELEASE_BRANCH, 'PATCH', {'sha': sha, 'force': False})
    if api.head(PAGES.RELEASE_BRANCH) != sha:
        raise RuntimeError('release branch drift')
    state = dict(state, phase='intent', requested_at=int(time.time()))
    rev = api.save(state, rev)  # Persist BEFORE the one external send.
    deployment = api.cf('/deployments', 'POST', {'branch': PAGES.RELEASE_BRANCH})
    deployment_status(deployment, sha)
    state = dict(state, phase='waiting', deployment_id=deployment['id'])
    rev = api.save(state, rev)
    reconcile(api, state, rev)


def initialize(api):
    """One explicit operator action; never auto-initialize on a failed read."""
    project = api.cf()
    validate_project(project)
    if any(active(d) for d in api.deployments()):
        raise RuntimeError('cannot initialize while deployments are active')
    current = project['canonical_deployment']
    sha = api.head(PAGES.RELEASE_BRANCH)
    if not current or deployment_status(current, sha) != 'success':
        raise RuntimeError('initialize from an accepted canonical production deployment')
    # Existing branch -> API rejects; never overwrite/reinitialize its journal.
    api.gh('git/refs', 'POST', {'ref': 'refs/heads/' + STATE_BRANCH, 'sha': api.head('main')})
    api.save({'schema': 1, 'phase': 'complete', 'sha': sha, 'deployment_id': current['id']}, None)


def recover_failed(api):
    """Explicit operator recovery only after the failed request is terminal."""
    state, rev = api.journal()
    if state['phase'] != 'blocked':
        raise RuntimeError('not a known failed transaction')
    result = api.cf('/deployments/' + state['deployment_id'])
    if deployment_status(result, state['sha']) not in {'failure', 'canceled'}:
        raise RuntimeError('failure is no longer confirmed')
    if any(active(d) for d in api.deployments()):
        raise RuntimeError('active deployment prevents failure recovery')
    api.save({'schema': 1, 'phase': 'ready', 'recovered_failure': state}, rev)


def main():
    api = API()
    if sys.argv[1:] == ['--recover-failed']:
        return recover_failed(api)
    if sys.argv[1:] == ['--initialize']:
        return initialize(api)
    if sys.argv[1:]:
        raise RuntimeError('usage: [--initialize | --recover-failed]')
    if (os.environ.get('GITHUB_REPOSITORY') != REPO or os.environ.get('GITHUB_REF') != 'refs/heads/main'
            or os.environ.get('NEEDS_PAGES_SIGN_RESULT') != 'success'):
        raise RuntimeError('only successful main signing may release')
    release(api, os.environ['GITHUB_SHA'])


if __name__ == '__main__':
    main()
