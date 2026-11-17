#!/usr/bin/env python3
"""Hermetic resource/identity tests, no full SDK compilation or network."""
import copy
import json
import os
from pathlib import Path
import re
import signal
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import probe_resources as resources
import splitter_probe as probe


class ManifestTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.identity = {'sha': 'current', 'run_id': '23', 'run_attempt': '1'}
        for name in probe.FILES:
            p = self.root / name
            p.parent.mkdir(exist_ok=True)
            p.write_bytes(b'\x00asm\x01\x00\x00\x00' if name.endswith('.wasm') else b'fixture')
        self.write_manifest()

    def tearDown(self):
        self.tmp.cleanup()

    def write_manifest(self):
        self.data = {'identity': copy.deepcopy(self.identity), 'files': {
            name: {'bytes': (self.root / name).stat().st_size, 'sha256': probe.digest(self.root / name)}
            for name in probe.FILES}}
        (self.root / 'manifest.json').write_text(json.dumps(self.data))

    def test_valid_inventory_and_current_identity(self):
        self.assertEqual(probe.input_manifest(self.root, self.identity), self.data)

    def test_wrong_sha_run_or_attempt_rejected(self):
        for key in self.identity:
            expected = {**self.identity, key: 'other'}
            with self.assertRaises(ValueError):
                probe.input_manifest(self.root, expected)

    def test_corrupt_file_rejected(self):
        (self.root / 'bindgen/animatix_web.js').write_text('changed')
        with self.assertRaises(ValueError):
            probe.input_manifest(self.root, self.identity)

    def test_extra_file_rejected(self):
        (self.root / 'unexpected').write_text('extra')
        with self.assertRaises(ValueError):
            probe.input_manifest(self.root, self.identity)

    def test_missing_file_rejected(self):
        (self.root / 'original.wasm').unlink()
        with self.assertRaises(ValueError):
            probe.input_manifest(self.root, self.identity)

    def test_symlink_rejected(self):
        p = self.root / 'original.wasm'
        p.unlink()
        p.symlink_to(self.root / 'bindgen/animatix_web_bg.wasm')
        with self.assertRaises(ValueError):
            probe.input_manifest(self.root, self.identity)

    def test_bad_header_rejected_even_with_matching_hash(self):
        (self.root / 'original.wasm').write_bytes(b'not wasm')
        self.write_manifest()
        with self.assertRaises(ValueError):
            probe.input_manifest(self.root, self.identity)

    def test_oversized_sparse_input_rejected_before_hashing(self):
        with (self.root / 'original.wasm').open('wb') as f:
            f.truncate(1024**3 + 1)
        with self.assertRaises(ValueError):
            probe.input_manifest(self.root, self.identity)


class ComparisonTests(unittest.TestCase):
    def test_failed_pair_is_recorded_without_repeating_or_claiming_success(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            evidence = root / 'evidence'
            evidence.mkdir()
            reports = [{'exit_code': -15, 'stop_reason': 'rss_guard'},
                       {'exit_code': 0, 'stop_reason': 'rss_guard'}]
            with patch.multiple(probe, OUT=root, INPUT=root / 'input', EVIDENCE=evidence):
                with patch.object(probe, 'identity', return_value={'sha': 'same'}), \
                     patch.object(probe, 'input_manifest', return_value={}), \
                     patch.object(probe, 'build_tools', return_value=root / 'tools'), \
                     patch.object(probe, 'run', side_effect=reports) as run:
                    with self.assertRaises(RuntimeError):
                        probe.compare()
            self.assertEqual(run.call_count, 2)
            result = json.loads((evidence / 'comparison.json').read_text())
            self.assertFalse(result['complete'])
            self.assertFalse(result['normalized_outputs_identical'])
            self.assertEqual(len(result['results']), 2)


class ResourceTests(unittest.TestCase):
    def test_success_and_nonzero_exit(self):
        with tempfile.TemporaryDirectory() as directory:
            for status in (0, 7):
                result = resources.run(f'exit{status}', [sys.executable, '-c', f'raise SystemExit({status})'], directory)
                self.assertEqual(result['exit_code'], status)
                self.assertIsNone(result['stop_reason'])

    def test_timeout_is_not_success_and_descendant_stops(self):
        with tempfile.TemporaryDirectory() as directory:
            pid = Path(directory) / 'pid'
            child = ('import subprocess,sys,time; p=subprocess.Popen([sys.executable,"-c","import time;time.sleep(60)"]);'
                     f'open({str(pid)!r},"w").write(str(p.pid));time.sleep(60)')
            result = resources.run('timeout', [sys.executable, '-c', child], directory, seconds=.4)
            self.assertEqual(result['stop_reason'], 'timeout')
            self.assertNotEqual(result['exit_code'], 0)
            status = Path('/proc') / pid.read_text() / 'stat'
            if status.exists():
                self.assertEqual(status.read_text().split(') ')[1].split()[0], 'Z')

    def test_rss_guard_is_not_oom_or_success(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(resources, 'group_rss_kib', return_value=2048):
            result = resources.run('rss', [sys.executable, '-c', 'import time;time.sleep(60)'], directory, rss_mib=1)
            self.assertEqual(result['stop_reason'], 'rss_guard')
            self.assertNotEqual(result['exit_code'], 0)

    def test_cancellation_forwarded_and_evidence_retained(self):
        with tempfile.TemporaryDirectory() as directory:
            code = (f'import sys;sys.path.insert(0,{str(Path(__file__).parent)!r});'
                    'from probe_resources import run;'
                    f'run("cancel",[sys.executable,"-c","import time;time.sleep(60)"],{directory!r})')
            p = subprocess.Popen([sys.executable, '-c', code], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            time.sleep(.5)
            p.send_signal(signal.SIGTERM)
            p.wait(timeout=8)
            self.assertNotEqual(p.returncode, 0)
            data = json.loads((Path(directory) / 'cancel.json').read_text())
            self.assertEqual(data['cancelled_signal'], signal.SIGTERM)

    def test_process_lookup_race_is_harmless(self):
        with patch.object(resources.os, 'killpg', side_effect=ProcessLookupError):
            resources.stop_group(type('Child', (), {'pid': 999999})())


class BindgenContractTests(unittest.TestCase):
    def test_real_bindgen_renames_original_to_expected_artifact_inventory(self):
        if not shutil.which('wasm-bindgen'):
            if os.environ.get('SPLITTER_PROBE_REAL_TOOLS') == '1':
                self.fail('real wasm-bindgen is required for the input job')
            self.skipTest('run in the web-build Nix shell for real bindgen coverage')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = root / 'original.wasm'
            original.write_bytes(b'\x00asm\x01\x00\x00\x00')
            output = root / 'bindgen'
            command = probe.bindgen_command(original, output)
            subprocess.run(command, check=True, capture_output=True, timeout=15)
            self.assertEqual({p.name for p in output.iterdir()},
                             {Path(name).name for name in probe.FILES if name.startswith('bindgen/')})


class WorkflowTests(unittest.TestCase):
    def test_pinned_readonly_two_job_same_run_artifact_contract(self):
        source = (probe.ROOT / '.github/workflows/splitter-resource-probe.yml').read_text()
        self.assertEqual(re.findall(r'^  (input|split):$', source, re.M), ['input', 'split'])
        self.assertIn('branches: [experiment/splitter-resource-20261117]', source)
        self.assertIn('permissions:\n  contents: read\n', source)
        for unsafe in ('secrets.', 'id-token:', 'contents: write', 'actions/cache@', 'rust-cache@',
                       'cachix-action@', 'workflow_dispatch:', 'pull_request:', 'workflow_run:'):
            self.assertNotIn(unsafe, source)
        self.assertEqual(source.count('persist-credentials: false'), 2)
        self.assertEqual(source.count('source-binary: ${{ runner.temp }}/splitter-nix-installer'), 2)
        self.assertIn('artifact-ids: ${{ needs.input.outputs.artifact_id }}', source)
        self.assertIn('needs: input', source)
        self.assertEqual(source.count('retention-days: 1'), 3)
        for reference in re.findall(r'uses: (\S+)', source):
            self.assertRegex(reference, r'@[0-9a-f]{40}$')

    def test_other_push_workflows_do_not_match_probe_branch(self):
        expected = {'ci.yml': 'branches: [main, feat/cloudflare-pages]',
                    'cloudflare-sdk-experiment.yml': 'branches: [experiment/cloudflare-native-build]',
                    'release.yml': 'tags:'}
        actual = {}
        for path in (probe.ROOT / '.github/workflows').glob('*.yml'):
            if path.name == 'splitter-resource-probe.yml':
                continue
            content = path.read_text()
            if re.search(r'^  push:', content, re.M):
                actual[path.name] = content
        self.assertEqual(set(actual), set(expected))
        for name, guard in expected.items():
            self.assertIn(guard, actual[name])

    def test_candidate_patch_only_changes_reviewed_vendor_files(self):
        paths = re.findall(r'^diff --git a/(\S+) b/\S+$', probe.PATCH.read_text(), re.M)
        self.assertEqual(set(paths), {'crates/vendor/wasm-split-cli/README.md',
                                     'crates/vendor/wasm-split-cli/src/lib.rs'})
        subprocess.run(['git', 'apply', '--check', str(probe.PATCH)], cwd=probe.ROOT, check=True)

    def test_experiment_gates_never_enter_production_matrix(self):
        matrix = json.loads(subprocess.check_output(['bash', 'tools/ci/ci.sh', '--matrix', 'all'], cwd=probe.ROOT))
        self.assertFalse(any('splitter-probe' in row['gate'] for row in matrix['include']))


if __name__ == '__main__':
    unittest.main()
