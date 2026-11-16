#!/usr/bin/env python3
"""Deterministic fault injection for the sole publisher; no network or tokens."""
import copy
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location('release', Path(__file__).with_name('cloudflare-pages-release.py'))
r = importlib.util.module_from_spec(spec)
spec.loader.exec_module(r)
OLD, NEW = 'a'*40, 'b'*40


def deployment(status='success', sha=NEW):
    return {'created_on': '2099-01-01T00:00:00Z', 'id': 'deployment-1', 'environment': 'production', 'is_skipped': False,
            'deployment_trigger': {'metadata': {'commit_hash': sha, 'branch': 'deploy/pages', 'commit_message': 'ticket'}},
            'latest_stage': {'status': status}}


class Fake:
    def __init__(self):
        self.state = {'schema': 1, 'phase': 'ready'}
        self.rev = 0
        self.heads = {'main': NEW, 'deploy/pages': OLD}
        self.sent = 0
        self.fail_send = False
        self.fail_receipt = False
        self.records = []
        self.result = deployment()
        self.project = {'production_branch': 'deploy/pages', 'source': {'config': {
            'production_branch': 'deploy/pages', 'production_deployments_enabled': False,
            'preview_deployment_setting': 'none'}}, 'canonical_deployment': {'id': 'deployment-1'}}

    def cf(self, path='', method='GET', data=None):
        if not path:
            return self.project
        if method == 'POST':
            self.sent += 1
            if self.fail_send:
                raise TimeoutError('lost response')
        return self.result

    def gh(self, path, method='GET', data=None):
        if method == 'PATCH':
            self.heads['deploy/pages'] = data['sha']
            return {}
        return {'status': 'ahead'}

    def head(self, branch):
        return self.heads[branch]

    def journal(self):
        return copy.deepcopy(self.state), self.rev

    def save(self, state, rev):
        if rev != self.rev:
            raise RuntimeError('CAS rejected')
        if self.fail_receipt and state['phase'] == 'waiting':
            raise TimeoutError('lost state write')
        self.state = copy.deepcopy(state)
        self.rev += 1
        return self.rev

    def deployments(self):
        return self.records


class InventoryTests(unittest.TestCase):
    def test_real_api_page_size_and_pagination(self):
        api = r.API()
        api.cf = Mock(side_effect=[[{'id': str(i)} for i in range(25)], [{'id': 'last'}]])
        self.assertEqual(len(api.deployments()), 26)
        self.assertEqual([c.args[0] for c in api.cf.call_args_list], [
            '/deployments?per_page=25&page=1', '/deployments?per_page=25&page=2'])

    def test_full_page_requires_next_empty_page(self):
        api = r.API()
        api.cf = Mock(side_effect=[[{}] * 25, []])
        self.assertEqual(len(api.deployments()), 25)
        self.assertEqual(api.cf.call_count, 2)

    def test_failure_or_truncation_never_means_idle(self):
        api = r.API()
        api.cf = Mock(side_effect=[[{}] * 25, RuntimeError('request failed')])
        with self.assertRaises(RuntimeError):
            api.deployments()
        api.cf = Mock(return_value=[{}] * 25)
        with self.assertRaisesRegex(RuntimeError, 'exceeds bound'):
            api.deployments()
        self.assertEqual(api.cf.call_count, 100)
        for malformed in ({'result': []}, [{}] * 26):
            api.cf = Mock(return_value=malformed)
            with self.assertRaisesRegex(RuntimeError, 'invalid deployment inventory'):
                api.deployments()


class InitializeTests(unittest.TestCase):
    def api(self, exists=False):
        api = Fake()
        api.project['canonical_deployment'] = deployment(sha=OLD)
        api.state = {'schema': 1, 'phase': 'complete', 'sha': OLD, 'deployment_id': 'deployment-1'}
        api.gh = Mock(return_value=[{'ref': 'refs/heads/' + r.STATE_BRANCH}] if exists else [])
        api.save = Mock()
        return api

    def test_new_journal_never_deploys(self):
        api = self.api()
        r.initialize(api)
        api.save.assert_called_once_with(api.state, None)
        self.assertEqual(api.sent, 0)
        self.assertEqual(api.head('deploy/pages'), OLD)
        self.assertEqual(api.gh.call_args_list[1].args, ('git/refs', 'POST', {
            'ref': 'refs/heads/' + r.STATE_BRANCH, 'sha': NEW}))

    def test_repeat_is_read_only(self):
        api = self.api(True)
        r.initialize(api)
        api.save.assert_not_called()
        self.assertEqual(api.gh.call_count, 1)
        self.assertEqual(api.sent, 0)

    def test_drift_unknown_and_incomplete_freeze(self):
        for phase in ('intent', 'waiting', 'blocked', 'preparing'):
            api = self.api(True)
            api.state['phase'] = phase
            with self.assertRaises(RuntimeError):
                r.initialize(api)
            api.save.assert_not_called()
        api = self.api(True)
        api.journal = Mock(side_effect=RuntimeError('missing journal'))
        with self.assertRaises(RuntimeError):
            r.initialize(api)
        api.save.assert_not_called()

    def test_active_or_read_failure_prevents_writes(self):
        api = self.api()
        api.records = [deployment('active', OLD)]
        with self.assertRaises(RuntimeError):
            r.initialize(api)
        api.gh.assert_not_called()
        api = self.api()
        api.gh.side_effect = RuntimeError('permission denied')
        with self.assertRaises(RuntimeError):
            r.initialize(api)
        api.save.assert_not_called()

    def test_wrong_event_or_ref_rejected_before_api(self):
        for env in ({}, {'GITHUB_REPOSITORY': r.REPO, 'GITHUB_REF': 'refs/heads/other',
                        'GITHUB_EVENT_NAME': 'workflow_dispatch'}):
            with patch.dict(r.os.environ, env, clear=True), patch.object(r.sys, 'argv', ['release', '--initialize']):
                with patch.object(r, 'initialize') as initialize:
                    with self.assertRaises(RuntimeError):
                        r.main()
                    initialize.assert_not_called()


class ReleaseTests(unittest.TestCase):
    def test_one_send_success_and_duplicate(self):
        api = Fake()
        r.release(api, NEW)
        self.assertEqual(api.state['phase'], 'complete')
        r.release(api, NEW)
        self.assertEqual(api.sent, 1)

    def test_unknown_post_never_retried(self):
        api = Fake()
        api.fail_send = True
        with self.assertRaises(TimeoutError):
            r.release(api, NEW)
        self.assertEqual(api.state['phase'], 'intent')
        with self.assertRaisesRegex(RuntimeError, 'unknown POST'):
            r.release(api, NEW)
        self.assertEqual(api.sent, 1)

    def test_receipt_save_failure_freezes(self):
        api = Fake()
        api.fail_receipt = True
        with self.assertRaises(TimeoutError):
            r.release(api, NEW)
        with self.assertRaisesRegex(RuntimeError, 'unknown POST'):
            r.release(api, NEW)
        self.assertEqual(api.sent, 1)

    def test_waiting_survives_new_runner(self):
        api = Fake()
        api.state = {'schema': 1, 'phase': 'waiting', 'sha': NEW, 'deployment_id': 'deployment-1', 'ticket': 'ticket', 'requested_at': 1}
        r.release(api, NEW)
        self.assertEqual(api.sent, 0)
        self.assertEqual(api.state['phase'], 'complete')

    def test_failed_deployment_stops_future(self):
        api = Fake()
        api.result = deployment('failure')
        with self.assertRaises(RuntimeError):
            r.release(api, NEW)
        self.assertEqual(api.state['phase'], 'blocked')
        with self.assertRaises(RuntimeError):
            r.release(api, NEW)
        self.assertEqual(api.sent, 1)

    def test_stale_result_cannot_roll_back(self):
        api = Fake()
        r.release(api, OLD)
        self.assertEqual(api.sent, 0)

    def test_other_build_blocks(self):
        api = Fake()
        api.records = [deployment('active')]
        with self.assertRaises(RuntimeError):
            r.release(api, NEW)
        self.assertEqual(api.sent, 0)

    def test_partial_ref_update_can_resume_same_only(self):
        api = Fake()
        api.state = {'schema': 1, 'phase': 'preparing', 'sha': NEW, 'previous_sha': OLD}
        api.heads['deploy/pages'] = NEW
        r.release(api, OLD)
        self.assertEqual(api.sent, 0)
        r.release(api, NEW)
        self.assertEqual(api.sent, 1)

    def test_wrong_sha_or_canonical_freezes(self):
        for wrong in ('sha', 'canonical'):
            api = Fake()
            if wrong == 'sha':
                api.result = deployment(sha=OLD)
            else:
                api.project['canonical_deployment'] = {'id': 'other'}
            with self.assertRaises(RuntimeError):
                r.release(api, NEW)
            self.assertNotEqual(api.state['phase'], 'complete')

    def test_same_sha_success_cannot_unfreeze_unknown_post(self):
        api = Fake()
        api.state = {'schema': 1, 'phase': 'intent', 'sha': NEW, 'requested_at': 1}
        api.heads['deploy/pages'] = NEW
        api.records = [deployment()]
        with self.assertRaisesRegex(RuntimeError, 'unknown POST'):
            r.release(api, NEW)
        self.assertEqual(api.state['phase'], 'intent')
        self.assertEqual(api.sent, 0)
        self.assertFalse(hasattr(r, 'adopt'))

    def test_operator_recovery_needs_known_terminal_failure(self):
        api = Fake()
        api.state = {'schema': 1, 'phase': 'blocked', 'sha': NEW, 'deployment_id': 'deployment-1', 'ticket': 'ticket', 'requested_at': 1}
        with self.assertRaises(RuntimeError):
            r.recover_failed(api)
        api.result = deployment('failure')
        r.recover_failed(api)
        self.assertEqual(api.state['phase'], 'ready')
        self.assertEqual(api.sent, 0)

    def test_automatic_deployments_must_stay_off(self):
        api = Fake()
        api.project['source']['config']['production_deployments_enabled'] = True
        with self.assertRaises(RuntimeError):
            r.release(api, NEW)
        self.assertEqual(api.sent, 0)


if __name__ == '__main__':
    unittest.main()
