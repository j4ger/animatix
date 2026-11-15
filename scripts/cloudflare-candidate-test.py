#!/usr/bin/env python3
"""Policy tests, not a substitute for real Sigstore/Cloudflare acceptance."""
import copy
import json
from pathlib import Path
import importlib.util
import unittest


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name+'.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class CandidateTests(unittest.TestCase):
    def test_entrypoints_parse_and_load(self):
        # Include the actual consumer and uploader: policy-only tests cannot catch
        # syntax or wiring failures in an otherwise unexecuted remote entrypoint.
        load('cloudflare-verified-consumer')
        load('cloudflare-candidate-upload')
        load('cloudflare-preview-check')

    def test_policy_separation_and_destination(self):
        fixture = load('cloudflare-permit-test')
        old = fixture.p.emit(fixture.metadata(), fixture.NOW)
        p = load('cloudflare-candidate').policy()
        with self.assertRaises(p.PermitError):
            p.validate_permit(old, fixture.SHA, fixture.NOW)
        meta = dict(fixture.metadata(), ref=p.REF, workflow=p.WORKFLOW)
        permit = p.emit(meta, fixture.NOW)
        self.assertEqual(p.validate_permit(permit, fixture.SHA, fixture.NOW), permit)
        with self.assertRaises(fixture.p.PermitError):
            fixture.p.validate_permit(permit, fixture.SHA, fixture.NOW)
        for key, value in [('mode', 'production'), ('ref', 'refs/heads/main'),
                           ('cloudflare', {'account_id': p.ACCOUNT, 'worker': 'blog'}),
                           ('site_sha', 'b'*40), ('expires_at', fixture.NOW)]:
            with self.subTest(key=key), self.assertRaises(p.PermitError):
                p.validate_permit(dict(permit, **{key: value}), fixture.SHA, fixture.NOW)

    def test_signed_identity_and_digest_are_still_required(self):
        fixture = load('cloudflare-permit-test')
        p = load('cloudflare-candidate').policy()
        fixture.p = p
        permit = p.emit(fixture.metadata(), fixture.NOW)
        raw = json.dumps(permit).encode()
        verified = fixture.verified(raw)
        self.assertTrue(p.validate_verified_output(verified, permit, raw))
        for field in ['buildSignerURI', 'buildSignerDigest', 'sourceRepositoryIdentifier', 'runInvocationURI']:
            bad = copy.deepcopy(verified)
            bad[0]['verificationResult']['signature']['certificate'][field] = 'wrong'
            with self.subTest(field=field), self.assertRaises(p.PermitError):
                p.validate_verified_output(bad, permit, raw)
        with self.assertRaises(p.PermitError):
            p.validate_verified_output(verified, permit, raw+b' ')

    def test_upload_rejects_production_config_and_routes(self):
        upload = load('cloudflare-candidate-upload')
        p = load('cloudflare-candidate').policy()
        config = json.loads(Path('wrangler.jsonc').read_text())
        upload.config_check(config, p)
        for key, value in [('name', 'blog'), ('routes', ['amx.j4ger.dev/*']), ('workers_dev', True)]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                upload.config_check(dict(config, **{key: value}), p)


if __name__ == '__main__':
    unittest.main()
