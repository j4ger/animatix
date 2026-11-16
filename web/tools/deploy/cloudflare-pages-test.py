#!/usr/bin/env python3
"""Pages policy tests; remote Git/build/deployment acceptance remains required."""
import copy
import hashlib
import io
import urllib.error
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + '.py'))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


class PagesTests(unittest.TestCase):
    def test_policy_is_separate(self):
        pages = load('cloudflare-pages')
        p = pages.policy()
        fixture = load('cloudflare-permit-test')
        meta = dict(fixture.metadata(), ref=p.REF, workflow=p.WORKFLOW)
        permit = p.emit(meta, fixture.NOW)
        self.assertEqual(p.validate_permit(permit, fixture.SHA, fixture.NOW), permit)
        for key, value in [('schema_version', 2), ('mode', 'candidate-upload'),
                           ('cloudflare', {'account_id': p.ACCOUNT, 'worker': 'animatix-candidate-preview'}),
                           ('ref', 'refs/heads/feat/cloudflare-pages'), ('sdk_source_sha', 'b'*40),
                           ('expires_at', fixture.NOW)]:
            with self.subTest(key=key), self.assertRaises(p.PermitError):
                p.validate_permit(dict(permit, **{key: value}), fixture.SHA, fixture.NOW)

    def test_signature_identity_remains_strict(self):
        fixture = load('cloudflare-permit-test')
        fixture.p = load('cloudflare-pages').policy()
        permit = fixture.p.emit(fixture.metadata(), fixture.NOW)
        raw = json.dumps(permit).encode()
        verified = fixture.verified(raw)
        self.assertTrue(fixture.p.validate_verified_output(verified, permit, raw))
        for field in ['buildSignerURI', 'sourceRepositoryRef', 'runInvocationURI']:
            bad = copy.deepcopy(verified)
            bad[0]['verificationResult']['signature']['certificate'][field] = 'wrong'
            with self.assertRaises(fixture.p.PermitError):
                fixture.p.validate_verified_output(bad, permit, raw)

    def test_pages_checkout_metadata_and_branch(self):
        pages = load('cloudflare-pages')
        env = dict(CF_PAGES='1', CF_PAGES_BRANCH='deploy/pages', CF_PAGES_COMMIT_SHA='a'*40)
        old = Path.cwd()
        with tempfile.TemporaryDirectory() as tmp:
            try:
                os.chdir(tmp)
                pages.check_context(env, 'a'*40)
                for key, value in [('CF_PAGES', '0'), ('CF_PAGES_BRANCH', 'main'),
                                   ('CF_PAGES_COMMIT_SHA', 'b'*40)]:
                    with self.subTest(key=key), self.assertRaises(ValueError):
                        pages.check_context(dict(env, **{key: value}), 'a'*40)
                Path('functions').mkdir()
                with self.assertRaises(ValueError):
                    pages.check_context(env, 'a'*40)
            finally:
                os.chdir(old)

    def test_ref_discovery_is_bounded_and_sha_bound(self):
        pages = load('cloudflare-pages')
        sha = 'a'*40
        tag = 'cf-pages-'+sha+'-r1-a2'
        self.assertEqual(pages.validate_refs(sha+'\trefs/tags/'+tag+'\n', sha), [tag])
        for result in ['', 'b'*40+'\trefs/tags/'+tag, sha+'\trefs/tags/cf-preview-'+sha+'-r1-a2',
                       (sha+'\trefs/tags/'+tag+'\n')*2, sha+'\trefs/tags/'+tag+'^{}']:
            with self.subTest(result=result), self.assertRaises(ValueError):
                pages.validate_refs(result, sha)

    def test_namespace_is_not_worker_candidate(self):
        pages = load('cloudflare-pages')
        self.assertEqual(pages.release_tag({'site_sha': 'a'*40, 'ci': {'run_id': 1, 'run_attempt': 2}}),
                         'cf-pages-'+'a'*40+'-r1-a2')
        # The shared consumer takes a distinct issuing policy and release-branch
        # check; it does not reinterpret a schema-2 permit as Pages authorization.
        consumer = load('cloudflare-verified-consumer')
        self.assertIn('issuance', consumer.main.__code__.co_varnames)


class IssuanceFallbackTests(unittest.TestCase):
    def setUp(self):
        self.consumer = load('cloudflare-verified-consumer')
        self.pages = load('cloudflare-pages')
        self.p = self.pages.policy()
        self.fixture = load('cloudflare-permit-test')
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.sdk = b'signed ZIP fixture'
        self.permits = {}
        self.assets = {}
        meta = dict(self.fixture.metadata(), ref=self.p.REF, workflow=self.p.WORKFLOW,
                    archive_sha256=hashlib.sha256(self.sdk).hexdigest(), archive_size_bytes=len(self.sdk))
        for attempt in (1, 2):
            permit = self.p.emit(dict(meta, run_attempt=attempt), self.fixture.NOW)
            tag = self.pages.release_tag(permit)
            self.permits[tag] = permit
            for name, raw in [('permit.json', json.dumps(permit).encode()),
                              ('permit.sigstore.json', b'verification fixture'), ('sdk.zip', self.sdk)]:
                path = self.root/tag/name
                path.parent.mkdir(exist_ok=True)
                path.write_bytes(raw)
                self.assets[tag, name] = path
        self.tags = list(reversed(self.permits))
        self.calls = []

    def fetch(self, tag, name, limit):
        self.calls.append((tag, name))
        path = self.assets.get((tag, name))
        if path is None:
            raise urllib.error.HTTPError('https://github.com/example', 404, 'missing', {}, None)
        return path

    def verify(self, permit_path, bundle, *args):
        # This fixture replaces only cryptography, not the policy's actual
        # identity, source-SHA, destination, issuance or expiry validation.
        return self.p.validate_permit(json.loads(permit_path.read_text()), self.fixture.SHA)

    def select(self):
        with patch.object(self.p, 'verify', side_effect=self.verify), \
                patch.object(self.p.time, 'time', return_value=self.fixture.NOW), \
                patch('sys.stdout', new_callable=io.StringIO):
            return self.consumer.select_issuance(self.tags, self.p, self.pages, self.fetch,
                                                '/pinned/gh', self.fixture.SHA, lambda: self.fixture.SHA)

    def test_missing_newest_assets_fall_back_to_complete_older_issuance(self):
        for name in ['permit.json', 'permit.sigstore.json', 'sdk.zip']:
            with self.subTest(name=name):
                saved = self.assets.pop((self.tags[0], name))
                permit, permit_path, bundle, archive = self.select()
                self.assertEqual(permit, self.permits[self.tags[1]])
                self.assertEqual({p.parent.name for p in (permit_path, bundle, archive)}, {self.tags[1]})
                self.assets[self.tags[0], name] = saved

    def test_corrupt_newest_sdk_falls_back_without_weakening_hash(self):
        self.assets[self.tags[0], 'sdk.zip'].write_bytes(b'corrupt')
        permit, *_ = self.select()
        self.assertEqual(permit, self.permits[self.tags[1]])
        self.assets[self.tags[1], 'sdk.zip'].write_bytes(b'corrupt')
        with self.assertRaises(RuntimeError):
            self.select()

    def test_expired_or_wrong_identity_older_permit_never_passes(self):
        self.assets.pop((self.tags[0], 'permit.json'))
        older = self.assets[self.tags[1], 'permit.json']
        valid = json.loads(older.read_text())
        for change in [dict(expires_at=self.fixture.NOW), dict(ref='refs/heads/feat/cloudflare-pages'),
                       dict(sdk_source_sha='b'*40)]:
            with self.subTest(change=change):
                older.write_text(json.dumps(dict(valid, **change)))
                with self.assertRaises(RuntimeError):
                    self.select()

    def test_invalid_signature_and_wrong_issuance_never_select_newest(self):
        original = self.verify
        for reason in ['signature', 'tag']:
            def verify(path, bundle, *args):
                value = original(path, bundle, *args)
                if path.parent.name == self.tags[0]:
                    if reason == 'signature':
                        raise self.p.PermitError('bad signature fixture')
                    value = dict(value, ci=dict(value['ci'], run_attempt=99))
                return value
            with patch.object(self.p, 'verify', side_effect=verify), \
                    patch.object(self.p.time, 'time', return_value=self.fixture.NOW), \
                    patch('sys.stdout', new_callable=io.StringIO):
                result = self.consumer.select_issuance(self.tags, self.p, self.pages, self.fetch,
                                                       '/pinned/gh', self.fixture.SHA, lambda: self.fixture.SHA)
            self.assertEqual(result[0], self.permits[self.tags[1]])

    def test_no_completion_claim_on_failure(self):
        message = self.consumer.outcome_message(True, False)
        self.assertNotIn('output complete', message)
        self.assertIn('incomplete', message)
        self.assertIn('output complete', self.consumer.outcome_message(True, True))


if __name__ == '__main__':
    unittest.main()
