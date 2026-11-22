#!/usr/bin/env python3
"""Check the generated site and its public scenes."""
import argparse
import hashlib
import json
import os
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[3]

class Links(HTMLParser):
    def __init__(self):
        super().__init__()
        self.urls = []
        self.icons = []
        self.choices = []
    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "link" and "icon" in attrs.get("rel", "").split():
            self.icons.append(attrs.get("href", ""))
        if tag == 'option' and 'data-source' in attrs:
            self.choices.append((attrs.get('value'), attrs['data-source']))
        for name in ('src', 'href', 'poster', 'data-recording', 'data-source', 'data-module'):
            if name in attrs:
                self.urls.append(attrs[name])

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('output', type=Path)
    ap.add_argument('--source-root', type=Path, default=ROOT, help='canonical source checkout (also permits isolated fixtures)')
    ap.add_argument('--assembled', action='store_true')
    ap.add_argument('--write-manifest', action='store_true', help='record verified static output')
    args = ap.parse_args()
    output = args.output.resolve()
    source_root = args.source_root.resolve()
    base = '/' + os.environ.get('SITE_BASE', '/').strip('/')
    base = base.rstrip('/')
    errors = []
    if args.assembled and args.write_manifest:
        ap.error('--assembled and --write-manifest are mutually exclusive')
    content = output / 'content' if args.assembled else source_root / 'web/content'
    manifest = content / 'preview-manifest.json'
    try:
        previews = json.loads(manifest.read_text())
        works = previews['works']
        if previews.get('schema_version') != 1 or len(works) < 3:
            errors.append('preview manifest must describe at least three works with schema_version 1')
        ids = set()
        for work in works + previews.get('stills', []):
            if work['id'] in ids:
                errors.append(f"duplicate preview id: {work['id']}")
            ids.add(work['id'])
            roles = ('source', 'poster', 'recording') if work in works else ('source', 'poster')
            for role in roles:
                path = (content / work[role]).resolve()
                if not path.is_relative_to(content.resolve()):
                    errors.append(f"preview path escapes content: {work[role]}")
                    continue
                if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != work[role + '_sha256']:
                    errors.append(f"stale or missing preview {role}: {work['id']}")
    except (OSError, ValueError, KeyError, TypeError) as exc:
        errors.append(f'invalid preview manifest: {exc}')
    scenes = sorted((source_root / 'web/content/scenes').glob('*.amx'))
    if not scenes:
        errors.append('public scene inventory is empty')
    expected_choices = {scene.stem: f'{base}/content/scenes/{scene.name}' for scene in scenes}
    if args.assembled:
        deployed_names = {scene.name for scene in (content / 'scenes').glob('*.amx')}
        for name in sorted(deployed_names - {scene.name for scene in scenes}):
            errors.append(f'unlisted deployed source: {name}')
    for scene in scenes:
        if not scene.read_text().strip():
            errors.append(f'empty canonical scene: {scene.name}')
        if args.assembled:
            deployed = output / 'content/scenes' / scene.name
            if not deployed.is_file() or deployed.read_bytes() != scene.read_bytes():
                errors.append(f'missing or stale deployed source: {scene.name}')
    # A missing output must never pass by checking zero pages.
    playground_pages = ('playground/index.html', 'zh/playground/index.html')
    for page in ('index.html', 'showcase/index.html', 'learn/index.html', *playground_pages):
        if not (output / page).is_file():
            errors.append(f'missing entry page: {page}')
    pages = sorted(output.rglob('*.html'))
    if args.assembled:
        # The installed artifact must be verifiable without a source build tree.
        # Legacy pages must not mask missing generated routes or JS/CSS bundles.
        try:
            built = json.loads((output / 'site-manifest.json').read_text())
            if built.get('schema_version') != 1 or built.get('base') != base:
                errors.append('site manifest schema/base does not match requested deployment')
            files = built['files']
            if not isinstance(files, dict) or not files:
                raise ValueError('generated-file inventory is empty')
            pages = []
            for name, digest in files.items():
                target = (output / name).resolve()
                if not target.is_relative_to(output) or name == 'site-manifest.json':
                    errors.append(f'invalid generated-file path: {name}')
                    continue
                if not target.is_file() or hashlib.sha256(target.read_bytes()).hexdigest() != digest:
                    errors.append(f'missing or changed generated file: {name}')
                    continue
                if target.suffix == '.html':
                    pages.append(target)
        except (OSError, ValueError, KeyError, TypeError, AttributeError) as exc:
            errors.append(f'invalid site manifest: {exc}')
            pages = []
    if len(pages) < 13:
        errors.append(f'expected at least 13 generated pages, found {len(pages)}')
    for page in pages:
        parser = Links()
        parser.feed(page.read_text())
        if page.relative_to(output).as_posix() in playground_pages:
            choices = dict(parser.choices)
            if len(choices) != len(parser.choices) or choices != expected_choices:
                errors.append(f'{page.relative_to(output)}: scene choices must match the public AMX sources')
        for icon in parser.icons:
            if icon != base + '/logo.svg':
                errors.append(f'{page.relative_to(output)}: favicon must reference deployment logo: {icon}')
        for raw in parser.urls:
            url = urlsplit(raw)
            if url.scheme or url.netloc or not url.path:
                continue
            path = unquote(url.path)
            if path.startswith('/'):
                if base and not (path == base or path.startswith(base + '/')):
                    errors.append(f'{page.relative_to(output)}: URL escapes base: {raw}')
                    continue
                rel = path[len(base):].lstrip('/')
                target = output / rel
            else:
                target = page.parent / path
                rel = str(target.relative_to(output))
            if not target.resolve().is_relative_to(output):
                errors.append(f'{page.relative_to(output)}: URL escapes output: {raw}')
                continue
            if not args.assembled and rel.startswith('runtime/'):
                continue  # independently verified SDK; required in final assembly
            if not args.assembled and (rel.startswith('content/') or rel == 'logo.svg'):
                target = source_root / 'web' / rel
            if target.is_dir():
                target = target / 'index.html'
            if not target.is_file():
                errors.append(f'{page.relative_to(output)}: missing target: {raw}')
    if args.assembled:
        # Keep retired pages and regression sources out of the public artifact.
        for retired in ('archive', 'demos', 'tour', 'recipes', 'gallery.html'):
            route = output / retired
            if route.is_file() or any(route.rglob('*.html')):
                errors.append(f'retired page in site artifact: {retired}')
        for retired in ('demos', 'tour', 'recipes', 'scenes'):
            for scene in (output / retired).rglob('*.amx'):
                errors.append(f'retired source in site artifact: {scene.relative_to(output)}')
        if (output / 'dev').exists():
            errors.append('developer tools in site artifact: dev')
        for name in ('amx-player.js', 'runtime-manifest.json'):
            if not (output / 'runtime' / name).is_file():
                errors.append(f'missing runtime/{name}')
    if errors:
        raise SystemExit('\n'.join(errors))
    if args.write_manifest:
        files = {str(path.relative_to(output)): hashlib.sha256(path.read_bytes()).hexdigest()
                 for path in sorted(output.rglob('*'))
                 if path.is_file() and path.name != 'site-manifest.json'}
        (output / 'site-manifest.json').write_text(json.dumps({
            'schema_version': 1, 'base': base, 'files': files,
        }, indent=2, sort_keys=True) + '\n')
    print(f'site-check: {len(pages)} HTML files; {len(scenes)} nonempty canonical scenes; links valid')

if __name__ == '__main__':
    main()
