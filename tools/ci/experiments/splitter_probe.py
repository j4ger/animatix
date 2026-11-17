#!/usr/bin/env python3
"""Isolated same-run input and splitter A/B, never a deployable SDK build."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import urllib.request

from probe_resources import checked, run

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'target/splitter-probe'
EVIDENCE = OUT / 'evidence'
INPUT = OUT / 'input'
PATCH = Path(__file__).with_name('splitter-lifetime.patch')
BRANCH = 'refs/heads/experiment/splitter-resource-20261117'
FILES = {'original.wasm', 'bindgen/animatix_web_bg.wasm', 'bindgen/animatix_web.js',
         'bindgen/animatix_web.d.ts', 'bindgen/animatix_web_bg.wasm.d.ts'}
FLAGS = '-C target-feature=+simd128 -C link-arg=--emit-relocs -C debuginfo=2'


def digest(path):
    result = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024**2), b''):
            result.update(chunk)
    return result.hexdigest()


def command(*args):
    return subprocess.check_output(args, text=True).strip()


def identity():
    return {'schema': 1, 'repository': os.environ['GITHUB_REPOSITORY'],
            'sha': os.environ['GITHUB_SHA'], 'run_id': os.environ['GITHUB_RUN_ID'],
            'run_attempt': os.environ['GITHUB_RUN_ATTEMPT'],
            'ref': os.environ['GITHUB_REF'], 'lock_sha256': digest(ROOT / 'Cargo.lock'),
            'flake_sha256': digest(ROOT / 'flake.lock'), 'patch_sha256': digest(PATCH),
            'common_splitter_sha256': digest(ROOT / 'crates/vendor/wasm-split-cli/src/lib.rs'),
            'rustc': command('rustc', '-vV'), 'cargo': command('cargo', '-V'),
            'wasm_bindgen': command('wasm-bindgen', '--version'),
            'profile': 'wasm-dev: baseline opt-level1/debug=true/fat-LTO/CGU1',
            'rustflags': FLAGS}


def authorize_environment():
    if os.environ.get('GITHUB_REPOSITORY') != 'j4ger/animatix' or os.environ.get('GITHUB_REF') != BRANCH:
        raise RuntimeError('diagnostic only runs on its exact authorized branch')
    if command('git', 'rev-parse', 'HEAD') != os.environ['GITHUB_SHA']:
        raise RuntimeError('checkout is not the triggering commit')
    if command('git', 'status', '--porcelain', '--untracked-files=no'):
        raise RuntimeError('tracked checkout must start clean')
    for name in os.environ:
        if name == 'RUSTFLAGS' or name == 'CARGO_ENCODED_RUSTFLAGS' or name.startswith('CARGO_PROFILE_'):
            raise RuntimeError('unexpected compiler override: ' + name)
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    mem = dict(line.split(':', 1) for line in Path('/proc/meminfo').read_text().splitlines())
    inventory = {'mem_total_kib': int(mem['MemTotal'].split()[0]),
                 'mem_available_kib': int(mem['MemAvailable'].split()[0]),
                 'swap_total_kib': int(mem['SwapTotal'].split()[0]),
                 'swap_free_kib': int(mem['SwapFree'].split()[0]),
                 'disk_free_bytes': shutil.disk_usage(ROOT).free,
                 'cpu_count': os.cpu_count(), 'image_os': os.environ.get('ImageOS'),
                 'image_version': os.environ.get('ImageVersion'), 'identity': identity()}
    (EVIDENCE / 'environment.json').write_text(json.dumps(inventory, indent=2) + '\n')
    if inventory['mem_available_kib'] < 13 * 1024**2:
        raise RuntimeError('requires at least 13 GiB available RAM before experiment')


def input_manifest(directory, expected):
    directory = Path(directory)
    actual = set()
    for path in directory.rglob('*'):
        if path.is_symlink():
            raise ValueError('artifact contains a symlink')
        if path.is_file():
            actual.add(path.relative_to(directory).as_posix())
    if actual != FILES | {'manifest.json'}:
        raise ValueError('artifact file inventory mismatch')
    if (directory / 'manifest.json').stat().st_size > 16384:
        raise ValueError('manifest exceeds bound')
    data = json.loads((directory / 'manifest.json').read_text())
    if data.get('identity') != expected or set(data.get('files', {})) != FILES:
        raise ValueError('artifact identity or manifest mismatch')
    total = 0
    for relative in sorted(FILES):
        path = directory / relative
        size = path.stat().st_size
        limit = 1024**3 if relative == 'original.wasm' else 256 * 1024**2
        if size > limit or size == 0:
            raise ValueError('artifact file exceeds input bounds')
        total += size
        if data['files'][relative] != {'bytes': size, 'sha256': digest(path)}:
            raise ValueError('artifact digest mismatch')
        if relative.endswith('.wasm'):
            with path.open('rb') as stream:
                if stream.read(8) != b'\x00asm\x01\x00\x00\x00':
                    raise ValueError('invalid Wasm input header')
    if total > 1536 * 1024**2:
        raise ValueError('artifact aggregate size exceeded')
    return data


def bindgen_command(original, output):
    return ['wasm-bindgen', '--no-demangle', '--target', 'web', '--out-name',
            'animatix_web', '--out-dir', str(output), str(original)]


def build_input():
    expected = identity()
    INPUT.mkdir(parents=True)
    env = {**os.environ, 'CARGO_BUILD_JOBS': '2', 'CARGO_TARGET_DIR': str(OUT / 'build'),
           'RUSTFLAGS': FLAGS}
    runtime_identity = json.loads(subprocess.check_output(
        ['node', 'web/tools/sdk.mjs', 'identity', 'dev'], text=True, env=env))
    runtime_id = runtime_identity['build_id']
    (EVIDENCE / 'runtime-identity.json').write_text(json.dumps(runtime_identity, indent=2) + '\n')
    env.update(ANIMATIX_RUNTIME_BUILD_ID=runtime_id, ANIMATIX_GIT_SHA=expected['sha'])
    checked('compile-input', ['cargo', 'build', '--locked', '-p', 'animatix-web', '--target',
                             'wasm32-unknown-unknown', '--profile', 'wasm-dev'], EVIDENCE,
            rss_mib=10240, seconds=3600, env=env)
    original = OUT / 'build/wasm32-unknown-unknown/wasm-dev/animatix_web.wasm'
    if original.stat().st_size > 1024**3 or shutil.disk_usage(ROOT).free < original.stat().st_size + 3 * 1024**3:
        raise RuntimeError('insufficient disk margin for bounded input artifact')
    shutil.copy2(original, INPUT / 'original.wasm')
    checked('bindgen', bindgen_command(INPUT / 'original.wasm', INPUT / 'bindgen'), EVIDENCE,
            rss_mib=6144, seconds=300)
    data = {'identity': expected, 'runtime_build_id': runtime_id, 'files': {name: {'bytes': (INPUT / name).stat().st_size,
                                                  'sha256': digest(INPUT / name)} for name in FILES}}
    (INPUT / 'manifest.json').write_text(json.dumps(data, indent=2) + '\n')
    input_manifest(INPUT, expected)
    (EVIDENCE / 'input-manifest.json').write_text(json.dumps(data, indent=2) + '\n')


def build_tools():
    tools = OUT / 'tools'
    tools.mkdir()
    env = {**os.environ, 'CARGO_BUILD_JOBS': '2', 'CARGO_TARGET_DIR': str(OUT / 'host')}
    sources = {}
    for variant in ('baseline', 'candidate'):
        if variant == 'candidate':
            if command('git', 'status', '--porcelain', '--untracked-files=no'):
                raise RuntimeError('baseline build changed tracked source')
            checked('patch-check', ['git', 'apply', '--check', str(PATCH)], EVIDENCE)
            checked('patch-apply', ['git', 'apply', str(PATCH)], EVIDENCE)
        checked(f'build-{variant}', ['cargo', 'build', '--locked', '-p', 'animatix-web-build'],
                EVIDENCE, rss_mib=6144, seconds=1200, env=env)
        shutil.copy2(OUT / 'host/debug/animatix-web-build', tools / variant)
        sources[variant] = {name: digest(ROOT / name) for name in (
            'crates/vendor/wasm-split-cli/README.md', 'crates/vendor/wasm-split-cli/src/lib.rs')}
        if variant == 'candidate':
            checked('candidate-reverse-check', ['git', 'apply', '--reverse', '--check', str(PATCH)], EVIDENCE)
    checked('candidate-unit-tests', ['cargo', 'test', '--locked', '-p', 'wasm-split-cli',
                                    '--lib'], EVIDENCE, rss_mib=6144, seconds=600, env=env)
    changed = set(command('git', 'diff', '--name-only').splitlines())
    allowed = {'crates/vendor/wasm-split-cli/README.md', 'crates/vendor/wasm-split-cli/src/lib.rs',
               }
    if changed != allowed:
        raise RuntimeError('candidate touched unexpected tracked files')
    (EVIDENCE / 'source-identity.json').write_text(json.dumps({'variants': sources,
        'common_commit': command('git', 'rev-parse', 'HEAD'),
        'candidate_patch_sha256': digest(PATCH)}, indent=2) + '\n')
    (EVIDENCE / 'tools.json').write_text(json.dumps({v: digest(tools / v) for v in ('baseline', 'candidate')}, indent=2))
    return tools


def output_hashes(directory):
    files = sorted(Path(directory).iterdir())
    if not files or not any(p.suffix == '.wasm' for p in files):
        raise ValueError('splitter produced no Wasm')
    if any(not p.is_file() or p.is_symlink() or p.suffix not in {'.wasm', '.js', '.json'} for p in files):
        raise ValueError('unexpected splitter output')
    return {p.name: {'bytes': p.stat().st_size, 'sha256': digest(p)} for p in files}


def wasm_sections(path):
    """Bounded streaming section evidence; not a semantic equivalence proof."""
    path = Path(path)
    size = path.stat().st_size
    if size > 256 * 1024**2:
        raise ValueError('split module exceeds evidence bound')
    result = []
    with path.open('rb') as stream:
        if stream.read(8) != b'\0asm\1\0\0\0':
            raise ValueError('bad Wasm header')
        while stream.tell() < size:
            if len(result) >= 1024:
                raise ValueError('too many Wasm sections')
            offset = stream.tell()
            section_id = stream.read(1)[0]
            length = 0
            for shift in range(0, 35, 7):
                byte = stream.read(1)
                if not byte:
                    raise ValueError('truncated section size')
                value = byte[0]
                if shift == 28 and value > 15:
                    raise ValueError('invalid u32 section size')
                length |= (value & 127) << shift
                if value < 128:
                    break
            else:
                raise ValueError('invalid section size')
            if length > size - stream.tell():
                raise ValueError('truncated section body')
            sha = hashlib.sha256()
            remaining = length
            while remaining:
                chunk = stream.read(min(1024**2, remaining))
                if not chunk:
                    raise ValueError('truncated section body')
                sha.update(chunk)
                remaining -= len(chunk)
            result.append({'id': section_id, 'offset': offset, 'bytes': length,
                           'sha256': sha.hexdigest()})
    return result


def section_evidence(directory):
    return {p.name: wasm_sections(p) for p in sorted(directory.glob('*.wasm'))}


def compare():
    expected = identity()
    manifest = input_manifest(INPUT, expected)
    (EVIDENCE / 'input-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    tools = build_tools()
    results = []
    env = {**os.environ, 'RAYON_NUM_THREADS': '1'}
    failed = False
    for pair, order in enumerate((('baseline', 'candidate'), ('candidate', 'baseline')), 1):
        for variant in order:
            label = f'{variant}-{pair}'
            output = OUT / label
            report = run(label, [str(tools / variant), 'split', str(INPUT / 'original.wasm'),
                                 str(INPUT / 'bindgen'), str(output)], EVIDENCE,
                         rss_mib=12288, seconds=600, env=env)
            results.append(report)
            (EVIDENCE / 'comparison.json').write_text(json.dumps({'scope': 'split-only-not-SDK',
                'input_identity': expected, 'results': results, 'complete': False}, indent=2) + '\n')
            if report['exit_code'] == 0 and report['stop_reason'] is None:
                report['raw_outputs'] = output_hashes(output)
                report['raw_sections'] = section_evidence(output)
                (EVIDENCE / f'{label}-raw-sections.json').write_text(json.dumps(report['raw_sections'], indent=2) + '\n')
                for module in sorted(output.glob('*.wasm')):
                    checked(f'validate-{label}-{module.stem}',
                            ['wasm-opt', str(module), '-Oz', '--strip-debug', '-o', str(module)],
                            EVIDENCE, rss_mib=4096, seconds=180)
                report['normalized_outputs'] = output_hashes(output)
                report['normalized_sections'] = section_evidence(output)
                (EVIDENCE / f'{label}-normalized-sections.json').write_text(json.dumps(report['normalized_sections'], indent=2) + '\n')
            else:
                failed = True
            (EVIDENCE / 'comparison.json').write_text(json.dumps({'scope': 'split-only-not-SDK',
                'input_identity': expected, 'results': results, 'complete': False}, indent=2) + '\n')
            if output.exists():
                shutil.rmtree(output)  # only this run's regenerable output
        if failed:
            break  # no repeat of a failed/guarded pair
    complete = not failed and len(results) == 4
    identical = complete and all(r['normalized_outputs'] == results[0]['normalized_outputs'] for r in results)
    summary = {'scope': 'split-only-not-SDK', 'input_identity': expected, 'results': results,
               'complete': complete, 'normalized_outputs_identical': bool(identical),
               'browser_tested': False, 'sdk_manifest_verified': False}
    (EVIDENCE / 'comparison.json').write_text(json.dumps(summary, indent=2) + '\n')
    if not complete or not identical:
        raise RuntimeError('comparison incomplete or normalized outputs differ')


def prepare_installer():
    # The action's default downloader writes an installer into GitHub cache.
    # A verified source-binary bypasses that downloader/cache completely.
    if os.environ.get('GITHUB_REPOSITORY') != 'j4ger/animatix' or os.environ.get('GITHUB_REF') != BRANCH:
        raise RuntimeError('wrong repository/ref for installer')
    if command('git', 'rev-parse', 'HEAD') != os.environ['GITHUB_SHA']:
        raise RuntimeError('installer checkout mismatch')
    url = ('https://github.com/DeterminateSystems/nix-installer/releases/download/'
           'v3.24.2/nix-installer-x86_64-linux')
    path = Path(os.environ['RUNNER_TEMP']) / 'splitter-nix-installer'
    with urllib.request.urlopen(url, timeout=60) as response, path.open('xb') as output:
        total = 0
        while chunk := response.read(1024**2):
            total += len(chunk)
            if total > 90 * 1024**2:
                raise RuntimeError('installer exceeded size bound')
            output.write(chunk)
    if digest(path) != '22ed14ee1b24d100d388f1fc245e3062bd5982433d159cffa6dec34c0eb15dab':
        raise RuntimeError('installer checksum mismatch')
    path.chmod(0o755)


def main():
    os.chdir(ROOT)
    if sys.argv[1:] == ['installer']:
        prepare_installer()
        return
    authorize_environment()
    if sys.argv[1:] == ['input']:
        build_input()
    elif sys.argv[1:] == ['compare']:
        compare()
    else:
        raise ValueError('expected input or compare')


if __name__ == '__main__':
    main()
