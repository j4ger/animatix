#!/usr/bin/env python3
"""Release orchestration pins commits and requires explicit publication intent."""
import os
import json
import sys
from unittest.mock import patch
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class ReleaseWorkflow(unittest.TestCase):
    def test_workflow_pins_artifacts_and_reusable_ci_to_resolved_commit(self):
        source = (ROOT / '.github/workflows/release.yml').read_text()
        self.assertIn('checkout-ref: ${{ needs.version.outputs.commit }}', source)
        self.assertEqual(source.count('ref: ${{ needs.version.outputs.commit }}'), 5)
        self.assertNotIn('ref: ${{ needs.version.outputs.version }}', source)
        self.assertIn("if: needs.version.outputs.publish == 'true'", source)
        self.assertIn('default: false', source)
        self.assertIn('run: tools/ci/ci.sh gate release-publish', source)
        self.assertNotIn('softprops/action-gh-release', source)
        self.assertIn('name: web-runtime', source)
        self.assertIn('web-artifact-verify site-artifact', source)
        self.assertIn('dist-site dist-web-sdk', source)
        self.assertIn('ANIMATIX_RELEASE_TAG_OBJECT:', source)

    def test_meta_version_requires_locked_metadata_and_inheritance(self):
        source = (ROOT / 'tools/ci/ci.sh').read_text().split('gate_meta_version() {', 1)[1]
        code = source.split("<<'PY'\n", 1)[1].split("\nPY", 1)[0]
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'Cargo.toml').write_text('[workspace.package]\nversion = "0.1.0"\n')
            manifest = root / 'member.toml'
            for declaration, resolved, good in (
                ('version.workspace = true', '0.1.0', True),
                ('version = "0.1.0"', '0.1.0', False),
                ('version.workspace = true', '0.2.0', False),
            ):
                manifest.write_text('[package]\nname="member"\n'+declaration+'\n')
                metadata = {'workspace_root': str(root), 'packages': [
                    {'name': 'member', 'version': resolved, 'manifest_path': str(manifest)}]}
                def run(command, **kwargs):
                    self.assertIn('--locked', command)
                    return subprocess.CompletedProcess(command, 0, json.dumps(metadata).encode())
                with patch.object(subprocess, 'run', side_effect=run), patch.object(sys, 'argv', ['gate', str(root)]):
                    if good:
                        exec(code, {})
                    else:
                        with self.assertRaises(SystemExit):
                            exec(code, {})

    def test_no_commit_message_enforcement_or_cog_dependency(self):
        ci = (ROOT / 'tools/ci/ci.sh').read_text()
        self.assertNotIn('gate_commit()', ci)
        self.assertNotIn('commit|nix|', ci)
        self.assertIn('meta-version|', ci)
        self.assertNotIn('cocogitto', (ROOT / 'flake.nix').read_text())
        self.assertIn('git-cliff', (ROOT / 'flake.nix').read_text())
        self.assertFalse((ROOT / 'cog.toml').exists())

    def test_resolution_gate_publish_policy_and_argument_integrity(self):
        ci = (ROOT / 'tools/ci/ci.sh').read_text()
        function = ci[ci.index('gate_release_resolve() {'):ci.index('gate_release_verify() {')]
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            helper = root / 'tools/release/release.py'
            helper.parent.mkdir(parents=True)
            helper.write_text('import json,sys\nopen("arguments.json","w").write(json.dumps(sys.argv[1:]))\n')
            for event, ref, requested, expected, success in (
                ('push', 'refs/tags/v0.1.0', '', 'true', True),
                ('workflow_dispatch', 'refs/heads/main', '', 'false', True),
                ('workflow_dispatch', 'refs/heads/main', 'false', 'false', True),
                ('workflow_dispatch', 'refs/heads/main', 'true', 'true', True),
                ('push', 'refs/heads/main', 'true', None, False),
                ('pull_request', 'refs/tags/v0.1.0', 'true', None, False),
            ):
                output = root / 'output'
                output.unlink(missing_ok=True)
                env = {**os.environ, 'GITHUB_EVENT_NAME': event, 'GITHUB_REF': ref,
                       'TAGGED': 'v0.1.0', 'DISPATCHED': 'v0.1.0', 'GITHUB_SHA': 'a'*40,
                       'PUBLISH_RELEASE': requested, 'GITHUB_OUTPUT': str(output)}
                result = subprocess.run(['bash', '-c', 'set -euo pipefail\n'+function+'\ngate_release_resolve'],
                                        cwd=root, env=env, capture_output=True, text=True)
                self.assertEqual(result.returncode == 0, success, result.stderr)
                if success:
                    self.assertIn('publish='+expected, output.read_text())
                else:
                    self.assertFalse(output.exists())


if __name__ == '__main__':
    unittest.main()
