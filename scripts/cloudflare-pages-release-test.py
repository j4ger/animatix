#!/usr/bin/env python3
"""Deterministic fault injection for the sole publisher; no network or tokens."""
import copy
import importlib.util
from pathlib import Path
import unittest

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
            self.result['deployment_trigger']['metadata']['commit_message'] = data['commit_message']
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

    def test_operator_adopts_receipt_without_send(self):
        api = Fake()
        api.state = {'schema': 1, 'phase': 'intent', 'sha': NEW, 'ticket': 'ticket', 'requested_at': 1}
        api.heads['deploy/pages'] = NEW
        r.adopt(api, 'deployment-1')
        self.assertEqual(api.state['phase'], 'waiting')
        self.assertEqual(api.sent, 0)

    def test_old_same_sha_receipt_cannot_unfreeze_unknown_send(self):
        api = Fake()
        api.state = {'schema': 1, 'phase': 'intent', 'sha': NEW, 'ticket': 'new-ticket', 'requested_at': 1}
        api.heads['deploy/pages'] = NEW
        with self.assertRaisesRegex(RuntimeError, 'ticket/time'):
            r.adopt(api, 'deployment-1')
        self.assertEqual(api.state['phase'], 'intent')
        api.result['deployment_trigger']['metadata']['commit_message'] = 'new-ticket'
        api.result['created_on'] = '1970-01-01T00:00:00Z'
        with self.assertRaisesRegex(RuntimeError, 'ticket/time'):
            r.adopt(api, 'deployment-1')
        self.assertEqual(api.sent, 0)

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
