#!/usr/bin/env python3
"""Contract tests for smoke wiring; no browser or fake Nix environment required."""
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
CI = (ROOT / 'scripts/ci.sh').read_text()


class SmokeGates(unittest.TestCase):
    def run_retry(self, first, second):
        function = CI.split('run_browser_smoke() {', 1)[1].split('\ngate_web_smoke()', 1)[0]
        with tempfile.TemporaryDirectory() as directory:
            counter = Path(directory) / 'attempts'
            script = '''run_browser_smoke() {''' + function + '''
node() {
  echo "$SMOKE_SUITE:$SMOKE_ATTEMPT:$SMOKE_OUTPUT_DIR" >> "$COUNTER"
  if [ "$SMOKE_ATTEMPT" = 1 ]; then return "$FIRST"; else return "$SECOND"; fi
}
sleep() { :; }
run_browser_smoke sdk
'''
            import os
            result = subprocess.run(['bash', '-c', script], env={**os.environ,
                'COUNTER': str(counter), 'FIRST': str(first), 'SECOND': str(second)}, capture_output=True)
            return result, counter.read_text().splitlines()

    def test_success_is_single_attempt(self):
        result, attempts = self.run_retry(0, 1)
        self.assertEqual(result.returncode, 0)
        self.assertEqual(attempts, ['sdk:1:target/web-smoke/sdk/attempt-1'])

    def test_scene_error_is_not_retried(self):
        result, attempts = self.run_retry(1, 0)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(len(attempts), 1)
        self.assertNotIn(b'retry', result.stderr)

    def test_startup_error_gets_one_retry_and_separate_evidence(self):
        result, attempts = self.run_retry(75, 0)
        self.assertEqual(result.returncode, 0)
        self.assertEqual(attempts[-1], 'sdk:2:target/web-smoke/sdk/attempt-2')

    def test_second_failure_is_final(self):
        result, attempts = self.run_retry(75, 75)
        self.assertEqual(result.returncode, 75)
        self.assertEqual(len(attempts), 2)
        self.assertEqual(result.stderr.count(b'retry'), 1)

    def test_site_gate_never_falls_back_to_source_site(self):
        body = CI.split('gate_site_smoke() {', 1)[1].split('\ngate_web_site_smoke()', 1)[0]
        self.assertIn('[ -f _site/index.html ]', body)
        self.assertIn('ANIMATIX_WEB_ROOT="$root/_site" run_browser_smoke site', body)

    def test_release_smoke_precedes_packaging(self):
        workflow = (ROOT / '.github/workflows/release.yml').read_text()
        self.assertLess(workflow.index('gate site-smoke'), workflow.index('gate dist-site dist-web-sdk'))


if __name__ == '__main__':
    unittest.main()
