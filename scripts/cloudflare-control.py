#!/usr/bin/env python3
"""Fixed-target Cloudflare control transport; no redirects, retries or secret logs."""
import importlib.util
import json
import os
from pathlib import Path
import re
import urllib.error
import urllib.request
import urllib.parse
import uuid

BUILD = 'bash scripts/ci.sh gate cloudflare-candidate-build'
UPLOAD = 'bash scripts/ci.sh gate cloudflare-candidate-upload'
BRANCH = 'feat/cloudflare-candidate-preview'
RECEIPT_MARKER = 'CANDIDATE_UPLOAD_RECEIPT '


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name+'.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def canonical(value):
    return (json.dumps(value, sort_keys=True, separators=(',', ':'))+'\n').encode()


def identifier(value):
    if not isinstance(value, str) or str(uuid.UUID(value)) != value:
        raise ValueError('expected canonical UUID')
    return value


def target(path='.github/cloudflare/preview-target.json'):
    value = json.loads(Path(path).read_text())
    policy = load('cloudflare-candidate').policy()
    if set(value) != {'enabled', 'account_id', 'worker', 'worker_tag', 'trigger_uuid',
                      'build_token_uuid', 'workers_subdomain', 'repo_connection_uuid'}:
        raise ValueError('unexpected target fields')
    if value['enabled'] is not True:
        raise ValueError('preview target disabled; provision and review target identities first')
    if value['account_id'] != policy.ACCOUNT or value['worker'] != policy.DESTINATION['worker']:
        raise ValueError('target is outside isolated preview policy')
    for name in ('trigger_uuid', 'build_token_uuid', 'repo_connection_uuid'):
        identifier(value[name])
    if not isinstance(value['worker_tag'], str) or not re.fullmatch('[0-9a-f]{32}', value['worker_tag']):
        raise ValueError('invalid reviewed Worker tag')
    if value['worker_tag'] in {'442539d56ae94a1c83c21ab8e84182ba', '7b143240c1ae4df7bd88fa753efdc133'} \
            or value['trigger_uuid'] == policy.TRIGGER \
            or value['build_token_uuid'] in {'f343df38-5ebb-4f05-ab89-1d3ca32cfad7', '349e4a6d-1231-4fa8-8fdf-c76a162fe008'}:
        raise ValueError('existing experiment/blog/other-app target is forbidden')
    if not isinstance(value['workers_subdomain'], str) or not re.fullmatch('[a-z0-9]+(?:-[a-z0-9]+)*', value['workers_subdomain']):
        raise ValueError('invalid reviewed workers.dev subdomain')
    return value


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        raise RuntimeError('control-plane redirect rejected')


class ApiError(RuntimeError):
    """Outcome may be unknown; callers must not retry mutations automatically."""


class API:
    def __init__(self, configuration, token):
        if not token:
            raise ValueError('missing configured Cloudflare token')
        self.configuration = configuration
        self.token = token

    def __call__(self, method, path, body=None, query=None):
        if not path.startswith('/') or '..' in path or '?' in path or '#' in path:
            raise ValueError('invalid API path')
        base = 'https://api.cloudflare.com/client/v4/accounts/'+self.configuration['account_id']
        if query not in (None, {'include': 'modules'}):
            raise ValueError('unreviewed control query')
        url = base+path+('?' + urllib.parse.urlencode(query) if query else '')
        request = urllib.request.Request(url, method=method,
                  data=None if body is None else canonical(body),
                  headers={'Authorization': 'Bearer '+self.token, 'Content-Type': 'application/json'})
        try:
            with urllib.request.build_opener(NoRedirect()).open(request, timeout=30) as response:
                raw = response.read(16*1024**2+1)
            if len(raw) > 16*1024**2:
                raise ApiError('API response exceeds bound')
            result = json.loads(raw)
            if result.get('success') is not True:
                raise ApiError('Cloudflare response not successful')
            return result['result']
        except (OSError, ValueError) as error:
            # Never echo HTTP body, headers, request or raw exception text.
            raise ApiError('Cloudflare request failed or outcome unknown') from None


def validate_trigger(trigger, config):
    expected = {'trigger_uuid': config['trigger_uuid'], 'external_script_id': config['worker_tag'],
                'build_command': BUILD, 'deploy_command': UPLOAD, 'root_directory': '/',
                'branch_includes': [BRANCH], 'branch_excludes': [],
                'build_token_uuid': config['build_token_uuid']}
    if any(trigger.get(k) != v for k, v in expected.items()):
        raise ValueError('dedicated trigger identity, commands or branch configuration drifted')
    connection = trigger.get('repo_connection', {})
    if connection.get('repo_connection_uuid') != config['repo_connection_uuid'] \
            or str(connection.get('repo_id')) != '1185977116' \
            or connection.get('repo_name') != 'animatix' \
            or connection.get('provider_account_name') != 'j4ger' \
            or connection.get('provider_type') != 'github':
        raise ValueError('trigger repository identity mismatch')


def validate_build(build, config, sha):
    meta = build.get('build_trigger_metadata', {})
    if build.get('trigger', {}).get('trigger_uuid') != config['trigger_uuid'] \
            or meta.get('commit_hash') != sha or meta.get('branch') != BRANCH \
            or meta.get('build_command') != BUILD or meta.get('deploy_command') != UPLOAD \
            or meta.get('build_token_uuid') != config['build_token_uuid'] \
            or meta.get('root_directory') != '/' \
            or meta.get('provider_account_name') != 'j4ger' or meta.get('repo_name') != 'animatix' \
            or meta.get('provider_type') != 'github':
        raise ValueError('actual build source, target or command mismatch')


def receipt_from_logs(logs):
    if logs.get('truncated') is not False:
        raise ValueError('complete build logs required')
    matches = [line[1][len(RECEIPT_MARKER):] for line in logs['lines']
               if isinstance(line, list) and len(line) == 2 and isinstance(line[1], str)
               and line[1].startswith(RECEIPT_MARKER)]
    if len(matches) != 1 or len(matches[0]) > 16384:
        raise ValueError('expected one bounded upload receipt')
    return json.loads(matches[0])
