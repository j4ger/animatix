#!/usr/bin/env python3
"""Inventory compatibility and diagnostic-runner contracts (no real renderer)."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from audit_corpus import ROOT, inventory, scene_files

spec = importlib.util.spec_from_file_location('canonical', Path(__file__).with_name('audit-canonical.py'))
canonical = importlib.util.module_from_spec(spec)
spec.loader.exec_module(canonical)


class CorpusTests(unittest.TestCase):
    def test_legacy_matches_prior_find(self):
        env = os.environ.copy()
        env['LC_ALL'] = 'C'
        env['LANG'] = 'C'
        old = subprocess.check_output(['bash', '-c', """
find examples web/scenes web/demos -name '*.amx' -not -path '*/lib/*' \\
-not -path '*/_site/*' -not -path '*/plugin_pulse.amx' \\
-not -path '*/epicycles/fourier.amx' | sort | while IFS= read -r f; do
if [ -f "$(dirname "$f")/main.amx" ] && [ "$(basename "$f")" != main.amx ]; then continue; fi
printf '%s\\n' "$f"; done
"""], cwd=ROOT, text=True, env=env).splitlines()
        self.assertEqual(old, [p.relative_to(ROOT).as_posix() for p in scene_files()])

    def test_canonical_discovers_all_without_legacy_exclusions(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            folder = root / 'web/content/scenes'
            folder.mkdir(parents=True)
            for name in ['main.amx', 'extra.amx']:
                (folder / name).touch()
            self.assertEqual(len(scene_files(root, 'canonical')), 2)
            self.assertTrue(all(x['excluded'] is None for x in inventory(root, 'canonical')))

    def test_missing_and_empty_fail_closed(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(ValueError): inventory(Path(d), 'canonical')
            (Path(d) / 'web/content/scenes').mkdir(parents=True)
            with self.assertRaises(ValueError): inventory(Path(d), 'canonical')

    def test_all_excluded_fails_closed(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            for source in ('examples/lib', 'web/scenes', 'web/demos'):
                (root / source).mkdir(parents=True)
            (root / 'examples/lib/module.amx').touch()
            with self.assertRaisesRegex(ValueError, 'no eligible scenes'):
                inventory(root)

    def test_newline_paths_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            folder = root / 'web/content/scenes'
            folder.mkdir(parents=True)
            (folder / 'bad\nname.amx').touch()
            with self.assertRaisesRegex(ValueError, 'newline'):
                inventory(root, 'canonical')

    def test_timeout_preserves_partial_output(self):
        entry = inventory(corpus='canonical')[0]
        with tempfile.TemporaryDirectory() as d:
            binary = Path(d) / 'fake'
            binary.write_text('fake')
            output = Path(d) / 'run'
            error = subprocess.TimeoutExpired(['fake'], 180, output=b'{partial', stderr=b'stuck')
            with patch.object(canonical, 'inventory', return_value=[entry]), patch.object(canonical.subprocess, 'run', side_effect=error), patch.object(canonical.subprocess, 'check_output', return_value='sha'):
                self.assertEqual(canonical.collect(binary, output), 1)
            self.assertEqual(next(output.glob('*.stderr.txt')).read_bytes(), b'stuck')
            self.assertEqual((output / '00-compose.json').read_bytes(), b'{partial')

    def test_zero_exit_is_not_zero_debt(self):
        entry = inventory(corpus='canonical')[0]
        clean = {'schema_version': 3, 'kind': 'audit',
                 'summary': {'errors': 0, 'warnings': 0},
                 'budget': {'exceeded': [], 'updated': False}}
        mutations = [
            ('warning', {'summary': {'errors': 0, 'warnings': 1}}),
            ('missing-warning', {'summary': {'errors': 0}}),
            ('boolean-warning', {'summary': {'errors': 0, 'warnings': False}}),
            ('negative-warning', {'summary': {'errors': 0, 'warnings': -1}}),
            ('exceeded', {'budget': {'exceeded': [{'code': 'test'}], 'updated': False}}),
            ('missing-budget', {'budget': None}),
            ('missing-exceeded', {'budget': {'updated': False}}),
            ('updated', {'budget': {'exceeded': [], 'updated': True}}),
            ('missing-updated', {'budget': {'exceeded': []}}),
            ('clean', {}),
        ]
        with tempfile.TemporaryDirectory() as d:
            binary = Path(d) / 'fake'
            binary.write_text('fake')
            for name, mutation in mutations:
                with self.subTest(name=name):
                    result = subprocess.CompletedProcess([], 0, json.dumps(clean | mutation), '')
                    with patch.object(canonical, 'inventory', return_value=[entry]), patch.object(canonical.subprocess, 'run', return_value=result), patch.object(canonical.subprocess, 'check_output', return_value='sha'):
                        self.assertEqual(canonical.collect(binary, Path(d) / name),
                                         0 if name == 'clean' else 1)

    def test_changed_source_or_binary_fails(self):
        entry = inventory(corpus='canonical')[0]
        clean = {'schema_version': 3, 'kind': 'audit',
                 'summary': {'errors': 0, 'warnings': 0},
                 'budget': {'exceeded': [], 'updated': False}}
        for hashes in [('bin', 'src', 'changed', 'bin'), ('bin', 'src', 'src', 'changed')]:
            with self.subTest(hashes=hashes), tempfile.TemporaryDirectory() as d:
                result = subprocess.CompletedProcess([], 0, json.dumps(clean), '')
                with patch.object(canonical, 'inventory', return_value=[entry]), patch.object(canonical, 'sha256', side_effect=hashes), patch.object(canonical.subprocess, 'run', return_value=result), patch.object(canonical.subprocess, 'check_output', return_value='sha'):
                    self.assertEqual(canonical.collect(Path(d) / 'binary', Path(d) / 'run'), 1)

    def test_safe_progress_survives_interrupted_collection(self):
        entry = inventory(corpus='canonical')[0]
        with tempfile.TemporaryDirectory() as d:
            binary = Path(d) / 'fake'
            binary.write_text('fake')
            output = Path(d) / 'raw'
            summary = Path(d) / 'summary.json'
            with patch.object(canonical, 'inventory', return_value=[entry]), patch.object(canonical.subprocess, 'run', side_effect=KeyboardInterrupt):
                with self.assertRaises(KeyboardInterrupt):
                    canonical.collect(binary, output, summary)
            data = json.loads(summary.read_text())
            self.assertFalse(data['complete'])
            self.assertEqual(data['expected'], 1)
            self.assertEqual(data['completed'], 0)
            self.assertEqual(set(data), {'schema_version', 'complete', 'expected',
                'completed', 'passed', 'binary_sha256', 'binary_unchanged'})
            self.assertNotIn(str(binary), summary.read_text())

    def test_missing_stdout_cannot_pass_and_summary_has_no_raw_diagnostics(self):
        entry = inventory(corpus='canonical')[0]
        with tempfile.TemporaryDirectory() as d:
            binary = Path(d) / 'fake'
            binary.write_text('fake')
            summary = Path(d) / 'summary.json'
            result = subprocess.CompletedProcess([], 0, '', 'secret diagnostic')
            with patch.object(canonical, 'inventory', return_value=[entry]), patch.object(canonical.subprocess, 'run', return_value=result), patch.object(canonical.subprocess, 'check_output', return_value='sha'):
                self.assertEqual(canonical.collect(binary, Path(d) / 'raw', summary), 1)
            data = json.loads(summary.read_text())
            self.assertTrue(data['complete'])
            self.assertEqual(data['passed'], 0)
            self.assertNotIn('secret', summary.read_text())

    def test_ci_upload_is_only_public_summary(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        start = workflow.index('      - name: Upload canonical audit counters')
        end = workflow.index('      - name:', start + 10)
        block = workflow[start:end]
        self.assertIn('path: target/ci/canonical-audit/*/summary.json', block)
        self.assertNotIn('/raw', block)
        self.assertIn("if: always()", block)

    def test_collection_keeps_failure_and_never_updates_budget(self):
        entry = inventory(corpus='canonical')[0]
        with tempfile.TemporaryDirectory() as d:
            output = Path(d) / 'run'
            binary = Path(d) / 'fake'
            binary.write_text('fake binary')
            for payload in ['{', '[]', json.dumps({'schema_version':3,'kind':'audit','summary':{'errors':1}})]:
                result = subprocess.CompletedProcess([], 1, payload, 'real diagnostic')
                with patch.object(canonical, 'inventory', return_value=[entry]), patch.object(canonical.subprocess, 'run', return_value=result), patch.object(canonical.subprocess, 'check_output', return_value='test-sha\n'):
                    self.assertEqual(canonical.collect(binary, output), 1)
                    data = json.loads((output / 'manifest.json').read_text())
                    self.assertNotIn('--update-budget', data['records'][0]['command'])
                    self.assertEqual(json.loads((output / 'zero-budget.json').read_text())['files'], {})
                    self.assertEqual(next(output.glob('*.stderr.txt')).read_text(), 'real diagnostic')
                    with self.assertRaises(FileExistsError): canonical.collect(binary, output)
                import shutil
                shutil.rmtree(output)


if __name__ == '__main__': unittest.main()
