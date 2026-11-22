#!/usr/bin/env python3
"""Inventory the public scene entry points shared by native quality checks."""
import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CANONICAL_ROOT = 'web/content/scenes'


def inventory(root=ROOT):
    entries = []
    for path in sorted((root / CANONICAL_ROOT).glob('*.amx')):
        if not path.is_file():
            continue
        rel = path.relative_to(root).as_posix()
        if '\n' in rel or '\r' in rel:
            raise ValueError(f'unsupported newline in scene path: {rel!r}')
        entries.append(dict(path=rel, source=CANONICAL_ROOT, purpose='canonical'))
    if not entries:
        raise ValueError(f'no public scenes in {CANONICAL_ROOT}')
    return entries


def scene_files(root=ROOT):
    return [root / item['path'] for item in inventory(root)]


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--json', action='store_true', help='report the source inventory as JSON')
    args = parser.parse_args()
    entries = inventory()
    if args.json:
        print(json.dumps(entries, indent=2))
    else:
        print('\n'.join(item['path'] for item in entries))
