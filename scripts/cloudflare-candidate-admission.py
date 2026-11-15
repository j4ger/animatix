#!/usr/bin/env python3
"""Admit candidates from authenticated Builds evidence and signed original SDK."""
import hashlib
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
                        'version_id', 'preview_url', 'site_manifest_sha256'}:
        raise ValueError('unexpected candidate receipt shape')
    if receipt['source_sha'] != sha or receipt['worker'] != config['worker']:
        raise ValueError('candidate source or destination mismatch')
    for field in ('permit_sha256', 'archive_sha256', 'site_manifest_sha256'):
        c.load('cloudflare-candidate').policy().sha(receipt[field], 64)
    version = c.identifier(receipt['version_id'])
    expected = f"https://{version[:8]}-{config['worker']}.{config['workers_subdomain']}.workers.dev"
    if receipt['preview_url'] != expected:
        raise ValueError('not the exact immutable Version URL for the reviewed target')
    return expected


def validate_manifest(manifest, receipt):
    if set(manifest) != {'source_sha', 'permit_sha256', 'archive_sha256', 'worker', 'files'}:
        raise ValueError('unexpected site manifest fields')
    if any(manifest[key] != receipt[key] for key in ('source_sha', 'permit_sha256', 'archive_sha256', 'worker')):
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


def version_check(api, config, version):
    detail = api('GET', '/workers/scripts/'+config['worker']+'/versions/'+c.identifier(version))
    if detail.get('id') != version:
        raise ValueError('version lookup mismatch')
    resources = detail.get('resources', {})
    if not isinstance(resources.get('bindings'), list) or any(b.get('type') != 'assets' for b in resources['bindings']):
        raise ValueError('candidate has unexpected resource bindings')
    runtime = resources.get('script_runtime', {})
    if runtime.get('migration_tag') or runtime.get('exports'):
        raise ValueError('candidate contains persistent runtime exports')
    return detail


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
    version_check(api, config, receipt['version_id'])
    raw_manifest = fetch(origin+'/_candidate.json', 4*1024**2)
    if hashlib.sha256(raw_manifest).hexdigest() != receipt['site_manifest_sha256']:
        raise ValueError('site manifest differs from authenticated build receipt')
    manifest = policy.decode(raw_manifest)
    files = validate_manifest(manifest, receipt)
    with tempfile.TemporaryDirectory(prefix='animatix-admit-') as temporary:
        work = Path(temporary)
        base = f'https://github.com/{policy.REPOSITORY}/releases/download/{policy.RELEASE_PREFIX}-{sha}/'
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
        permit = verify(work/'permit.json', work/'permit.sigstore.json', root, gh, sha)
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
