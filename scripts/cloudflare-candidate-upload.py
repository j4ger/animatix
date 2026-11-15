#!/usr/bin/env python3
"""Revalidate candidate, then upload an unactivated version to the isolated Worker."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import uuid

from importlib.util import spec_from_file_location, module_from_spec


def load(name):
    spec = spec_from_file_location(name, Path(__file__).with_name(name + '.py'))
    module = module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def tree(root):
    result = {}
    for path in sorted(Path(root).rglob('*')):
        if path.is_symlink():
            raise ValueError('linked candidate file')
        if path.is_file():
            result[path.relative_to(root).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    return result


def config_check(config, policy):
    expected = {'$schema', 'name', 'account_id', 'compatibility_date', 'workers_dev',
                'preview_urls', 'assets', 'observability', 'main', 'no_bundle'}
    if set(config) != expected or config['name'] != policy.DESTINATION['worker'] \
            or config['account_id'] != policy.ACCOUNT or config['workers_dev'] is not False \
            or config['preview_urls'] is not True or config['main'] != 'web/deploy/static-worker.js' \
            or config['no_bundle'] is not True or config['assets'] != {
                'directory': './_site', 'html_handling': 'auto-trailing-slash',
                'not_found_handling': '404-page', 'binding': 'ASSETS', 'run_worker_first': False}:
        raise ValueError('unexpected upload target, routes or asset settings')


def main():
    policy = load('cloudflare-candidate').policy()
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    config_check(json.loads(Path('wrangler.jsonc').read_text()), policy)
    receipt = Path('target/cloudflare-candidate')
    with tempfile.TemporaryDirectory(prefix='animatix-upload-') as temporary:
        work = Path(temporary)
        subprocess.run(['bash', 'scripts/cloudflare-attestation-tools.sh', str(work/'tools')], check=True)
        permit = policy.verify(receipt/'permit.json', receipt/'permit.sigstore.json',
                               '.github/cloudflare/trusted-root.jsonl',
                               work/'tools/gh_2.104.0_linux_amd64/bin/gh', commit)
        archive = receipt/'sdk.zip'
        if archive.stat().st_size != permit['sdk']['archive_size_bytes'] or \
                hashlib.sha256(archive.read_bytes()).hexdigest() != permit['sdk']['archive_sha256']:
            raise ValueError('archive differs from signed candidate')
        load('cloudflare-sdk-consumer').unpack(archive, work/'runtime')
        if tree(work/'runtime') != tree('_site/runtime'):
            raise ValueError('site SDK bytes differ from signed CI SDK')
        subprocess.run(['node', 'web/tools/sdk.mjs', 'verify', '_site/runtime', '--release',
                        '--current', '--commit', commit], check=True)
        subprocess.run(['python3', 'scripts/cloudflare-site-check.py', '_site'], check=True)
        manifest_path = Path('_site/_candidate.json')
        if manifest_path.exists():
            raise ValueError('refuse pre-existing candidate manifest')
        files = tree('_site')
        evidence = {'source_sha': commit, 'permit_sha256': hashlib.sha256((receipt/'permit.json').read_bytes()).hexdigest(),
                    'archive_sha256': permit['sdk']['archive_sha256'], 'files': files,
                    'release_tag': load('cloudflare-candidate').release_tag(permit),
                    'worker': policy.DESTINATION['worker']}
        manifest = (json.dumps(evidence, sort_keys=True, separators=(',', ':'))+'\n').encode()
        manifest_path.write_bytes(manifest)
        (receipt/'site.json').write_bytes(manifest)
        manifest_digest = hashlib.sha256(manifest).hexdigest()
    # Only this process receives the Builds-provisioned upload credential.
    # versions upload cannot activate the candidate. No mutable preview alias.
    if not os.environ.get('CLOUDFLARE_API_TOKEN'):
        raise RuntimeError('missing dedicated Builds upload credential')
    upload_env = {k: v for k, v in os.environ.items() if k in {
        'PATH', 'HOME', 'LANG', 'SSL_CERT_FILE', 'NODE_EXTRA_CA_CERTS',
        'HTTPS_PROXY', 'HTTP_PROXY', 'NO_PROXY'}}
    subprocess.run(['npm', '--prefix', 'web/deploy', 'ci', '--no-audit', '--no-fund'], check=True, env=upload_env)
    output = receipt/'wrangler-output.jsonl'
    if output.exists():
        raise RuntimeError('previous upload outcome exists; reconcile before retry')
    upload_env.update(CLOUDFLARE_API_TOKEN=os.environ['CLOUDFLARE_API_TOKEN'],
                      CLOUDFLARE_ACCOUNT_ID=policy.ACCOUNT,
                      WRANGLER_CI_GENERATE_PREVIEW_ALIAS='false',
                      WRANGLER_OUTPUT_FILE_PATH=str(output.resolve()))
    subprocess.run(['node', 'web/deploy/node_modules/wrangler/bin/wrangler.js', 'versions', 'upload',
                    '--config', 'wrangler.jsonc', '--tag', commit[:12],
                    '--message', 'Verified candidate ' + commit], check=True, env=upload_env)
    records = [json.loads(line) for line in output.read_text().splitlines() if line.strip()]
    versions = [r for r in records if r.get('type') == 'version-upload']
    if len(versions) != 1 or versions[0].get('worker_name') != policy.DESTINATION['worker']:
        raise RuntimeError('upload result target mismatch; do not accept candidate')
    version = versions[0]
    uuid.UUID(version['version_id'])
    evidence.update(version_id=version['version_id'], preview_url=version.get('preview_url'))
    (receipt/'uploaded.json').write_text(json.dumps(evidence, sort_keys=True) + '\n')
    summary = {k: v for k, v in evidence.items() if k != 'files'}
    summary['site_manifest_sha256'] = manifest_digest
    print('CANDIDATE_UPLOAD_RECEIPT ' + json.dumps(summary, sort_keys=True, separators=(',', ':')))
    print('CANDIDATE_UPLOADED_NOT_ACTIVATED', commit)


if __name__ == '__main__':
    main()
