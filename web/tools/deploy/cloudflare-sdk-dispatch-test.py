#!/usr/bin/env python3
import importlib.util
import io
import json
import os
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('dispatch', Path(__file__).with_name('cloudflare-sdk-dispatch.py'))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
SHA = 'a'*40
REF = 'refs/heads/experiment/cloudflare-native-build'
TRIGGER = '59dc8a02-0b69-42e9-9c04-f76ac35f8b82'


class DispatchTests(unittest.TestCase):
    def exercise(self, stale=False, fail=False, mismatch=False):
        calls = []
        reads = 0
        def shell(args, **kwargs):
            nonlocal reads
            if args[1] == 'rev-parse':
                return SHA+'\n'
            reads += 1
            return (('b'*40 if stale and reads == 3 else SHA)+'\t'+REF+'\n')
        def api(req, **kwargs):
            path = req.full_url.split('/accounts/')[1].split('/',1)[1]
            body = json.loads(req.data) if req.data else None
            calls.append((req.method,path,body))
            if path.endswith('/triggers'):
                result = [dict(trigger_uuid=TRIGGER,branch_includes=['experiment/cloudflare-native-build'],build_command=m.SAFE_PROBE)]
            elif req.method == 'GET' and path.endswith('/environment_variables'):
                result = {}
            elif req.method == 'POST':
                result = {'build_uuid':'fixed-build'}
            elif path.endswith('/logs'):
                result = {'lines':[[1,'bad' if fail else 'SIGNED_SDK_ASTRO_BUILD_ONLY_PASS']], 'truncated':False}
            elif req.method == 'GET':
                result = {'status':'stopped','build_outcome':'fail','trigger':{'trigger_uuid':TRIGGER},'build_trigger_metadata':{'commit_hash':('b'*40 if mismatch else SHA),'branch':'experiment/cloudflare-native-build','build_command':m.BUILD_COMMAND}}
            else:
                result = {}
            return io.BytesIO(json.dumps({'success':True,'result':result}).encode())
        with patch.dict(os.environ, {'CLOUDFLARE_EXPERIMENT_TOKEN':'not-a-real-token', 'GITHUB_SHA':SHA, 'GITHUB_REF':REF},clear=True), \
                patch.object(m.subprocess,'check_output',side_effect=shell), patch.object(m.time,'sleep'), \
                patch.object(m.urllib.request,'build_opener') as opener:
            opener.return_value.open.side_effect = api
            if stale or fail or mismatch:
                with self.assertRaises(ValueError if stale else RuntimeError):
                    m.main()
            else:
                m.main()
        return calls

    def test_exact_sha_and_restore(self):
        calls = self.exercise()
        post = next(c for c in calls if c[0]=='POST')
        self.assertEqual(post[2],{'branch':'experiment/cloudflare-native-build','commit_hash':SHA})
        self.assertEqual(calls[-2][2],{'build_command':m.SAFE_PROBE})
        self.assertEqual(calls[-1][0],'DELETE')
        self.assertTrue(all('/deployments' not in c[1] for c in calls))

    def test_old_result_rejected_and_restored(self):
        calls = self.exercise(stale=True)
        self.assertEqual(calls[-1][0],'DELETE')

    def test_missing_marker_rejected(self):
        self.assertEqual(self.exercise(fail=True)[-1][0],'DELETE')

    def test_actual_wrong_commit_rejected(self):
        self.assertEqual(self.exercise(mismatch=True)[-1][0], 'DELETE')

    def test_missing_token_no_request(self):
        with patch.dict(os.environ,{},clear=True), patch.object(m.urllib.request,'build_opener') as opener:
            with self.assertRaises(RuntimeError):
                m.main()
            opener.assert_not_called()

    def test_no_auth_redirect(self):
        with self.assertRaises(RuntimeError):
            m.NoRedirect().redirect_request()


if __name__ == '__main__':
    unittest.main()
