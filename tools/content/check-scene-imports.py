#!/usr/bin/env python3
"""Validate real AMX imports without an engine-embedded library allowlist."""
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
IMPORT = re.compile(r'^\s*import\s+"([^"\n]+)"', re.M)


def missing_imports(root, sources):
    root = root.resolve()
    for name in sorted(sources):
        source = root / name
        if not source.is_file():
            continue  # a tracked deletion must not keep obsolete sources alive
        for specifier in IMPORT.findall(source.read_text()):
            target = (source.parent / specifier).resolve()
            if not target.is_relative_to(root) or not target.is_file():
                yield f"{name}: unresolved repository import {specifier}"


def main():
    names = subprocess.check_output(
        ['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z', '--', '*.amx'],
        cwd=ROOT,
    ).decode().split('\0')
    sources = {name for name in names if name}
    errors = list(missing_imports(ROOT, sources))
    if errors:
        print('\n'.join(errors), file=sys.stderr)
        return 1
    print(f'scene imports: checked {sum((ROOT / p).is_file() for p in sources)} AMX sources')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
