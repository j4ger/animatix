#!/usr/bin/env python3
"""Asset and Linux desktop integration checks without a display or Rust build."""
import importlib.util
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "crates/animatix-gui/assets"
APP_ID = "io.github.j4ger.animatix"
spec = importlib.util.spec_from_file_location("desktop_installer", ROOT / "packaging/linux/install-desktop.py")
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class Icons(unittest.TestCase):
    def test_svg_and_favicon_consumers(self):
        self.assertEqual((ROOT / "logo.svg").read_bytes(), (ROOT / "web/logo.svg").read_bytes())
        svg = ET.parse(ROOT / "logo.svg")
        self.assertEqual(svg.getroot().attrib["viewBox"], "0 0 512 512")
        links = 0
        for page in (ROOT / "web").rglob("*.html"):
            for href in re.findall(r'<link rel="icon"[^>]+href="([^"]+)"', page.read_text()):
                self.assertEqual((page.parent / href).resolve(), ROOT / "web/logo.svg")
                links += 1
        self.assertGreaterEqual(links, 16)
        self.assertIn("entries=(logo.svg", (ROOT / "scripts/site-artifact.sh").read_text())

    def test_native_png_dimensions(self):
        for name, size in (("icon.png", 256), ("icon-512.png", 512)):
            blob = (ASSETS / name).read_bytes()
            self.assertEqual(blob[:8], b"\x89PNG\r\n\x1a\n")
            self.assertEqual(struct.unpack(">II", blob[16:24]), (size, size))
            self.assertEqual(blob[25], 6)  # RGBA, including transparent outside corners.

    def test_ico_directory_matches_png_payloads(self):
        blob = (ASSETS / "icon.ico").read_bytes()
        self.assertEqual(struct.unpack("<HHH", blob[:6]), (0, 1, 6))
        for i, size in enumerate((16, 32, 48, 64, 128, 256)):
            w, h, colors, reserved, planes, depth, length, offset = struct.unpack_from("<BBBBHHII", blob, 6 + i * 16)
            self.assertEqual((w or 256, h or 256, planes, depth), (size, size, 1, 32))
            png = blob[offset:offset + length]
            self.assertEqual(png[:8], b"\x89PNG\r\n\x1a\n")
            self.assertEqual(struct.unpack(">II", png[16:24]), (size, size))
            self.assertLessEqual(offset + length, len(blob))

    def test_icns_chunks_match_png_sizes(self):
        blob = (ASSETS / "icon.icns").read_bytes()
        self.assertEqual(blob[:4], b"icns")
        self.assertEqual(struct.unpack(">I", blob[4:8])[0], len(blob))
        expected = {b"icp4": 16, b"icp5": 32, b"icp6": 64, b"ic07": 128, b"ic08": 256,
                    b"ic09": 512, b"ic10": 1024, b"ic11": 32, b"ic12": 64, b"ic13": 256, b"ic14": 512}
        cursor, found = 8, set()
        while cursor < len(blob):
            tag, length = struct.unpack_from(">4sI", blob, cursor)
            self.assertGreater(length, 32)
            png = blob[cursor + 8:cursor + length]
            self.assertEqual(struct.unpack(">II", png[16:24]), (expected[tag], expected[tag]))
            found.add(tag)
            cursor += length
        self.assertEqual(found, set(expected))
        self.assertEqual(cursor, len(blob))

    def test_desktop_and_window_identity_match(self):
        desktop = ROOT / f"packaging/linux/{APP_ID}.desktop"
        self.assertIn(f"StartupWMClass={APP_ID}", desktop.read_text())
        self.assertIn(f".with_app_id(\"{APP_ID}\")", (ROOT / "crates/animatix-gui/src/app/runtime/run.rs").read_text())
        if shutil.which("desktop-file-validate"):
            subprocess.run(["desktop-file-validate", str(desktop)], check=True)

    def package_fixture(self, temp):
        package = Path(temp) / 'Animatix 100% "preview"'
        (package / "bin").mkdir(parents=True)
        binary = package / "bin/animatix-gui"
        binary.write_text("#!/bin/sh\nexit 0\n")
        binary.chmod(0o755)
        desktop = package / f"share/applications/{APP_ID}.desktop"
        desktop.parent.mkdir(parents=True)
        shutil.copyfile(ROOT / f"packaging/linux/{APP_ID}.desktop", desktop)
        for size, ext, source in (("scalable", "svg", ROOT / "logo.svg"), ("256x256", "png", ASSETS / "icon.png"), ("512x512", "png", ASSETS / "icon-512.png")):
            dest = package / f"share/icons/hicolor/{size}/apps/{APP_ID}.{ext}"
            dest.parent.mkdir(parents=True)
            shutil.copyfile(source, dest)
        return package

    def test_user_install_repeat_and_paths(self):
        with tempfile.TemporaryDirectory() as temp:
            package = self.package_fixture(temp)
            data = Path(temp) / "user-data"
            desktop = installer.install(package, data)
            self.assertIn("Exec=" + installer.exec_quote(package / "bin/animatix-gui") + " %f", desktop.read_text())
            self.assertIn("100%%", desktop.read_text())
            self.assertEqual(installer.install(package, data), desktop)
            self.assertTrue((data / f"icons/hicolor/scalable/apps/{APP_ID}.svg").is_file())
            if shutil.which("desktop-file-validate"):
                subprocess.run(["desktop-file-validate", str(desktop)], check=True)

    def test_foreign_launcher_and_relative_data_home_are_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            package = self.package_fixture(temp)
            data = Path(temp) / "data"
            desktop = data / f"applications/{APP_ID}.desktop"
            desktop.parent.mkdir(parents=True)
            desktop.write_text("unrelated custom launcher")
            with self.assertRaisesRegex(ValueError, "unmanaged"):
                installer.install(package, data)
            self.assertEqual(desktop.read_text(), "unrelated custom launcher")
            with self.assertRaisesRegex(ValueError, "absolute"):
                installer.install(package, Path("relative"))


if __name__ == "__main__":
    unittest.main()
