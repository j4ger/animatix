#!/usr/bin/env python3
"""Single audit inventory. Legacy gate coverage is deliberately unchanged."""
import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
LEGACY_ROOTS = ('examples', 'web/scenes', 'web/demos')
CANONICAL_ROOT = 'web/content/scenes'


def inventory(root=ROOT, corpus='legacy'):
    roots = LEGACY_ROOTS if corpus == 'legacy' else (CANONICAL_ROOT,)
    entries = []
    for source in roots:
        directory = root / source
        if not directory.is_dir():
            raise ValueError(f'missing audit source: {source}')
        for path in directory.rglob('*.amx'):
            rel = path.relative_to(root).as_posix()
            if '\n' in rel or '\r' in rel:
                raise ValueError(f'unsupported newline in scene path: {rel!r}')
            reason = None
            if corpus == 'legacy':
                if '/lib/' in rel or '/_site/' in rel:
                    reason = 'module or generated site copy'
                elif path.name == 'plugin_pulse.amx':
                    reason = 'requires demo plugin manifest'
                elif rel.endswith('/epicycles/fourier.amx'):
                    reason = 'pure constant module'
                elif path.name != 'main.amx' and (path.parent / 'main.amx').is_file():
                    reason = 'fragment driven by sibling main.amx'
            entries.append(dict(path=rel, source=source, excluded=reason,
                                purpose='canonical' if corpus == 'canonical' else
                                ('example' if source == 'examples' else 'legacy-regression')))
    if not any(item['excluded'] is None for item in entries):
        raise ValueError(f'no eligible scenes in audit corpus: {corpus}')
    return sorted(entries, key=lambda item: item['path'])


def scene_files(root=ROOT, corpus='legacy'):
    return [root / item['path'] for item in inventory(root, corpus) if not item['excluded']]


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--corpus', choices=('legacy', 'canonical'), default='legacy')
    parser.add_argument('--json', action='store_true', help='include exclusions and ownership')
    args = parser.parse_args()
    entries = inventory(corpus=args.corpus)
    if args.json:
        print(json.dumps(entries, indent=2))
    else:
        print('\n'.join(item['path'] for item in entries if not item['excluded']))
