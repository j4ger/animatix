#!/usr/bin/env python3
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location('consumer', Path(__file__).with_name('cloudflare-sdk-consumer.py'))
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)


class ConsumerTests(unittest.TestCase):
    def setUp(self):
        self.commit = 'a'*40
        self.run = dict(id=10, workflow_id=281504146, head_sha=self.commit, head_branch='main', event='push', status='completed', conclusion='success', path='.github/workflows/ci.yml', repository=dict(full_name=p.REPOSITORY))
        self.artifact = dict(name='web-runtime', expired=False, digest='sha256:'+'b'*64, workflow_run=dict(id=10, head_sha=self.commit, repository_id=1185977116, head_repository_id=1185977116))

    def test_valid(self):
        self.assertEqual(p.validate_run(self.run, self.artifact, self.commit, 10), 'b'*64)

    def test_reject_run(self):
        for key, value in [('conclusion','failure'),('status','in_progress'),('event','pull_request'),('head_sha','c'*40),('head_branch','other'),('workflow_id',1),('id',12),('path','wrong')]:
            run = dict(self.run, **{key:value})
            with self.assertRaises(RuntimeError):
                p.validate_run(run,self.artifact,self.commit,10)

    def test_reject_artifact(self):
        for key, value in [('expired',True),('name','other'),('digest','sha256:bad')]:
            with self.assertRaises(RuntimeError):
                p.validate_run(self.run,dict(self.artifact,**{key:value}),self.commit,10)
        artifact = copy.deepcopy(self.artifact)
        artifact['workflow_run']['head_repository_id'] = 999
        with self.assertRaises(RuntimeError):
            p.validate_run(self.run,artifact,self.commit,10)

    def test_unsafe_zip(self):
        for name in ['../escape','/escape','dir\\escape']:
            with tempfile.TemporaryDirectory() as root:
                path = Path(root)/'test.zip'
                with zipfile.ZipFile(path,'w') as z:
                    z.writestr(name,'bad')
                with self.assertRaises(RuntimeError):
                    p.unpack(path,Path(root)/'out')
        with tempfile.TemporaryDirectory() as root:
            path = Path(root)/'test.zip'
            with zipfile.ZipFile(path,'w') as z:
                info = zipfile.ZipInfo('link')
                info.external_attr = 0o120777 << 16
                z.writestr(info,'/etc/passwd')
            with self.assertRaises(RuntimeError):
                p.unpack(path,Path(root)/'out')

    def test_https_redirect(self):
        with self.assertRaises(RuntimeError):
            p.HTTPSOnly().redirect_request(None,None,302,'',{},'http://example.com')


if __name__ == '__main__':
    unittest.main()
