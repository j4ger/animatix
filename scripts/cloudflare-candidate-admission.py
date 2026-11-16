#!/usr/bin/env python3
"""Admit candidates from authenticated Builds evidence and signed original SDK."""
import hashlib
import base64
import datetime
import time
import importlib.util
import json
from pathlib import Path, PurePosixPath
import re
import tempfile
import urllib.request

spec = importlib.util.spec_from_file_location('control', Path(__file__).with_name('cloudflare-control.py'))
c = importlib.util.module_from_spec(spec)
spec.loader.exec_module(c)


def download(url, limit):
    # Public artifacts only. Never inherit a Cloudflare authorization header.
    if not url.startswith('https://'):
        raise ValueError('HTTPS required')
    opener = urllib.request.build_opener(c.load('cloudflare-sdk-consumer').HTTPSOnly())
    with opener.open(urllib.request.Request(url, headers={'Accept-Encoding': 'identity'}), timeout=30) as response:
        data = response.read(limit+1)
    if len(data) > limit:
        raise ValueError('artifact exceeds bound')
    return data


def validate_receipt(receipt, config, sha):
    if set(receipt) != {'source_sha', 'permit_sha256', 'archive_sha256', 'worker',
                        'version_id', 'preview_url', 'site_manifest_sha256', 'release_tag'}:
        raise ValueError('unexpected candidate receipt shape')
    if receipt['source_sha'] != sha or receipt['worker'] != config['worker']:
        raise ValueError('candidate source or destination mismatch')
    for field in ('permit_sha256', 'archive_sha256', 'site_manifest_sha256'):
        c.load('cloudflare-candidate').policy().sha(receipt[field], 64)
    c.load('cloudflare-candidate').validate_release_tag(receipt['release_tag'], sha)
    version = c.identifier(receipt['version_id'])
    expected = f"https://{version[:8]}-{config['worker']}.{config['workers_subdomain']}.workers.dev"
    if receipt['preview_url'] != expected:
        raise ValueError('not the exact immutable Version URL for the reviewed target')
    return expected


def validate_manifest(manifest, receipt):
    if set(manifest) != {'source_sha', 'permit_sha256', 'archive_sha256', 'worker', 'files', 'release_tag'}:
        raise ValueError('unexpected site manifest fields')
    if any(manifest[key] != receipt[key] for key in ('source_sha', 'permit_sha256', 'archive_sha256', 'worker', 'release_tag')):
        raise ValueError('site identity mismatch')
    files = manifest['files']
    if not isinstance(files, dict) or not 1 <= len(files) <= 20000:
        raise ValueError('invalid site file inventory')
    for name, digest in files.items():
        path = PurePosixPath(name)
        if not name or path.is_absolute() or '..' in path.parts or str(path) != name \
                or not re.fullmatch(r'[A-Za-z0-9_./-]+', name) or name == '_candidate.json' \
                or not isinstance(digest, str) or not re.fullmatch('[0-9a-f]{64}', digest):
            raise ValueError('unsafe site inventory')
    return files


# Source is pinned independently of the untrusted API receipt or candidate checkout.
STATIC_MODULE_SHA256 = '2e35546260a53329466e6640dad5bc694c94391a7ea2e165f175c5b85a788517'


def version_check(api, config, version):
    detail = api('GET', '/workers/workers/'+config['worker']+'/versions/'+c.identifier(version),
                 query={'include': 'modules'})
    if detail.get('id') != version or detail.get('bindings') != [{'type': 'assets', 'name': 'ASSETS'}]:
        raise ValueError('version/asset binding mismatch; no inference from zero bindings')
    for field in ('migration_tag', 'migrations', 'exports', 'containers'):
        if detail.get(field):
            raise ValueError('candidate contains non-static runtime resources')
    assets = detail.get('assets', {}).get('config', {})
    if assets.get('html_handling') != 'auto-trailing-slash' or assets.get('not_found_handling') != '404-page' \
            or assets.get('run_worker_first') is not False or assets.get('base_path', '/') != '/':
        raise ValueError('unexpected static asset routing')
    modules = detail.get('modules')
    if not isinstance(modules, list) or len(modules) != 1 or not detail.get('main_module'):
        raise ValueError('exact deployed module bytes required')
    module = modules[0]
    if module.get('name') != detail['main_module'] or module.get('content_type') != 'application/javascript+module':
        raise ValueError('unexpected executable module')
    content = base64.b64decode(module.get('content_base64', ''), validate=True)
    if hashlib.sha256(content).hexdigest() != STATIC_MODULE_SHA256:
        raise ValueError('deployed executable is not the reviewed static forwarding module')
    return detail


def timestamp(value):
    if not isinstance(value, str):
        raise ValueError('authenticated timestamp missing')
    parsed = datetime.datetime.fromisoformat(value.replace('Z', '+00:00'))
    if parsed.tzinfo is None:
        raise ValueError('timestamp lacks timezone')
    return parsed.timestamp()


def admission_window(build, version, now=None):
    # Both inputs originate from authenticated provider APIs, never site JSON.
    start = timestamp(build.get('running_on'))
    finish = timestamp(build.get('stopped_on'))
    uploaded = timestamp(version.get('created_on'))
    now = time.time() if now is None else now
    if not 0 < start <= uploaded <= finish <= now or finish-start > 1800:
        raise ValueError('invalid authenticated upload/build window')
    return start, int(finish)


def admit(api, config, build_id, gh, fetch=download, check_http=None, verify_signature=None):
    policy = c.load('cloudflare-candidate').policy()
    build_id = c.identifier(build_id)
    build = api('GET', '/builds/builds/'+build_id)
    sha = build.get('build_trigger_metadata', {}).get('commit_hash')
    policy.sha(sha)
    c.validate_build(build, config, sha)
    if build.get('status') != 'stopped' or build.get('build_outcome') != 'success':
        raise ValueError('completed successful candidate build required')
    receipt = c.receipt_from_logs(api('GET', '/builds/builds/'+build_id+'/logs'))
    origin = validate_receipt(receipt, config, sha)
    version = version_check(api, config, receipt['version_id'])
    build_start, build_finish = admission_window(build, version)
    raw_manifest = fetch(origin+'/_candidate.json', 4*1024**2)
    if hashlib.sha256(raw_manifest).hexdigest() != receipt['site_manifest_sha256']:
        raise ValueError('site manifest differs from authenticated build receipt')
    manifest = policy.decode(raw_manifest)
    files = validate_manifest(manifest, receipt)
    with tempfile.TemporaryDirectory(prefix='animatix-admit-') as temporary:
        work = Path(temporary)
        release_tag = receipt['release_tag']
        base = f'https://github.com/{policy.REPOSITORY}/releases/download/{release_tag}/'
        for name, limit in [('permit.json', 1024**2), ('permit.sigstore.json', 4*1024**2), ('sdk.zip', policy.MAX_ARCHIVE)]:
            (work/name).write_bytes(fetch(base+name, limit))
        permit_raw = (work/'permit.json').read_bytes()
        archive = (work/'sdk.zip').read_bytes()
        if hashlib.sha256(permit_raw).hexdigest() != receipt['permit_sha256']:
            raise ValueError('permit digest mismatch')
        # Pin roots even when the caller supplies the already pinned gh executable.
        root = Path('.github/cloudflare/trusted-root.jsonl')
        if hashlib.sha256(root.read_bytes()).hexdigest() != '65ca537f6ed8a47fd0e560c421baa1f6c1efb8b25fc200d8c5c02c0e92eb2b9c':
            raise ValueError('unreviewed trusted root snapshot')
        verify = verify_signature or policy.verify
        permit = verify(work/'permit.json', work/'permit.sigstore.json', root, gh, sha, validation_time=build_finish)
        policy.validate_permit(permit, sha, now=build_finish)
        if permit['issued_at'] > build_start+60 or c.load('cloudflare-candidate').release_tag(permit) != receipt['release_tag']:
            raise ValueError('permit was not issued for this completed build/issuance')
        if hashlib.sha256(archive).hexdigest() != permit['sdk']['archive_sha256'] \
                or receipt['archive_sha256'] != permit['sdk']['archive_sha256'] \
                or len(archive) != permit['sdk']['archive_size_bytes']:
            raise ValueError('signed SDK bytes mismatch')
        c.load('cloudflare-sdk-consumer').unpack(work/'sdk.zip', work/'runtime')
        expected = {'runtime/'+p: digest for p, digest in c.load('cloudflare-candidate-upload').tree(work/'runtime').items()}
        actual = {p: digest for p, digest in files.items() if p.startswith('runtime/')}
        if expected != actual:
            raise ValueError('deployed SDK inventory differs from signed CI archive')
        check = check_http or c.load('cloudflare-preview-check').verify
        check(origin, manifest)
        # Keep cryptographic evidence in the durable record. No credentials.
        evidence = dict(receipt, build_uuid=build_id, permit=json.loads(permit_raw),
                        bundle_sha256=hashlib.sha256((work/'permit.sigstore.json').read_bytes()).hexdigest(),
                        bundle=policy.decode((work/'permit.sigstore.json').read_bytes()),
                        site_manifest=manifest)
    return hashlib.sha256(c.canonical(evidence)).hexdigest(), evidence
