#!/usr/bin/env python3
"""Read-only Pages upload preflight; SDK integrity remains a separate gate."""
import argparse
import json
import os
from pathlib import Path
import stat
import sys

MAX_FILE_BYTES = 25 * 1024 * 1024
MAX_FILES = 20_000


def check_site(root):
    errors = []
    root = Path(root)
    if root.is_symlink() or not root.is_dir():
        return ['upload root must be a real directory, not a symlink']
    for name in ('index.html', '404.html', 'runtime/runtime-manifest.json', 'site-manifest.json'):
        path = root / name
        if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
            errors.append(f'missing, empty or linked required file: {name}')
    for name in ('functions', '_worker.js'):
        path = root / name
        if path.exists() or path.is_symlink():
            errors.append(f'unexpected server code in static upload: {name}')
    manifest = root / 'site-manifest.json'
    if manifest.is_file() and not manifest.is_symlink():
        try:
            data = json.loads(manifest.read_text())
            if not isinstance(data, dict) or data.get('schema_version') != 1 or data.get('base') != '':
                errors.append('site manifest must describe schema 1 with root base (empty string)')
        except (OSError, ValueError) as exc:
            errors.append(f'invalid site manifest: {exc}')
    count = 0
    for directory, dirs, files in os.walk(root, followlinks=False,
                                          onerror=lambda exc: errors.append(f'cannot walk upload: {exc}')):
        for name in dirs + files:
            path = Path(directory) / name
            relative = path.relative_to(root)
            try:
                mode = path.lstat().st_mode
                if stat.S_ISLNK(mode):
                    errors.append(f'symlink is not deployable: {relative}')
                    continue
                if stat.S_ISDIR(mode):
                    continue
                if not stat.S_ISREG(mode):
                    errors.append(f'non-regular upload entry: {relative}')
                    continue
                count += 1
                if path.stat().st_size > MAX_FILE_BYTES:
                    errors.append(f'asset exceeds 25 MiB: {relative}')
                if relative.parts[0] == 'runtime' and path.suffix == '.gz':
                    with path.open('rb') as source:
                        if source.read(2) != b'\x1f\x8b':
                            errors.append(f'SDK gzip asset has no gzip signature: {relative}')
            except OSError as exc:
                errors.append(f'cannot inspect {relative}: {exc}')
    if count > MAX_FILES:
        errors.append(f'upload exceeds {MAX_FILES} files: {count}')
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    errors = check_site(args.output)
    if errors:
        print('\n'.join(errors), file=sys.stderr)
        return 1
    print('Cloudflare static upload preflight passed')
    return 0


if __name__ == '__main__':
    sys.exit(main())
