#!/usr/bin/env python3
"""Execute instrumented shell bodies with fake tools, preserving gate contracts."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SOURCE = (ROOT / 'tools/ci/ci.sh').read_text()
BODIES = SOURCE[SOURCE.index('measure_phase() {'):SOURCE.index('gate_doc_links() {')]


class PhaseGates(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        for folder in ('tools/ci', 'tools/quality', 'bin', 'target/debug', 'web/content/scenes'):
            (self.root / folder).mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / 'tools/ci/phase-timing.py', self.root / 'tools/ci/phase-timing.py')
        shutil.copyfile(ROOT / 'tools/quality/audit_corpus.py', self.root / 'tools/quality/audit_corpus.py')
        cargo = self.root / 'bin/cargo'
        cargo.write_text('''#!/usr/bin/env python3
import json,os,sys
with open('cargo-calls.jsonl','a') as f:
    f.write(json.dumps({'args':sys.argv[1:],'rustdocflags':os.environ.get('RUSTDOCFLAGS')})+'\\n')
if sys.argv[1] == os.environ.get('FAIL_CARGO_COMMAND'):
    sys.exit(int(os.environ['FAIL_CODE']))
if sys.argv[1] == 'tree': print('animatix v0.1.0')
''')
        cargo.chmod(0o755)
        audit = self.root / 'target/debug/animatix'
        audit.write_text('''#!/usr/bin/env python3
import json,os,sys
if os.environ.get('FAIL_AUDIT'): sys.exit(9)
print(json.dumps({'schema_version':4,'kind':'audit','summary':{'errors':0}}))
''')
        audit.chmod(0o755)
        (self.root / 'web/content/scenes/space scene.amx').write_text('fixture')
        (self.root / 'tools/quality/audit-budget.sh').write_text('exit "${BUDGET_CODE:-0}"\n')
        self.env = {**os.environ, 'PATH': str(self.root/'bin')+os.pathsep+os.environ['PATH']}
        self.env.pop('ANIMATIX_PHASE_TIMINGS_DIR', None)

    def tearDown(self):
        self.tmp.cleanup()

    def run_gate(self, gate, **env):
        script = 'set -euo pipefail\nroot="$PWD"\nwant_cargo() { :; }\nwant_nix_shell() { :; }\n'+BODIES+'\ngate_'+gate.replace('-','_')
        return subprocess.run(['bash','-c',script], cwd=self.root, env={**self.env,**env},
                              capture_output=True,text=True)

    def calls(self):
        return [json.loads(line) for line in (self.root/'cargo-calls.jsonl').read_text().splitlines()]

    def records(self):
        return [json.loads(p.read_text()) for p in (self.root/'target/ci/phases').glob('*.json')]

    def test_canonical_gate_executes_collector_and_records_failure(self):
        shutil.copyfile(ROOT / 'tools/quality/audit-canonical.py', self.root / 'tools/quality/audit-canonical.py')
        scene = self.root / 'web/content/scenes/new.amx'
        scene.parent.mkdir(parents=True, exist_ok=True)
        scene.write_text('fixture')
        # The collector records repository identity, not fake build provenance.
        subprocess.run(['git', 'init', '-q', str(self.root)], check=True)
        subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                        'commit', '--allow-empty', '-qm', 'fixture'], cwd=self.root, check=True)
        result = self.run_gate('canonical-audit')
        # Existing mock omits warning/budget output: zero CLI exit must not pass.
        self.assertNotEqual(result.returncode, 0)
        summaries = list((self.root / 'target/ci/canonical-audit').glob('*/summary.json'))
        self.assertEqual(len(summaries), 1)
        self.assertEqual(json.loads(summaries[0].read_text())['passed'], 0)
        self.assertEqual([x['args'] for x in self.calls()], [['build', '--bin', 'animatix']])
        self.assertEqual({r['phase'] for r in self.records()}, {'build-cli', 'audit-scenes'})

    def test_no_video_retains_every_original_cargo_argument_and_order(self):
        result = self.run_gate('no-video')
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual([c['args'] for c in self.calls()], [
            ['build','-p','animatix-cli','-p','animatix-gui'],
            ['check','-p','animatix','--no-default-features'],
            ['tree','-p','animatix','--no-default-features','-e','normal','--prefix','none','--format','{p}'],
            ['tree','-e','features','-p','animatix-cli'],
            ['tree','-e','features','-p','animatix-gui'],
        ])
        self.assertEqual(len(self.records()),5)

    def test_cargo_failure_stops_gate_with_original_code(self):
        result = self.run_gate('no-video',FAIL_CARGO_COMMAND='build',FAIL_CODE='17')
        self.assertEqual(result.returncode,17,result.stderr)
        self.assertEqual(len(self.calls()),1)
        self.assertEqual(len(self.records()),1)

    def test_doc_preserves_rustdoc_flags_and_mixed_run(self):
        result = self.run_gate('doc',FAIL_CARGO_COMMAND='run',FAIL_CODE='19')
        self.assertEqual(result.returncode,19,result.stderr)
        calls = self.calls()
        self.assertEqual(calls[0]['args'],['doc','--workspace','--no-deps'])
        self.assertEqual(calls[0]['rustdocflags'],'-D warnings')
        self.assertEqual(calls[1]['args'],['run','-p','animatix','--example','property_reference','--','--check'])
        self.assertEqual({r['kind'] for r in self.records()},{'cargo','mixed'})

    def test_scene_audit_keeps_space_paths_and_original_failure_mapping(self):
        result = self.run_gate('scene-audit')
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertIn('OK scene-audit: web/content/scenes/space scene.amx',result.stdout)
        result = self.run_gate('scene-audit',FAIL_AUDIT='1')
        self.assertEqual(result.returncode,1,result.stderr)
        self.assertIn('scene-audit: one or more scene audits failed',result.stderr)
        self.assertEqual(len(self.records()),4)

    def test_budget_failure_and_build_are_separate(self):
        result = self.run_gate('budget-audit',BUDGET_CODE='23')
        self.assertEqual(result.returncode,23,result.stderr)
        self.assertEqual(self.calls()[0]['args'],['build','--bin','animatix'])
        self.assertEqual({r['kind'] for r in self.records()},{'cargo','execution'})

    def test_existing_matrix_order_and_artifact_upload_remain_explicit(self):
        row = next(line for line in SOURCE.splitlines() if line.startswith('Check & Lint|'))
        gates = row.split('|')[1].split()
        self.assertEqual(gates[:4], ['renderer-dependencies', 'scene-audit', 'budget-audit', 'canonical-audit'])
        self.assertIn('Security Audit|audit|nix|ubuntu-latest|nix|pr|security-audit',SOURCE)
        self.assertIn('target/ci/phases/*.json',(ROOT/'.github/workflows/ci.yml').read_text())


if __name__ == '__main__':
    unittest.main()
