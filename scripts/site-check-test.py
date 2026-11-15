#!/usr/bin/env python3
"""Publication failures use isolated fixtures, never moving authored media."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

CHECK = Path(__file__).with_name('site-check.py').resolve()
def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

class SiteCheckTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / 'output'
        self.root.mkdir()
        self.source = Path(self.temp.name) / 'source'
        content = self.source / 'web/content'
        (content / 'scenes').mkdir(parents=True)
        works = []
        for i in range(10):
            (content / f'scenes/scene{i}.amx').write_text('config { resolution: (640, 360) }')
            if i < 3:
                item = {'id': f'scene{i}'}
                for role, name in [('source',f'scenes/scene{i}.amx'),('poster',f'poster{i}.png'),('recording',f'video{i}.mp4')]:
                    path = content / name
                    if not path.exists():
                        path.write_bytes(b'fixture')
                    item[role] = name
                    item[role+'_sha256'] = digest(path)
                works.append(item)
        (content / 'preview-manifest.json').write_text(json.dumps({'schema_version':1,'works':works}))

    def run_check(self, *args, base='/'):
        return subprocess.run(['python3',str(CHECK),str(self.root),'--source-root',str(self.source),*args],
                              capture_output=True, env=dict(os.environ,SITE_BASE=base))

    def pages(self):
        pages = ['index.html','showcase/index.html','learn/index.html','playground/index.html']
        pages += [f'lesson{i}/index.html' for i in range(9)]
        for name in pages + ['_astro/client.js']:
            path = self.root / name
            path.parent.mkdir(parents=True,exist_ok=True)
            path.write_text('<title>Test</title>' if name.endswith('.html') else 'export {};')
        return pages

    def assembly(self):
        pages = self.pages()
        result = self.run_check('--write-manifest')
        self.assertEqual(result.returncode,0,result.stderr)
        shutil.copytree(self.source/'web/content',self.root/'content')
        (self.root/'runtime').mkdir()
        for name in ('amx-player.js','runtime-manifest.json'):
            (self.root/'runtime'/name).write_text('fixture')
        return pages

    def test_empty_output_fails(self):
        result = self.run_check()
        self.assertNotEqual(result.returncode,0)
        self.assertIn(b'missing entry page',result.stderr)

    def test_missing_target_and_base_escape_fail(self):
        self.pages()
        result = self.run_check(base='/animatix/')
        self.assertEqual(result.returncode,0,result.stderr)
        (self.root/'index.html').write_text('<a href="/missing/">bad base</a><a href="/animatix/absent/">missing</a>')
        result = self.run_check(base='/animatix/')
        self.assertNotEqual(result.returncode,0)
        self.assertIn(b'URL escapes base',result.stderr)
        self.assertIn(b'missing target',result.stderr)

    def test_legacy_pages_cannot_mask_missing_generated_page(self):
        pages = self.assembly()
        result = self.run_check('--assembled')
        self.assertEqual(result.returncode,0,result.stderr)
        (self.root/pages[-1]).unlink()
        for i in range(20):
            (self.root/f'legacy{i}.html').write_text('<title>Legacy</title>')
        result = self.run_check('--assembled')
        self.assertNotEqual(result.returncode,0)
        self.assertIn(b'missing or changed generated file',result.stderr)

    def test_changed_bundle_and_absent_manifest_fail(self):
        self.assembly()
        (self.root/'_astro/client.js').write_text('changed')
        result = self.run_check('--assembled')
        self.assertNotEqual(result.returncode,0)
        self.assertIn(b'missing or changed generated file',result.stderr)
        (self.root/'site-manifest.json').unlink()
        result = self.run_check('--assembled')
        self.assertNotEqual(result.returncode,0)
        self.assertIn(b'invalid site manifest',result.stderr)

    def test_relative_url_cannot_escape_output(self):
        self.pages()
        (self.root/'index.html').write_text('<a href="../outside.html">escape</a>')
        result = self.run_check()
        self.assertNotEqual(result.returncode,0)
        self.assertIn(b'URL escapes output',result.stderr)

    def test_stale_teaching_still_fails(self):
        self.pages()
        content = self.source / 'web/content'
        manifest = content / 'preview-manifest.json'
        data = json.loads(manifest.read_text())
        still = dict(data['works'][0], id='lesson')
        del still['recording']
        del still['recording_sha256']
        data['stills'] = [still]
        manifest.write_text(json.dumps(data))
        self.assertEqual(self.run_check().returncode, 0)
        (content / still['source']).write_text('changed source')
        result = self.run_check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b'stale or missing preview source: lesson', result.stderr)

    def test_stale_preview_fails(self):
        self.assembly()
        (self.root/'content/poster0.png').write_bytes(b'changed')
        result = self.run_check('--assembled')
        self.assertNotEqual(result.returncode,0)
        self.assertIn(b'stale or missing preview poster',result.stderr)

if __name__ == '__main__':
    unittest.main()
