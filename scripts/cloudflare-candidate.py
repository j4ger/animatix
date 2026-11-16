#!/usr/bin/env python3
"""Separate signed preview-candidate policy. Never authorizes production traffic."""
import importlib.util
from pathlib import Path
import sys
import json
import re
import urllib.request


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def policy():
    result = load('cloudflare-permit')
    result.SCHEMA = 2
    result.MODE = 'candidate-upload'
    result.REF = 'refs/heads/feat/cloudflare-candidate-preview'
    result.WORKFLOW = '.github/workflows/ci.yml'
    result.SIGNER = f'https://github.com/{result.REPOSITORY}/{result.WORKFLOW}@{result.REF}'
    result.DESTINATION = {'account_id': result.ACCOUNT, 'worker': 'animatix-candidate-preview'}
    result.RELEASE_PREFIX = 'cf-preview'
    return result


def release_tag(permit):
    return f"cf-preview-{permit['site_sha']}-r{permit['ci']['run_id']}-a{permit['ci']['run_attempt']}"


def validate_release_tag(tag, sha):
    policy().sha(sha)
    if not isinstance(tag, str) or not re.fullmatch(rf'cf-preview-{sha}-r[1-9][0-9]{{0,18}}-a[1-9][0-9]{{0,5}}', tag):
        raise ValueError('invalid issuance-specific release tag')
    return tag


def release_tags(sha, refs=None):
    policy().sha(sha)
    if refs is None:
        url = f'https://api.github.com/repos/j4ger/animatix/git/matching-refs/tags/cf-preview-{sha}-'
        with urllib.request.urlopen(url, timeout=30) as response:
            raw = response.read(4*1024**2+1)
        if len(raw) > 4*1024**2:
            raise ValueError('release discovery exceeds bound')
        refs = json.loads(raw)
    if not isinstance(refs, list):
        raise ValueError('invalid release reference inventory')
    tags = []
    for ref in refs:
        name = ref.get('ref', '').removeprefix('refs/tags/')
        validate_release_tag(name, sha)
        if ref.get('object', {}).get('type') != 'commit' or ref['object'].get('sha') != sha:
            raise ValueError('release tag does not point at candidate SHA')
        tags.append(name)
    if not tags or len(tags) > 10000 or len(set(tags)) != len(tags):
        raise ValueError('invalid or oversized permit issuance inventory')
    # This orders discovery of permits for ONE source SHA, never activation order.
    return sorted(tags, key=lambda tag: tuple(map(int, re.search(r'-r([0-9]+)-a([0-9]+)$', tag).groups())), reverse=True)[:8]


if __name__ == '__main__':
    if sys.argv[1:] == ['publish']:
        load('cloudflare-sdk-publisher').main(policy())
    elif sys.argv[1:] == ['build']:
        raise SystemExit(load('cloudflare-verified-consumer').main(policy(), candidate=True))
    else:
        raise SystemExit('usage: cloudflare-candidate.py publish|build')
