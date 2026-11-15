#!/usr/bin/env python3
"""Regressions reported by independent review; no remote writes or real credentials."""
import io
import os
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

import importlib.util
spec = importlib.util.spec_from_file_location('fixtures', Path(__file__).with_name('cloudflare-control-test.py'))
f = importlib.util.module_from_spec(spec)
spec.loader.exec_module(f)


class ReviewTests(unittest.TestCase):
    def test_unsigned_zero_binding_fetch_is_not_static_proof(self):
        def arbitrary_handler(*args, **kwargs):
            return {'id': f.VERSION, 'resources': {'bindings': [], 'script': {'handlers': ['fetch']}, 'script_runtime': {}}}
        with self.assertRaises(ValueError):
            f.a.version_check(arbitrary_handler, f.CONFIG, f.VERSION)

    def test_first_dispatch_may_follow_an_earlier_failed_ci_attempt(self):
        env = dict(GITHUB_ACTIONS='true', GITHUB_SHA=f.SHA, GITHUB_REF='refs/heads/'+f.c.BRANCH,
                   GITHUB_REPOSITORY='j4ger/animatix', GITHUB_RUN_ID='123', GITHUB_RUN_ATTEMPT='2',
                   CLOUDFLARE_EXPERIMENT_TOKEN='test-only')
        with patch.dict(os.environ, env), patch.object(sys, 'argv', ['dispatch']), \
                patch.object(f.d.c, 'target', return_value=f.CONFIG), \
                patch.object(f.d.subprocess, 'check_output', return_value=f.SHA), \
                patch.object(f.d.c, 'API'), patch.object(f.d, 'check_prior_dispatch', create=True), \
                patch.object(f.d, 'dispatch', return_value={'build_uuid': f.BUILD_ID, 'receipt': {'version_id': f.VERSION}}) as start, \
                patch('sys.stdout', new_callable=io.StringIO):
            f.d.main()
            start.assert_called_once()

    def test_permit_release_is_namespaced_by_issuance_not_only_sha(self):
        workflow = Path('.github/workflows/ci.yml').read_text()
        self.assertIn('cf-preview-${GITHUB_SHA}-r${GITHUB_RUN_ID}-a${GITHUB_RUN_ATTEMPT}', workflow)

    def history(self, jobs):
        def read(path):
            if '/jobs?' in path:
                page = int(path.rsplit('=', 1)[1])
                return {'total_count': len(jobs), 'jobs': jobs[(page-1)*100:page*100]}
            return dict(id=123, run_attempt=int(path.rsplit('/', 1)[1]), head_sha=f.SHA,
                        head_branch=f.c.BRANCH, event='push', path='.github/workflows/ci.yml',
                        status='completed', repository={'id': 1185977116})
        return read

    def job(self, identifier=1, dispatch=False, conclusion='failure', steps=None):
        return dict(id=identifier, run_id=123, head_sha=f.SHA, status='completed', conclusion=conclusion,
                    name='Dispatch isolated Cloudflare candidate' if dispatch else 'CI', steps=steps or [])

    def test_authenticated_history_allows_only_proven_nonexecution(self):
        for jobs in [[self.job()], [self.job(dispatch=True, conclusion='skipped')],
                     [self.job(dispatch=True, steps=[{'name': 'Trigger exact-SHA candidate build after publication',
                                                     'status': 'completed', 'conclusion': 'skipped'}])]]:
            f.d.check_prior_dispatch('123', '3', f.SHA, self.history(jobs))
        for conclusion in ['success', 'failure', 'cancelled', None]:
            jobs = [self.job(dispatch=True, steps=[{'name': 'Trigger exact-SHA candidate build after publication',
                                                   'status': 'completed', 'conclusion': conclusion}])]
            with self.assertRaises(ValueError):
                f.d.check_prior_dispatch('123', '2', f.SHA, self.history(jobs))
        for jobs in [[], [self.job(dispatch=True)], [self.job(), self.job()]]:
            with self.assertRaises(ValueError):
                f.d.check_prior_dispatch('123', '2', f.SHA, self.history(jobs))

    def test_history_pagination_does_not_miss_prior_dispatch(self):
        jobs = [self.job(i) for i in range(100)]+[self.job(100, dispatch=True)]
        with self.assertRaises(ValueError):
            f.d.check_prior_dispatch('123', '2', f.SHA, self.history(jobs))

    def test_same_sha_renewal_preserves_sdk_and_expiry(self):
        candidate = f.c.load('cloudflare-candidate')
        fixture = f.c.load('cloudflare-permit-test')
        policy = candidate.policy()
        metadata = dict(fixture.metadata(), ref=policy.REF, workflow=policy.WORKFLOW)
        old = policy.emit(metadata, fixture.NOW)
        new = policy.emit(dict(metadata, run_attempt=metadata['run_attempt']+1), fixture.NOW+policy.TTL+1)
        self.assertEqual(old['sdk'], new['sdk'])
        self.assertNotEqual(candidate.release_tag(old), candidate.release_tag(new))
        with self.assertRaises(policy.PermitError):
            policy.validate_permit(old, fixture.SHA, fixture.NOW+policy.TTL+1)
        refs = [{'ref': 'refs/tags/'+candidate.release_tag(p), 'object': {'type': 'commit', 'sha': fixture.SHA}}
                for p in [old, new]]
        self.assertEqual(candidate.release_tags(fixture.SHA, refs)[0], candidate.release_tag(new))
        refs[0]['object']['sha'] = 'f'*40
        with self.assertRaises(ValueError):
            candidate.release_tags(fixture.SHA, refs)

    def test_late_admission_uses_only_authenticated_finished_window(self):
        api, data, permit = f.AdmissionTests().fixture()
        calls = []
        def verified(*args, **kwargs):
            calls.append(kwargs['validation_time'])
            return permit
        with patch.object(f.a.time, 'time', return_value=f.NOW+10000):
            f.a.admit(api, f.CONFIG, f.BUILD_ID, '/pinned/gh', fetch=lambda u, n: data[u.rsplit('/', 1)[1]],
                      check_http=lambda *args: None, verify_signature=verified)
        self.assertEqual(calls, [f.NOW-80])
        api.build['stopped_on'] = f.iso(permit['expires_at']+1)
        with self.assertRaises(ValueError):
            f.a.admit(api, f.CONFIG, f.BUILD_ID, '/pinned/gh', fetch=lambda u, n: data[u.rsplit('/', 1)[1]],
                      check_http=lambda *args: None, verify_signature=verified)
        version = {'created_on': f.iso(f.NOW-90)}
        for start, end in [(f.NOW-89, f.NOW-80), (f.NOW-100, f.NOW-91), (f.NOW-100, f.NOW+1)]:
            with self.assertRaises(ValueError):
                f.a.admission_window({'running_on': f.iso(start), 'stopped_on': f.iso(end)}, version, now=f.NOW)

    def test_exact_module_bytes_and_asset_routing_required(self):
        api = f.FakeAPI()
        valid = f.a.version_check(api, f.CONFIG, f.VERSION)
        for field in ['modules', 'assets', 'bindings']:
            bad = dict(valid)
            bad.pop(field)
            with self.assertRaises(ValueError):
                f.a.version_check(lambda *args, **kwargs: bad, f.CONFIG, f.VERSION)
        valid['modules'][0]['content_base64'] = f.base64.b64encode(b'export default {fetch(){return fetch("https://example.com")}}').decode()
        with self.assertRaises(ValueError):
            f.a.version_check(lambda *args, **kwargs: valid, f.CONFIG, f.VERSION)

    def test_candidate_jobs_override_implicit_ancestor_success(self):
        # GitHub propagates skips through the ancestor chain by default. These
        # jobs must rely on their explicit required-result gate and still cancel.
        workflow = Path('.github/workflows/ci.yml').read_text()
        for job, dependency, switch in [('cloudflare_candidate', 'required', 'ANIMATIX_CF_CANDIDATE'),
                                         ('cloudflare_candidate_dispatch', 'cloudflare_candidate', 'ANIMATIX_CF_PREVIEW_DISPATCH')]:
            block = workflow.split('  '+job+':', 1)[1]
            condition = next(line.strip() for line in block.splitlines() if line.strip().startswith('if:'))
            self.assertIn('!cancelled()', condition)
            self.assertIn("needs."+dependency+".result == 'success'", condition)
            self.assertIn("vars."+switch+" == 'enabled'", condition)


if __name__ == '__main__':
    unittest.main()
