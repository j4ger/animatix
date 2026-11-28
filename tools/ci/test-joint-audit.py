#!/usr/bin/env python3
"""Compare independent production audit gates with their explicit-list union."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('phase_tests', Path(__file__).with_name('test-phase-gates.py'))
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)
SOURCE = (ROOT / 'tools/ci/ci.sh').read_text()
PAIR = SOURCE[SOURCE.index('run_scene_budget_pair() {'):SOURCE.index('gate_names() {')]


class JointAudit(base.PhaseGates):
    # Inherited phase tests additionally cover unchanged standalone behavior.
    def setUp(self):
        super().setUp()
        shutil.copyfile(ROOT/'tools/quality/audit-budget.sh', self.root/'tools/quality/audit-budget.sh')
        self.env.update(ANIMATIX_GATE_CHILD='1', ANIMATIX_CI_SHELL='nix')
        for key in ('ANIMATIX_BIN', 'ANIMATIX_AUDIT_BUDGET', 'ANIMATIX_SCENE_AUDIT_JOBS'):
            self.env.pop(key, None)
        for name in ('web/content/scenes/single.amx', 'web/content/scenes/timing.amx',
                     'web/content/scenes/other.amx', 'examples/retired.amx',
                     'web/content/scenes/lib/module.amx'):
            path = self.root/name
            path.parent.mkdir(parents=True,exist_ok=True)
            path.write_text('fixture')
        (self.root/'target/debug/animatix').write_text('''#!/usr/bin/env python3
import json,os,sys
from pathlib import Path
with open('audit-calls.jsonl','a') as f: f.write(json.dumps(sys.argv[1:])+'\\n')
print('audit diagnostic',file=sys.stderr)
mode=os.environ.get('AUDIT_MODE','ok')
if os.environ.get('VALIDATE_BUDGET'):
    try:
        budget=json.loads(Path(sys.argv[sys.argv.index('--budget')+1]).read_text())
        policy=sys.argv[sys.argv.index('--policy')+1] if '--policy' in sys.argv else 'default'
        if budget.get('policy') != policy or budget.get('exceeded') or budget.get('errors',0): sys.exit(9)
    except Exception: sys.exit(9)
if os.environ.get('FAIL_AUDIT') or mode=='exit': sys.exit(9)
if mode=='malformed': print('{'); sys.exit(0)
if mode=='truncated': print('{"schema_version":4,'); sys.exit(0)
data={'schema_version':4,'kind':'audit','summary':{'errors':0}}
if mode=='budget-exceeded':
    data['findings']=[{'code':'actor.test','severity':'warning','actor':'leaf','message':'leaf needs attention'}]
    data['budget']={'exceeded':[{'code':'actor.test','count':1,'allowed':0}]}
    print(json.dumps(data))
    sys.exit(1)
if mode=='list': data=[]
if mode=='null': data=None
if mode=='summary': data['summary']=[]
if mode=='missing-summary': del data['summary']
if mode=='schema': data['schema_version']=2
if mode=='kind': data['kind']='other'
if mode=='errors': data['summary']['errors']=1
print(json.dumps(data))
''')

    def execute(self, command, **env):
        script = ('set -euo pipefail\nroot="$PWD"\nwant_cargo(){ :; }\n'
                  'want_nix_shell(){ :; }\n'+base.BODIES+PAIR+
                  '\nrun_gate(){ "gate_${1//-/_}"; }\n'+command)
        return subprocess.run(['bash','-c',script],cwd=self.root,env={**self.env,**env},capture_output=True,text=True)

    def audit_calls(self):
        path=self.root/'audit-calls.jsonl'
        return [json.loads(x) for x in path.read_text().splitlines()] if path.exists() else []

    def clear(self):
        for name in ('audit-calls.jsonl','cargo-calls.jsonl'):
            (self.root/name).unlink(missing_ok=True)
        shutil.rmtree(self.root/'target/ci',ignore_errors=True)

    def test_actual_repository_enumeration_closure(self):
        start=SOURCE.index('  scenes() {', SOURCE.index('gate_scene_audit() {'))
        stop=SOURCE.index('\n  }',start)+len('\n  }')
        scene_function=SOURCE[start:stop]
        budget_source=(ROOT/'tools/quality/audit-budget.sh').read_text()
        start=budget_source.index('scenes() {')
        stop=budget_source.index('\n}',start)+len('\n}')
        budget_function=budget_source[start:stop]
        def enumerate_with(function, root):
            result=subprocess.run(['bash','-u','-o','pipefail','-c',function+'\nscenes'],
                                  cwd=root,capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stderr)
            return result.stdout.splitlines()
        self.assertEqual(enumerate_with(scene_function,ROOT), enumerate_with(budget_function,ROOT))
        self.assertEqual(enumerate_with(scene_function,self.root),enumerate_with(budget_function,self.root))

    def test_union_matches_both_enumerators_and_explicit_options(self):
        legacy=self.execute('gate_scene_audit; gate_budget_audit')
        self.assertEqual(legacy.returncode,0,legacy.stderr)
        calls=self.audit_calls()
        expected={'web/content/scenes/space scene.amx', 'web/content/scenes/single.amx',
                  'web/content/scenes/timing.amx', 'web/content/scenes/other.amx'}
        self.assertEqual({c[1] for c in calls[:4]},expected)
        self.assertEqual({c[1] for c in calls[4:]},expected)
        self.assertEqual(len(calls),8)
        self.assertEqual(len(self.calls()),2)
        self.clear()
        joint=self.execute('run_explicit_gates scene-audit budget-audit')
        self.assertEqual(joint.returncode,0,joint.stderr)
        self.assertEqual(len(self.calls()),1)
        self.assertEqual({c[1] for c in self.audit_calls()},expected)
        self.assertEqual(len(self.audit_calls()),4)
        for call in self.audit_calls():
            self.assertEqual(call[0],'audit')
            options=dict(zip(call[2::2],call[3::2]))
            self.assertEqual(options,{'--budget':'tools/quality/audit-budget.json',
                                     '--render':'never','--policy':'default','--format':'json'})
        self.assertEqual(joint.stderr.count('audit diagnostic'),4)
        self.assertIn('OK gate: scene-audit\nOK gate: budget-audit',joint.stdout)
        self.assertEqual({r['gate'] for r in self.records()},{'scene-budget-audit'})

    def test_invalid_results_fail_both_without_budget_success(self):
        for mode in ('malformed','truncated','schema','kind','errors','exit','list','null','summary'):
            for command in ('gate_scene_audit; gate_budget_audit', 'run_explicit_gates scene-audit budget-audit'):
                with self.subTest(mode=mode,command=command):
                    self.clear()
                    result=self.execute(command,AUDIT_MODE=mode)
                    self.assertEqual(result.returncode,1,result.stderr)
                    self.assertNotIn('OK gate: budget-audit',result.stdout)
                    self.assertEqual(len(self.calls()),1)

    def test_failed_audit_keeps_status_and_actionable_report(self):
        for command in ('gate_scene_audit', 'run_explicit_gates scene-audit budget-audit'):
            with self.subTest(command=command):
                result = self.execute(command, AUDIT_MODE='budget-exceeded')
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn('FAIL scene-audit (exit 1):', result.stderr)
                self.assertIn('audit diagnostic', result.stderr)
                self.assertIn('audit finding: {"code": "actor.test"', result.stderr)
                self.assertIn('leaf needs attention', result.stderr)
                self.assertIn('audit budget: {"code": "actor.test", "count": 1, "allowed": 0}', result.stderr)

    def test_missing_summary_retains_legacy_default(self):
        for command in ('gate_scene_audit; gate_budget_audit',
                        'run_explicit_gates scene-audit budget-audit'):
            result=self.execute(command,AUDIT_MODE='missing-summary')
            self.assertEqual(result.returncode,0,result.stderr)

    def test_regular_file_stderr_is_not_reopened_or_truncated(self):
        script = ('set -euo pipefail\nroot="$PWD"\nwant_nix_shell(){ :; }\n'
                  +base.BODIES+PAIR+'\nrun_explicit_gates scene-audit budget-audit')
        path=self.root/'stderr.log'
        with path.open('w') as stream:
            stream.write('sentinel\n')
            stream.flush()
            result=subprocess.run(['bash','-c',script],cwd=self.root,env=self.env,
                                  stdout=subprocess.PIPE,stderr=stream,text=True)
        self.assertEqual(result.returncode,0,path.read_text())
        self.assertTrue(path.read_text().startswith('sentinel\n'))
        self.assertEqual(path.read_text().count('audit diagnostic'),4)

    def test_build_failure_is_failfast(self):
        result=self.execute('run_explicit_gates scene-audit budget-audit',FAIL_CARGO_COMMAND='build',FAIL_CODE='17')
        self.assertEqual(result.returncode,17)
        self.assertEqual(self.audit_calls(),[])
        self.assertNotIn('OK gate:',result.stdout)

    def test_overrides_disable_union(self):
        for overrides in ({'ANIMATIX_BIN':'target/debug/animatix'},
                          {'ANIMATIX_AUDIT_BUDGET':'tools/quality/audit-budget.json'}):
            self.clear()
            result=self.execute('run_explicit_gates scene-audit budget-audit',**overrides)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual(len(self.calls()),2)
            self.assertEqual(len(self.audit_calls()),8)
            self.assertEqual({r['gate'] for r in self.records()},{'scene-audit','budget-audit'})

    def test_nondefault_overrides_are_consumed_and_fail(self):
        alternative=self.root/'alternate-cli'
        alternative.write_text('#!/bin/sh\nprintf "alternate-called\\n" >&2\nexit 19\n')
        alternative.chmod(0o755)
        result=self.execute('run_explicit_gates scene-audit budget-audit',ANIMATIX_BIN='./alternate-cli')
        self.assertEqual(result.returncode,1)
        self.assertIn('alternate-called',result.stderr)
        self.assertEqual(len(self.calls()),2)
        self.clear()
        (self.root/'tools/quality/audit-budget.json').write_text('{"policy":"default"}')
        (self.root/'custom-budget.json').write_text('{"policy":"default","exceeded":true}')
        result=self.execute('run_explicit_gates scene-audit budget-audit',
                            ANIMATIX_AUDIT_BUDGET='custom-budget.json',VALIDATE_BUDGET='1')
        self.assertEqual(result.returncode,1)
        calls=self.audit_calls()
        self.assertEqual({c[c.index('--budget')+1] for c in calls[:4]},
                         {'tools/quality/audit-budget.json'})
        self.assertEqual({c[c.index('--budget')+1] for c in calls[4:]},{'custom-budget.json'})
        self.assertEqual(len(self.calls()),2)

    def test_budget_failure_matrix_matches_legacy_cli_contract(self):
        # These are fake-CLI contract fixtures, not tests of Rust budget semantics.
        path=self.root/'tools/quality/audit-budget.json'
        for payload in (None, '{', '{"policy":"different"}',
                        '{"policy":"default","exceeded":true}',
                        '{"policy":"default","errors":1,"allowed_errors":1}',
                        '{"policy":"default"}'):
            for command in ('gate_scene_audit; gate_budget_audit',
                            'run_explicit_gates scene-audit budget-audit'):
                with self.subTest(payload=payload,command=command):
                    self.clear()
                    if payload is None: path.unlink(missing_ok=True)
                    else: path.write_text(payload)
                    result=self.execute(command,VALIDATE_BUDGET='1')
                    self.assertEqual(result.returncode,0 if payload=='{"policy":"default"}' else 1)

    def test_production_dispatch_and_timing(self):
        for filename in ('ci.sh','ci-control.py'):
            shutil.copyfile(ROOT/'tools/ci'/filename,self.root/'tools/ci'/filename)
        for mode in ('ok','errors'):
            self.clear()
            env={**self.env,'IN_NIX_SHELL':'1','AUDIT_MODE':mode}
            env.pop('ANIMATIX_GATE_CHILD',None)
            env.pop('ANIMATIX_GATE_TIMINGS',None)
            result=subprocess.run(['bash','tools/ci/ci.sh','gate','scene-audit','budget-audit'],
                                  cwd=self.root,env=env,capture_output=True,text=True)
            self.assertEqual(result.returncode,0 if mode=='ok' else 1,result.stderr)
            records=[json.loads(line) for line in (self.root/'target/ci/timings.jsonl').read_text().splitlines()]
            self.assertEqual(len(records),1)
            self.assertEqual(records[0]['gate'],'scene-budget-audit')
            self.assertEqual(records[0]['exit_code'],result.returncode)
            self.assertEqual(len([c for c in self.calls() if c['args'][0]=='build']),1)
            self.assertEqual(len(self.audit_calls()),4)

    def test_all_dispatch_does_not_use_union_and_registry_is_unchanged(self):
        # Exercise the real all case with just its enumerator stubbed, avoiding
        # unrelated gates while keeping production dispatch and audit functions.
        dispatch=SOURCE[SOURCE.index('stage="${2:-}"'):]
        script=('set -euo pipefail\nroot="$PWD"\nwant_nix_shell(){ :; }\n'+base.BODIES+PAIR+
                '\nrun_gate(){ "gate_${1//-/_}"; }\n'
                'gate_names(){ printf "scene-audit\\nbudget-audit\\n"; }\n'+dispatch)
        result=subprocess.run(['bash','-c',script,'fixture','all','pr'],cwd=self.root,
                              env=self.env,capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual(len(self.calls()),2)
        self.assertEqual(len(self.audit_calls()),8)
        registry=SOURCE[SOURCE.index('GATE_TABLE='):SOURCE.index('# ── environment assertions')]
        self.assertNotIn('scene-budget-audit',registry)

    def test_multiple_sequences_and_reverse_order(self):
        result=self.execute('run_explicit_gates "scene-audit budget-audit" budget-audit scene-audit scene-audit budget-audit')
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual(len(self.calls()),4)
        self.assertEqual(len(self.audit_calls()),16)
        self.assertEqual(result.stdout.count('OK gate: budget-audit'),2)

    def test_update_stays_serial_independent(self):
        result=subprocess.run(['bash','tools/quality/audit-budget.sh','--update'],cwd=self.root,env=self.env,capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual(len(self.audit_calls()),4)
        self.assertTrue(all('--update-budget' in c for c in self.audit_calls()))
        self.assertFalse((self.root/'cargo-calls.jsonl').exists())

    def test_missing_input_directory_fails_union(self):
        shutil.rmtree(self.root/'web/content/scenes')
        result=self.execute('run_explicit_gates scene-audit budget-audit')
        self.assertEqual(result.returncode,1)
        self.assertNotIn('OK gate: budget-audit',result.stdout)

    # This inherited fixture uses a stub budget script; use the unchanged
    # standalone actual script's failing CLI instead in this integration suite.
    def test_budget_failure_and_build_are_separate(self):
        result=self.execute('gate_budget_audit',AUDIT_MODE='exit')
        self.assertEqual(result.returncode,1)
        self.assertEqual(len(self.calls()),1)


if __name__=='__main__':
    unittest.main()
