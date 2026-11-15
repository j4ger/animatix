#!/usr/bin/env python3
"""Fault-injection tests for the real dispatcher, admission path and coordinator."""
import base64
import datetime
import time
import copy
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import os
import sys
import unittest
from unittest.mock import patch
import zipfile


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name+'.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


c = load('cloudflare-control')
r = load('cloudflare-release-coordinator')
d = load('cloudflare-candidate-dispatch')
a = load('cloudflare-candidate-admission')
CONFIG = dict(enabled=True, account_id='65bc5958d8a01d7a1d9b7c326b244406',
              worker='animatix-candidate-preview', worker_tag='a'*32,
              trigger_uuid='10000000-0000-4000-8000-000000000001',
              build_token_uuid='10000000-0000-4000-8000-000000000002',
              repo_connection_uuid='06316b53-a8a0-4a6f-ace9-14ca28076d74', workers_subdomain='test')
BUILD_ID = '10000000-0000-4000-8000-000000000003'
VERSION = '20000000-0000-4000-8000-000000000001'
SHA = 'a'*40
BASELINE = '30000000-0000-4000-8000-000000000001'
NOW = int(time.time())
def iso(seconds):
    return datetime.datetime.fromtimestamp(seconds, datetime.timezone.utc).isoformat()


def trigger():
    return dict(trigger_uuid=CONFIG['trigger_uuid'], external_script_id=CONFIG['worker_tag'],
                build_command=c.BUILD, deploy_command=c.UPLOAD, root_directory='/',
                branch_includes=[c.BRANCH], branch_excludes=[], build_token_uuid=CONFIG['build_token_uuid'],
                repo_connection=dict(repo_connection_uuid=CONFIG['repo_connection_uuid'], repo_id='1185977116',
                                     repo_name='animatix', provider_account_name='j4ger', provider_type='github'))


def build():
    return dict(status='stopped', build_outcome='success', trigger=trigger(),
                running_on=iso(NOW-100), stopped_on=iso(NOW-80),
                build_trigger_metadata=dict(commit_hash=SHA, branch=c.BRANCH, build_command=c.BUILD,
                     deploy_command=c.UPLOAD, build_token_uuid=CONFIG['build_token_uuid'], root_directory='/',
                     provider_account_name='j4ger', repo_name='animatix', provider_type='github'))


class FakeAPI:
    def __init__(self):
        self.calls = []
        self.deployments = [dict(id=BASELINE, strategy='percentage', versions=[{'version_id': VERSION, 'percentage': 100}], annotations={})]
        self.mode = 'success'
        self.deferred = None
        self.build = build()
        self.trigger = trigger()
        self.receipt = dict(source_sha=SHA, worker=CONFIG['worker'], version_id=VERSION)
        self.version_bindings = [{'type': 'assets', 'name': 'ASSETS'}]

    def apply(self, body):
        deployment = dict(body, id=f'30000000-0000-4000-8000-{len(self.deployments)+1:012d}')
        self.deployments.insert(0, deployment)
        return deployment

    def __call__(self, method, path, body=None, query=None):
        self.calls.append((method, path, copy.deepcopy(body)))
        if method == 'GET' and path.endswith('/triggers'):
            return [self.trigger]
        if method == 'GET' and path.endswith('/environment_variables'):
            return {k: {'value': v, 'is_secret': False} for k, v in [('NODE_VERSION', '24.14.0'), ('SKIP_DEPENDENCY_INSTALL', '1')]}
        if method == 'POST' and path.endswith('/builds'):
            if self.mode == 'timeout':
                raise TimeoutError('simulated lost response')
            return {'build_uuid': BUILD_ID}
        if method == 'GET' and path.endswith('/logs'):
            return {'truncated': False, 'lines': [[0, c.RECEIPT_MARKER+json.dumps(self.receipt)]]}
        if method == 'GET' and '/builds/builds/' in path:
            return self.build
        if method == 'GET' and '/versions/' in path:
            return {'id': path.rsplit('/', 1)[1], 'bindings': self.version_bindings, 'created_on': iso(NOW-90),
                    'assets': {'config': {'html_handling': 'auto-trailing-slash', 'not_found_handling': '404-page', 'run_worker_first': False}},
                    'main_module': 'static-worker.js', 'modules': [{'name': 'static-worker.js',
                    'content_type': 'application/javascript+module',
                    'content_base64': base64.b64encode(Path('web/deploy/static-worker.js').read_bytes()).decode()}]}
        if path.endswith('/deployments'):
            if method == 'GET':
                return {'deployments': copy.deepcopy(self.deployments)}
            if method == 'POST':
                if self.mode == 'timeout-before-apply':
                    self.deferred = body
                    raise TimeoutError('request may still be in flight')
                result = self.apply(body)
                if self.mode == 'timeout-after-apply':
                    raise TimeoutError('response was lost')
                return result
        if method == 'GET' and '/deployments/' in path:
            return next(x for x in self.deployments if x['id'] == path.rsplit('/', 1)[1])
        raise AssertionError((method, path))


class CoordinatorTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.directory = Path(self.tmp.name)
        self.api = FakeAPI()
        self.store = r.Store(self.directory, CONFIG)
        self.addCleanup(lambda: self.store.close())
        self.coordinator = r.Coordinator(self.store, self.api, enabled=True)
        self.coordinator.initialize(acknowledged=True)
        self.a = self.register(VERSION, SHA)
        self.b = self.register('20000000-0000-4000-8000-000000000002', 'b'*40)

    def register(self, version, sha):
        evidence = {'version_id': version, 'source_sha': sha, 'worker': CONFIG['worker']}
        digest = hashlib.sha256(c.canonical(evidence)).hexdigest()
        self.store.register(digest, evidence)
        return digest

    def posts(self):
        return [call for call in self.api.calls if call[0] == 'POST']

    def test_real_cli_init_select_activate_status_with_fake_transport(self):
        with tempfile.TemporaryDirectory() as tmp:
            api = FakeAPI()
            api.deployments = []  # A newly provisioned Worker has no active version.
            base = ['coordinator', '--state-directory', tmp]
            with patch.object(r.c, 'target', return_value=CONFIG), patch.object(r.c, 'API', return_value=api), \
                    patch.dict(os.environ, {'CLOUDFLARE_PREVIEW_COORDINATOR_TOKEN': 'test-only',
                                           'ANIMATIX_CF_PREVIEW_ACTIVATION': 'enabled'}):
                with patch.object(sys, 'argv', base+['init', '--new-single-writer-installation']):
                    r.main()
                store = r.Store(Path(tmp), CONFIG)
                evidence = {'version_id': VERSION, 'source_sha': SHA}
                digest = hashlib.sha256(c.canonical(evidence)).hexdigest()
                store.register(digest, evidence)
                store.close()
                with patch('sys.stdout', new_callable=io.StringIO) as output:
                    with patch.object(sys, 'argv', base+['select', '--expected-epoch', '0', '--candidate', digest,
                                                       '--reviewed-browser-and-receipt']):
                        r.main()
                    with patch.object(sys, 'argv', base+['activate', '--epoch', '1', '--candidate', digest]):
                        r.main()
                    with patch.object(sys, 'argv', base+['status']):
                        r.main()
                    self.assertIn('PREVIEW_ACTIVATED', output.getvalue())
                    self.assertNotIn('test-only', output.getvalue())
                store = r.Store(Path(tmp), CONFIG)
                self.assertEqual(store.state()['active_candidate'], digest)
                store.close()
                self.assertEqual(sum(call[0] == 'POST' for call in api.calls), 1)

    def test_old_completion_never_selects_or_overwrites_b(self):
        self.coordinator.select(0, self.a)
        self.coordinator.select(1, self.b)
        with self.assertRaises(r.Blocked):
            self.coordinator.activate(1, self.a)
        self.assertEqual(self.posts(), [])
        self.coordinator.activate(2, self.b)
        with self.assertRaises(r.Blocked):
            self.coordinator.activate(2, self.b)
        self.assertEqual(len(self.posts()), 1)
        self.assertEqual(self.store.state()['active_candidate'], self.b)

    def test_unknown_outcome_survives_restart_and_waits_for_late_apply(self):
        self.coordinator.select(0, self.a)
        self.api.mode = 'timeout-before-apply'
        with self.assertRaises(TimeoutError):
            self.coordinator.activate(1, self.a)
        self.store.close()
        self.store = r.Store(self.directory, CONFIG)
        self.coordinator = r.Coordinator(self.store, self.api, enabled=True)
        with self.assertRaises(r.Blocked):
            self.coordinator.select(1, self.b)
        with self.assertRaises(r.Blocked):
            self.coordinator.activate(1, self.a)
        with self.assertRaises(r.Blocked):
            self.coordinator.reconcile()  # Old deployment on GET cannot unlock.
        self.assertEqual(len(self.posts()), 1)
        self.api.apply(self.api.deferred)
        self.coordinator.reconcile()
        self.api.mode = 'success'
        self.coordinator.select(1, self.b)
        self.coordinator.activate(2, self.b)
        self.assertEqual(len(self.posts()), 2)

    def test_lost_success_response_recovers_without_repost(self):
        self.coordinator.select(0, self.a)
        self.api.mode = 'timeout-after-apply'
        with self.assertRaises(TimeoutError):
            self.coordinator.activate(1, self.a)
        self.coordinator.reconcile()
        self.assertIsNone(self.store.state()['pending'])
        self.assertEqual(len(self.posts()), 1)

    def test_prepared_crash_can_be_abandoned_but_sent_crash_cannot(self):
        self.coordinator.select(0, self.a)
        original = self.store.save
        def crash_after_prepare(state, event):
            original(state, event)
            if event['event'] == 'prepare':
                raise RuntimeError('simulated crash')
        with patch.object(self.store, 'save', crash_after_prepare), self.assertRaises(RuntimeError):
            self.coordinator.activate(1, self.a)
        self.assertEqual(self.posts(), [])
        self.coordinator.reconcile()
        self.assertIsNone(self.store.state()['desired'])
        self.coordinator.select(1, self.a)
        def crash_after_sent(state, event):
            original(state, event)
            if event['event'] == 'send-intent':
                raise RuntimeError('simulated crash immediately before POST')
        with patch.object(self.store, 'save', crash_after_sent), self.assertRaises(RuntimeError):
            self.coordinator.activate(2, self.a)
        with self.assertRaises(r.Blocked):
            self.coordinator.reconcile()
        self.assertEqual(self.posts(), [])

    def test_rollback_has_new_epoch_and_only_admitted_receipts(self):
        for epoch, candidate in [(1, self.a), (2, self.b), (3, self.a)]:
            self.coordinator.select(epoch-1, candidate)
            self.coordinator.activate(epoch, candidate)
        self.assertEqual(self.store.state()['epoch'], 3)
        self.assertEqual(self.store.state()['active_candidate'], self.a)
        with self.assertRaises(r.Blocked):
            self.coordinator.activate(2, self.b)
        with self.assertRaises(r.Blocked):
            self.coordinator.select(3, 'f'*64)
        self.assertEqual(len(self.posts()), 3)

    def test_disabled_activation_and_external_drift_fail_before_post(self):
        self.coordinator.select(0, self.a)
        self.coordinator.enabled = False
        with self.assertRaises(r.Blocked):
            self.coordinator.activate(1, self.a)
        self.coordinator.enabled = True
        self.api.apply({'strategy': 'percentage', 'versions': [], 'annotations': {}})
        with self.assertRaises(r.Blocked):
            self.coordinator.activate(1, self.a)
        self.assertEqual(self.posts(), [])

    def test_exclusive_process_lock_and_wrong_ticket_recovery(self):
        with self.assertRaises(BlockingIOError):
            r.Store(self.directory, CONFIG)
        self.coordinator.select(0, self.a)
        self.api.mode = 'timeout-after-apply'
        with self.assertRaises(TimeoutError):
            self.coordinator.activate(1, self.a)
        self.api.deployments[0]['annotations']['workers/message'] = 'wrong-ticket'
        with self.assertRaises(r.Blocked):
            self.coordinator.reconcile()
        self.assertIsNotNone(self.store.state()['pending'])

    def test_non_asset_bindings_rejected_before_post(self):
        self.coordinator.select(0, self.a)
        self.api.version_bindings = [{'type': 'kv_namespace', 'name': 'PRODUCTION'}]
        with self.assertRaises(ValueError):
            self.coordinator.activate(1, self.a)
        self.assertEqual(self.posts(), [])


class DispatcherTests(unittest.TestCase):
    def test_exact_sha_post_and_no_trigger_mutations(self):
        api = FakeAPI()
        with tempfile.TemporaryDirectory() as tmp:
            result = d.dispatch(api, CONFIG, SHA, Path(tmp)/'dispatch.json', wait=lambda _: None)
        self.assertEqual(result['build_uuid'], BUILD_ID)
        writes = [x for x in api.calls if x[0] != 'GET']
        self.assertEqual(writes, [('POST', '/builds/triggers/'+CONFIG['trigger_uuid']+'/builds',
                                  {'branch': c.BRANCH, 'commit_hash': SHA})])

    def test_wrong_trigger_command_and_actual_build_fail_closed(self):
        for source in ['trigger', 'build']:
            api = FakeAPI()
            if source == 'trigger':
                api.trigger['deploy_command'] = 'npx wrangler deploy'
            else:
                api.build['build_trigger_metadata']['commit_hash'] = 'b'*40
            with tempfile.TemporaryDirectory() as tmp, self.assertRaises(ValueError):
                d.dispatch(api, CONFIG, SHA, Path(tmp)/'state.json')
            self.assertLessEqual(sum(x[0] == 'POST' for x in api.calls), 1)

    def test_resume_only_reads_known_build_without_new_ci_or_post(self):
        api = FakeAPI()
        with tempfile.TemporaryDirectory() as tmp:
            result = d.inspect_build(api, CONFIG, SHA, BUILD_ID, Path(tmp)/'state.json')
        self.assertEqual(result['status'], 'candidate-uploaded')
        self.assertTrue(all(call[0] == 'GET' for call in api.calls))

    def test_ambiguous_dispatch_is_not_retried(self):
        api = FakeAPI()
        api.mode = 'timeout'
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp)/'state.json'
            with self.assertRaises(TimeoutError):
                d.dispatch(api, CONFIG, SHA, path)
            with self.assertRaises(FileExistsError):
                d.dispatch(api, CONFIG, SHA, path)
        self.assertEqual(sum(x[0] == 'POST' for x in api.calls), 1)

    def test_target_disabled_and_existing_tokens_forbidden(self):
        with self.assertRaises(ValueError):
            c.target()
        for key, value in [('worker', 'blog'), ('build_token_uuid', 'f343df38-5ebb-4f05-ab89-1d3ca32cfad7'),
                           ('trigger_uuid', '59dc8a02-0b69-42e9-9c04-f76ac35f8b82')]:
            with tempfile.TemporaryDirectory() as tmp:
                path = Path(tmp)/'target.json'
                path.write_text(json.dumps(dict(CONFIG, **{key: value})))
                with self.subTest(key=key), self.assertRaises(ValueError):
                    c.target(path)


class TransportTests(unittest.TestCase):
    def test_failed_api_call_never_retries_or_prints_credential(self):
        with patch.object(c.urllib.request, 'build_opener') as opener:
            opener.return_value.open.side_effect = OSError('sensitive credential test-only')
            with self.assertRaises(c.ApiError) as error:
                c.API(CONFIG, 'test-only')('POST', '/workers/scripts/animatix-candidate-preview/deployments', {})
            self.assertEqual(opener.return_value.open.call_count, 1)
            self.assertNotIn('test-only', str(error.exception))
        with self.assertRaises(RuntimeError):
            c.NoRedirect().redirect_request(None, None, 302, '', {}, 'https://example.com')


class AdmissionTests(unittest.TestCase):
    def fixture(self):
        policy = c.load('cloudflare-candidate').policy()
        sdk = b'verified CI SDK bytes'
        stream = io.BytesIO()
        with zipfile.ZipFile(stream, 'w') as archive:
            archive.writestr('runtime-manifest.json', sdk)
        archive = stream.getvalue()
        permit = policy.emit(dict(repository_id=policy.REPOSITORY_ID, repository=policy.REPOSITORY, ref=policy.REF,
                                  workflow=policy.WORKFLOW, site_sha=SHA, sdk_source_sha=SHA, producer_sha=SHA,
                                  run_id=123, run_attempt=1, needs_ci_result='success', artifact_id=456,
                                  archive_sha256=hashlib.sha256(archive).hexdigest(), archive_size_bytes=len(archive)), now=NOW-200)
        raw_permit = c.canonical(permit)
        manifest = dict(source_sha=SHA, worker=CONFIG['worker'], release_tag=c.load('cloudflare-candidate').release_tag(permit), permit_sha256=hashlib.sha256(raw_permit).hexdigest(),
                        archive_sha256=hashlib.sha256(archive).hexdigest(),
                        files={'runtime/runtime-manifest.json': hashlib.sha256(sdk).hexdigest()})
        receipt = {k: v for k, v in manifest.items() if k != 'files'}
        receipt.update(version_id=VERSION, preview_url=f'https://{VERSION[:8]}-{CONFIG["worker"]}.test.workers.dev',
                       site_manifest_sha256=hashlib.sha256(c.canonical(manifest)).hexdigest())
        api = FakeAPI()
        api.receipt = receipt
        data = {'permit.json': raw_permit, 'permit.sigstore.json': b'{}', 'sdk.zip': archive,
                '_candidate.json': c.canonical(manifest)}
        return api, data, permit

    def test_build_receipt_signature_archive_and_runtime_are_all_required(self):
        api, data, permit = self.fixture()
        calls = []
        def check(origin, manifest):
            calls.append((origin, manifest))
        fetch = lambda url, limit: data[url.rsplit('/', 1)[1]]
        digest, evidence = a.admit(api, CONFIG, BUILD_ID, '/pinned/gh', fetch=fetch,
                                  check_http=check, verify_signature=lambda *args, **kwargs: permit)
        self.assertEqual(hashlib.sha256(c.canonical(evidence)).hexdigest(), digest)
        self.assertEqual(len(calls), 1)
        data['sdk.zip'] += b'tampered'
        with self.assertRaises(ValueError):
            a.admit(api, CONFIG, BUILD_ID, '/pinned/gh', fetch=fetch,
                    check_http=check, verify_signature=lambda *args, **kwargs: permit)
        self.assertEqual(len(calls), 1)

    def test_bad_signature_wrong_preview_and_truncated_logs_reject(self):
        api, data, permit = self.fixture()
        def reject(*args, **kwargs):
            raise ValueError('invalid signature')
        with self.assertRaises(ValueError):
            a.admit(api, CONFIG, BUILD_ID, '/pinned/gh', fetch=lambda u, n: data[u.rsplit('/', 1)[1]],
                    check_http=lambda *args: self.fail('must not reach HTTP acceptance'), verify_signature=reject)
        bad = dict(api.receipt, preview_url='https://amx.j4ger.dev')
        with self.assertRaises(ValueError):
            a.validate_receipt(bad, CONFIG, SHA)
        with self.assertRaises(ValueError):
            c.receipt_from_logs({'truncated': True, 'lines': []})


if __name__ == '__main__':
    unittest.main()
