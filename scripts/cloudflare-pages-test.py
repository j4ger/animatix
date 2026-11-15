#!/usr/bin/env python3
"""Pages policy tests; remote Git/build/deployment acceptance remains required."""
import copy
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
                           ('ref', 'refs/heads/main'), ('sdk_source_sha', 'b'*40),
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


if __name__ == '__main__':
    unittest.main()
