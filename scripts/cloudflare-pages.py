#!/usr/bin/env python3
"""Pages-specific signed SDK build policy; not a Workers upload permit."""
import importlib.util
import os
from pathlib import Path
import re
import subprocess
import sys

PROJECT = 'animatix-pages'
RELEASE_BRANCH = 'deploy/pages'
ISSUER_REF = 'refs/heads/main'
PREFIX = 'cf-pages'


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def policy():
    result = load('cloudflare-permit')
    result.SCHEMA = 3
    result.MODE = 'pages-static-deploy'
    result.REF = ISSUER_REF
    result.WORKFLOW = '.github/workflows/ci.yml'
    result.SIGNER = f'https://github.com/{result.REPOSITORY}/{result.WORKFLOW}@{result.REF}'
    result.DESTINATION = {'account_id': result.ACCOUNT, 'pages_project': PROJECT}
    return result


def release_tag(permit):
    return f"{PREFIX}-{permit['site_sha']}-r{permit['ci']['run_id']}-a{permit['ci']['run_attempt']}"


def release_tags(sha):
    policy().sha(sha)
    # Git's advertised refs avoid the shared anonymous REST API rate budget.
    # The tag is still discovery only: the signature authenticates its payload.
    result = subprocess.check_output(
        ['git', 'ls-remote', '--refs', '--tags', 'https://github.com/j4ger/animatix.git',
         f'refs/tags/{PREFIX}-{sha}-*'], text=True, timeout=30)
    return validate_refs(result, sha)


def validate_refs(result, sha):
    policy().sha(sha)
    if len(result) > 4*1024**2:
        raise ValueError('oversized Pages release inventory')
    tags = []
    for line in result.splitlines():
        parts = line.split()
        if len(parts) != 2 or parts[0] != sha:
            raise ValueError('Pages release must be a lightweight tag at this SHA')
        tag = parts[1].removeprefix('refs/tags/')
        if not re.fullmatch(rf'{PREFIX}-{sha}-r[1-9][0-9]{{0,18}}-a[1-9][0-9]{{0,5}}', tag):
            raise ValueError('invalid Pages issuance')
        tags.append(tag)
    if not tags or len(tags) > 10000 or len(set(tags)) != len(tags):
        raise ValueError('invalid Pages issuance inventory')
    return sorted(tags, key=lambda tag: tuple(map(int, re.search(r'-r([0-9]+)-a([0-9]+)$', tag).groups())),
                  reverse=True)[:8]


def check_context(env, commit):
    policy().sha(commit)
    if env.get('CF_PAGES') != '1' or env.get('CF_PAGES_BRANCH') != RELEASE_BRANCH:
        raise ValueError('only the controlled Pages production branch may build')
    if env.get('CF_PAGES_COMMIT_SHA') != commit:
        raise ValueError('Pages checkout SHA differs from deployment metadata')
    # This is a static site. Root functions would otherwise be auto-detected.
    for name in ('functions', '_worker.js'):
        if Path(name).exists() or Path(name).is_symlink():
            raise ValueError('unexpected Pages Functions input')


def main():
    if sys.argv[1:] == ['publish']:
        load('cloudflare-sdk-publisher').main(policy())
    elif sys.argv[1:] == ['build']:
        commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
        check_context(os.environ, commit)
        return load('cloudflare-verified-consumer').main(
            policy(), issuance=sys.modules[__name__], head_ref='refs/heads/' + RELEASE_BRANCH, pages=True)
    else:
        raise ValueError('usage: cloudflare-pages.py publish|build')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
