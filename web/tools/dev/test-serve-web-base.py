#!/usr/bin/env python3
"""Path-prefix regression tests without opening sockets or requiring a built SDK."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('serve_web', Path(__file__).with_name('serve-web.py'))
server = importlib.util.module_from_spec(spec)
spec.loader.exec_module(server)


class BasePathTests(unittest.TestCase):
    def setUp(self):
        self.base = server.SERVE_BASE
        self.root = server.WEB_DIR
        self.tmp = tempfile.TemporaryDirectory()
        server.WEB_DIR = self.tmp.name
        self.handler = object.__new__(server.Handler)
        self.handler.directory = self.tmp.name

    def tearDown(self):
        server.SERVE_BASE = self.base
        server.WEB_DIR = self.root
        self.tmp.cleanup()

    def test_project_pages_resources_resolve_inside_artifact(self):
        server.SERVE_BASE = server.normalize_base('/animatix/')
        for path in ['index.html', 'runtime/amx-player.js', 'content/scenes/first-move.amx']:
            self.assertEqual(self.handler.translate_path('/animatix/' + path + '?x=1'), str(Path(self.tmp.name, path)))
        self.assertEqual(self.handler.translate_path('/animatix/'), self.tmp.name + '/')
        self.assertEqual(server.strip_base('/animatix/learn/'), '/learn/')

    def test_root_base_is_unchanged(self):
        server.SERVE_BASE = server.normalize_base('/')
        self.assertEqual(self.handler.translate_path('/runtime/amx-player.js'), str(Path(self.tmp.name, 'runtime/amx-player.js')))

    def test_prefix_boundary_is_not_optional(self):
        server.SERVE_BASE = '/animatix'
        for path in ['/runtime/amx-player.js', '/animatix-other/runtime/a.js']:
            self.assertIsNone(server.strip_base(path))
            self.assertEqual(self.handler.translate_path(path), str(Path(self.tmp.name, '__rejected_path__')))

    def test_probe_imports_resolve_inside_the_served_tree(self):
        server.SERVE_BASE = '/animatix'
        path = self.handler.translate_path('/animatix/dev/lib/stage.amx')
        self.assertEqual(path, str(Path(self.tmp.name, 'dev/lib/stage.amx')))

    def test_nix_store_script_finds_checker_from_served_checkout(self):
        repo = Path(self.tmp.name)
        (repo / 'Cargo.toml').write_text('[workspace]')
        (repo / 'web/tools').mkdir(parents=True)
        (repo / 'web/tools/sdk.mjs').write_text('// verifier')
        (repo / 'web/runtime').mkdir()
        (repo / 'web/runtime/runtime-manifest.json').write_text('{}')
        with patch.object(server, '__file__', '/nix/store/hash-serve-web.py'), \
             patch.object(server.subprocess, 'run') as run:
            result = server.check_wasm_staleness(str(repo / 'web'))
        self.assertEqual(result['results'][0][1], 'fresh')
        self.assertEqual(run.call_args.kwargs['cwd'], str(repo))
        self.assertIn('--current', run.call_args.args[0])
        self.assertEqual(run.call_args.args[0][1], str(repo / 'web/tools/sdk.mjs'))

    def test_invalid_configuration_fails_closed(self):
        for base in ['animatix', '/../private', '/a//b', '/%2e', '/a?b']:
            with self.assertRaises(ValueError):
                server.normalize_base(base)


if __name__ == '__main__':
    unittest.main()
