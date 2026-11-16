#!/usr/bin/env python3
"""Exact-SHA build-only dispatch. Restores temporary trigger settings; no deploy API."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import signal
import time
import urllib.request


SAFE_PROBE = 'if command -v timeout >/dev/null 2>&1 && command -v unshare >/dev/null 2>&1; then timeout 5s unshare --user --mount true >/dev/null 2>&1; rc=$?; printf \'user_mount_namespace_exit=%s\\n\' "$rc"; else echo \'namespace_probe_tools_missing\'; fi; echo \'Capability probe complete: no mappings, mounts, installation, or deployment\'; exit 1'
BUILD_COMMAND = "timeout -k 10 650 python3 web/tools/deploy/cloudflare-verified-consumer.py; exit 1"

def interrupted(signum, frame):
    raise RuntimeError("dispatcher interrupted; restoring settings")


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        raise RuntimeError('unexpected API redirect')


def main():
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    token = os.environ.get('CLOUDFLARE_EXPERIMENT_TOKEN')
    if not token:
        raise RuntimeError('missing user-provisioned CLOUDFLARE_EXPERIMENT_TOKEN; no dispatch')
    spec = importlib.util.spec_from_file_location('policy', Path(__file__).with_name('cloudflare-permit.py'))
    policy = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(policy)
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    if commit != os.environ.get('GITHUB_SHA') or os.environ.get('GITHUB_REF') != policy.REF:
        raise RuntimeError('publisher source mismatch')
    def latest():
        fields = subprocess.check_output(['git', 'ls-remote', 'origin', policy.REF], text=True, timeout=25).split()
        if len(fields) != 2:
            raise RuntimeError('cannot determine latest request')
        policy.accept_result(fields[0], commit)
    def request(method, path, body=None):
        data = None if body is None else json.dumps(body).encode()
        req = urllib.request.Request('https://api.cloudflare.com/client/v4/accounts/' + policy.ACCOUNT + path,
                                     data=data, method=method, headers={'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'})
        # No redirect, secret-bearing URL, token logging, or rate-limit retry.
        with urllib.request.build_opener(NoRedirect()).open(req, timeout=30) as response:
            raw = response.read(4*1024**2+1)
        if len(raw)>4*1024**2:
            raise RuntimeError('API response bound exceeded')
        result = json.loads(raw)
        if not result.get('success'):
            raise RuntimeError('Cloudflare API rejected request')
        return result.get('result')
    latest()
    trigger_path = '/builds/triggers/' + policy.TRIGGER
    triggers = request('GET', '/builds/workers/442539d56ae94a1c83c21ab8e84182ba/triggers')
    matches = [x for x in triggers if x['trigger_uuid'] == policy.TRIGGER]
    if len(matches) != 1 or matches[0]['branch_includes'] != ['experiment/cloudflare-native-build']:
        raise RuntimeError('experiment trigger filter changed; refusing mutation')
    original_command = matches[0]['build_command']
    if original_command != SAFE_PROBE:
        raise RuntimeError('safe probe baseline changed; refusing mutation')
    variables = request('GET', trigger_path + '/environment_variables')
    node = variables.get('NODE_VERSION')
    if node and (node.get('is_secret') or not isinstance(node.get('value'), str)):
        raise RuntimeError('cannot safely restore pre-existing Node variable')
    baseline = Path('target/cloudflare-dispatch-baseline.json')
    baseline.parent.mkdir(parents=True, exist_ok=True)
    recovery = dict(account_id=policy.ACCOUNT, trigger_uuid=policy.TRIGGER, source_sha=commit,
                    build_command=original_command, node_version=node, build_uuid=None)
    baseline.write_text(json.dumps(recovery, sort_keys=True))
    build_id = None
    terminal = False
    changed = False
    try:
        # Set before request: even ambiguous API outcome will be restored.
        changed = True
        request('PATCH', trigger_path + '/environment_variables', {'NODE_VERSION': {'is_secret': False, 'value': '24.14.0'}})
        request('PATCH', trigger_path, {'build_command': BUILD_COMMAND})
        latest()
        build = request('POST', trigger_path + '/builds', {'branch': 'experiment/cloudflare-native-build', 'commit_hash': commit})
        build_id = build['build_uuid']
        recovery['build_uuid'] = build_id
        baseline.write_text(json.dumps(recovery, sort_keys=True))
        print('Build-only request:', build_id, 'source:', commit, flush=True)
        deadline = time.monotonic() + 900
        while time.monotonic() < deadline:
            time.sleep(20)
            state = request('GET', '/builds/builds/' + build_id)
            metadata = state.get('build_trigger_metadata', {})
            if not (metadata.get('commit_hash') == commit and metadata.get('branch') == 'experiment/cloudflare-native-build'
                    and metadata.get('build_command') == BUILD_COMMAND and state.get('trigger', {}).get('trigger_uuid') == policy.TRIGGER):
                raise RuntimeError('actual build source/trigger mismatch')
            if state['status'] == 'stopped':
                terminal = True
                if state.get('build_outcome') != 'fail' or state.get('preview_url'):
                    raise RuntimeError('expected deliberate failure without deployment')
                logs = request('GET', '/builds/builds/' + build_id + '/logs')
                if logs.get('truncated') or not any(line[1] == 'SIGNED_SDK_ASTRO_BUILD_ONLY_PASS' for line in logs['lines']):
                    raise RuntimeError('signed consumer did not provide complete success evidence')
                latest()
                print('Verified build-only candidate completed; NO production activation', flush=True)
                return
        raise RuntimeError('build wait budget exhausted')
    finally:
        # Nested finally preserves restoration if cancellation itself fails.
        try:
            if build_id and not terminal:
                request('PUT', '/builds/builds/' + build_id + '/cancel')
        finally:
            if changed:
                try:
                    request('PATCH', trigger_path, {'build_command': original_command})
                finally:
                    if node is None:
                        request('DELETE', trigger_path + '/environment_variables/NODE_VERSION')
                    else:
                        request('PATCH', trigger_path + '/environment_variables', {'NODE_VERSION': {'is_secret': False, 'value': node['value']}})


if __name__ == '__main__':
    main()
