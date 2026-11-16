#!/usr/bin/env python3
"""Exercise the production uploader boundary without credentials or network."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SOURCE = Path(__file__).with_name('cloudflare-deploy.sh')

class DeployTests(unittest.TestCase):
    def run_fixture(self, *, remote='a'*40, dirty='', expected='a'*40, token='fixture-only', project='animatix'):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root/'web/tools/deploy').mkdir(parents=True)
            shutil.copy(SOURCE, root/'web/tools/deploy/cloudflare-deploy.sh')
            binpath = root/'bin'; binpath.mkdir()
            git = binpath/'git'
            git.write_text('#!/bin/sh\ncase "$1" in\nrev-parse) echo '+ 'a'*40 +' ;;\nstatus) printf "%s" "$FIXTURE_DIRTY" ;;\nls-remote) printf "%s\\trefs/heads/main\\n" "$FIXTURE_REMOTE" ;;\nesac\n')
            for name in ('python3', 'node'):
                (binpath/name).write_text('#!/bin/sh\nexit 0\n')
            for p in binpath.iterdir(): p.chmod(0o755)
            cli=root/'web/deploy/node_modules/.bin/wrangler'; cli.parent.mkdir(parents=True)
            cli.write_text('#!/bin/sh\nprintf "%s\\n" "$@" > deployed\n'); cli.chmod(0o755)
            env=dict(os.environ, PATH=str(binpath)+':'+os.environ['PATH'],
                     CLOUDFLARE_ACCOUNT_ID='b'*32, CLOUDFLARE_PAGES_PROJECT=project,
                     CLOUDFLARE_API_TOKEN=token, ANIMATIX_ARTIFACT_COMMIT=expected,
                     FIXTURE_REMOTE=remote, FIXTURE_DIRTY=dirty)
            run=subprocess.run(['bash',str(root/'web/tools/deploy/cloudflare-deploy.sh')],env=env,capture_output=True,text=True)
            return run, (root/'deployed').read_text() if (root/'deployed').exists() else None
    def test_exact_clean_main_uploads_explicit_identity(self):
        run,args=self.run_fixture(); self.assertEqual(run.returncode,0,run.stderr)
        self.assertIn('--commit-hash\n'+'a'*40,args); self.assertIn('--branch\nmain',args)
    def test_stale_run_never_uploads(self):
        run,args=self.run_fixture(remote='c'*40); self.assertNotEqual(run.returncode,0); self.assertIsNone(args)
    def test_dirty_never_uploads(self):
        run,args=self.run_fixture(dirty=' M source'); self.assertNotEqual(run.returncode,0); self.assertIsNone(args)
    def test_mismatched_ci_sha_never_uploads(self):
        run,args=self.run_fixture(expected='d'*40); self.assertNotEqual(run.returncode,0); self.assertIsNone(args)
    def test_missing_secret_never_uploads(self):
        run,args=self.run_fixture(token=''); self.assertNotEqual(run.returncode,0); self.assertIsNone(args)
    def test_project_option_injection_rejected(self):
        run,args=self.run_fixture(project='--bad'); self.assertNotEqual(run.returncode,0); self.assertIsNone(args)

if __name__ == '__main__': unittest.main()
