"""Unit tests for compatibility smoke validation without invoking Cargo."""

from pathlib import Path
import binascii
import hashlib
import json
import os
import struct
import subprocess
import tempfile
import unittest
from unittest import mock
import zlib

from scripts.experiments import compat, gui_probe, linkers
from scripts.experiments.common import ExperimentFailure, ExperimentOutput


def _png_fixture(width: int, height: int, pixel) -> bytes:
    def chunk(kind: bytes, data: bytes) -> bytes:
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", binascii.crc32(body) & 0xFFFFFFFF)

    rows = bytearray()
    for y in range(height):
        rows.append(0)
        for x in range(width):
            rows.extend(pixel(x, y))
    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header)
            + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))


class CompatibilityTests(unittest.TestCase):
    def test_png_smoke_accepts_visible_nonuniform_render(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "smoke.png"
            path.write_bytes(_png_fixture(
                320, 180,
                lambda x, y: ((40, 80, 120, 255) if 25 <= x < 35 and 25 <= y < 35
                              else (255, 255, 255, 255) if 20 <= x < 40 and 20 <= y < 40
                              else (0, 0, 0, 0)),
            ))
            record = compat._assert_png(path)
            self.assertEqual((record["width"], record["height"]), (320, 180))
            self.assertGreater(record["visible_pixels"], 10)
            self.assertGreater(record["visible_color_count"], 1)

            path.write_bytes(b"not an image" * 40)
            with self.assertRaisesRegex(ExperimentFailure, "not a PNG"):
                compat._assert_png(path)

    def test_png_smoke_rejects_blank_or_uniform_stage(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "blank.png"
            path.write_bytes(_png_fixture(320, 180, lambda _x, _y: (0, 0, 0, 0)))
            with self.assertRaisesRegex(ExperimentFailure, "visually blank or uniform"):
                compat._assert_png(path)

    def test_png_smoke_rejects_wrong_dimensions(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "smoke.png"
            path.write_bytes(_png_fixture(1280, 720, lambda x, y: (x % 255, y % 255, 40, 255)))
            with self.assertRaisesRegex(ExperimentFailure, "dimensions"):
                compat._assert_png(path)

    def test_linux_wrapper_proof_tracks_link_output_after_binary_staging(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            callee = root / "ld.bfd"
            callee.write_text("GNU linker fixture")
            callee_hash = linkers._file_sha256(callee)
            linked_output = root / "owned-target/debug/animatix"
            staged = root / "compatibility/animatix"
            staged.parent.mkdir(parents=True)
            staged.write_text("same executable bytes")
            audit = root / "logs/linker-audit.jsonl"
            audit.parent.mkdir()
            audit.write_text(json.dumps({
                "callee": str(callee), "callee_sha256": callee_hash,
                "argv0": "ld.bfd", "argv": ["-o", str(linked_output)],
                "expanded_argv": ["-o", str(linked_output)], "response_files": [],
                "cwd": str(root),
                "output_path": str(linked_output),
            }) + "\n")
            linker_log = root / "logs/build.linker.log"
            linker_log.write_text(
                ' INFO rustc_codegen_ssa::back::link LC_ALL="C" "cc" "-o" '
                f'"{linked_output}"\n',
            )
            output = ExperimentOutput(root, Path("target/experiments/linker"), "linker", {})
            arm = {"id": "system-bfd", "artifact_identity": "gnu", "audit": {
                "callee_path": str(callee), "callee_sha256": callee_hash, "argv0": "ld.bfd",
            }}
            info = {"audit_log_path": str(audit), "link_output_path": str(linked_output),
                    "linker_log_path": str(linker_log)}
            no_comment = subprocess.CompletedProcess(
                ["readelf"], 0, "There is no .comment section in this file.\n", "",
            )
            with mock.patch.object(linkers.shutil, "which", return_value="readelf"), \
                 mock.patch.object(linkers.subprocess, "run", return_value=no_comment):
                evidence = compat._linker_binary_evidence(
                    "linux", arm, [(staged, info)], output,
                )
            self.assertEqual(evidence["artifacts"][0]["detected"], "unknown")
            self.assertEqual(evidence["rustc_linker_invocations"][0]["output_path"],
                             str(linked_output))
            invocation = evidence["actual_linker_invocations"][0]
            self.assertEqual(invocation["output_path"], str(linked_output))

    def test_pinned_macos_lld_checks_exact_version_and_workflow_hash(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "ld64.lld"
            binary.write_text("#!/bin/sh\necho 'LLD 21.1.8 (compatible with GNU linkers)'\n")
            binary.chmod(0o755)
            expected_hash = linkers._file_sha256(binary)
            with mock.patch.object(linkers.shutil, "which", side_effect=lambda name: str(binary) if name == "ld64.lld" else None), \
                 mock.patch.dict(os.environ, {"ANIMATIX_EXPECTED_LLD_SHA256": expected_hash}):
                self.assertEqual(linkers._find_macos_lld().name, "ld64.lld")
            with mock.patch.object(linkers.shutil, "which", side_effect=lambda name: str(binary) if name == "ld64.lld" else None), \
                 mock.patch.dict(os.environ, {"ANIMATIX_EXPECTED_LLD_SHA256": "0" * 64}):
                with self.assertRaisesRegex(ExperimentFailure, "SHA-256 differs"):
                    linkers._find_macos_lld()

    def test_linux_child_environment_is_private_x11_profile(self):
        with tempfile.TemporaryDirectory() as temporary:
            profile = Path(temporary) / "gui-profile"
            parent = {
                "DISPLAY": ":99", "WAYLAND_DISPLAY": "wayland-0", "WAYLAND_SOCKET": "/tmp/socket",
                "XDG_SESSION_TYPE": "wayland", "XDG_CONFIG_HOME": "/user/config",
                "XDG_DATA_HOME": "/user/data", "XDG_CACHE_HOME": "/user/cache", "PATH": "/usr/bin",
            }
            original = dict(parent)
            child = gui_probe._linux_child_env(profile, parent)
            self.assertEqual(parent, original)
            self.assertEqual(child["DISPLAY"], ":99")
            self.assertEqual(child["XDG_SESSION_TYPE"], "x11")
            self.assertNotIn("WAYLAND_DISPLAY", child)
            self.assertNotIn("WAYLAND_SOCKET", child)
            self.assertEqual(child["PATH"], "/usr/bin")
            for variable, directory in (("XDG_CONFIG_HOME", "config"),
                                        ("XDG_DATA_HOME", "data"),
                                        ("XDG_CACHE_HOME", "cache")):
                self.assertEqual(Path(child[variable]), profile.resolve() / directory)
                self.assertTrue(Path(child[variable]).is_dir())
            with self.assertRaisesRegex(RuntimeError, "xvfb-run"):
                gui_probe._linux_child_env(profile, {"WAYLAND_DISPLAY": "wayland-0"})

    def test_smoke_scene_copy_preserves_bytes_and_library_tree(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / "repo"
            scene = repo / "examples/basics/00_hello.amx"
            library = repo / "web/demos/lib"
            scene.parent.mkdir(parents=True)
            library.mkdir(parents=True)
            scene_bytes = b'import "../../web/demos/lib/theme.amx"\nimport "../../web/demos/lib/stage.amx"\nconfig {}\n'
            scene.write_bytes(scene_bytes)
            (library / "theme.amx").write_bytes(b"theme source\n")
            (library / "stage.amx").write_bytes(b"stage source\n")
            (library / "shared.amx").write_bytes(b"additional sibling\n")
            smoke_root = root / "owned-output"

            staged_scene, proof = compat._stage_smoke_scene(
                repo, Path("examples/basics/00_hello.amx"), smoke_root,
            )

            self.assertEqual(staged_scene.read_bytes(), scene_bytes)
            self.assertTrue(proof["relative_layout_mirrored"])
            source_files = [scene, *sorted(path for path in library.rglob("*") if path.is_file())]
            self.assertEqual(len(proof["files"]), len(source_files))
            for record in proof["files"]:
                source_path = Path(record["source"])
                destination = Path(record["destination"])
                self.assertEqual(source_path.read_bytes(), destination.read_bytes())
                self.assertEqual(record["source_sha256"], record["destination_sha256"])
                self.assertEqual(record["source_sha256"], hashlib.sha256(source_path.read_bytes()).hexdigest())

    def test_macos_window_probe_requires_and_returns_real_window_names(self):
        result = mock.Mock(returncode=0, stdout="Animatix, Settings\n", stderr="")
        with mock.patch.object(gui_probe.subprocess, "run", return_value=result):
            self.assertEqual(gui_probe._mac_window_names("animatix-gui"), ["Animatix", "Settings"])
        empty = mock.Mock(returncode=0, stdout="missing value\n", stderr="")
        with mock.patch.object(gui_probe.subprocess, "run", return_value=empty):
            self.assertEqual(gui_probe._mac_window_names("animatix-gui"), [])


if __name__ == "__main__":
    unittest.main()
