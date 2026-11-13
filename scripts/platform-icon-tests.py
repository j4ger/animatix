#!/usr/bin/env python3
"""Cross-platform fixtures, not claims of Finder/Explorer visual verification."""
from pathlib import Path
import os
import plistlib
import runpy
import shutil
import struct
import subprocess
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parent.parent
CHECK = runpy.run_path(str(ROOT / "scripts/platform-icon-check.py"))


def icns():
    chunk = b"ic07" + struct.pack(">I", 16) + b"fixture!"
    return b"icns" + struct.pack(">I", len(chunk) + 8) + chunk


def pe_with_icon(missing_image=False):
    image = b"fixture icon bytes"
    group = struct.pack("<HHHBBBBHHIH", 0, 1, 1, 32, 32, 0, 0, 1, 32, len(image), 99 if missing_image else 1)
    tree = {3: {1: {1033: image}}, 14: {1: {1033: group}}}
    blob = bytearray()

    def directory(children):
        position = len(blob)
        blob.extend(bytes(16 + len(children) * 8))
        struct.pack_into("<HH", blob, position + 12, 0, len(children))
        for index, (key, value) in enumerate(children.items()):
            if isinstance(value, dict):
                target = directory(value) | 0x80000000
            else:
                target = len(blob)
                blob.extend(bytes(16))
                start = len(blob)
                blob.extend(value)
                blob.extend(bytes((-len(blob)) % 4))
                struct.pack_into("<II", blob, target, 0x1000 + start, len(value))
            struct.pack_into("<II", blob, position + 16 + index * 8, key, target)
        return position

    directory(tree)
    data = bytearray(0x200 + len(blob))
    struct.pack_into("<I", data, 0x3C, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<HHIIIHH", data, 0x84, 0x8664, 1, 0, 0, 0, 240, 0)
    struct.pack_into("<H", data, 0x98, 0x20B)
    struct.pack_into("<II", data, 0x98 + 128, 0x1000, len(blob))
    struct.pack_into("<IIII", data, 0x98 + 240 + 8, len(blob), 0x1000, len(blob), 0x200)
    data[0x200:] = blob
    return data


class PlatformIconTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="animatix-platform-icons-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def test_windows_resource_tree_resolves_group_images(self):
        binary = self.root / "gui.exe"
        binary.write_bytes(pe_with_icon())
        self.assertEqual(CHECK["check_windows"](binary), [(32, 32)])
        binary.write_bytes(pe_with_icon(missing_image=True))
        with self.assertRaisesRegex(AssertionError, "missing image"):
            CHECK["check_windows"](binary)

    def test_windows_missing_and_truncated_resources_fail(self):
        binary = self.root / "gui.exe"
        data = pe_with_icon()
        struct.pack_into("<II", data, 0x98 + 128, 0, 0)
        binary.write_bytes(data)
        with self.assertRaisesRegex(AssertionError, "no embedded resources"):
            CHECK["check_windows"](binary)
        binary.write_bytes(pe_with_icon()[:540])
        with self.assertRaises((AssertionError, struct.error)):
            CHECK["check_windows"](binary)

    def fixture_project(self):
        (self.root / "scripts").mkdir()
        for name in ("dist-package.sh", "native-package-check.py", "package-macos.py", "platform-icon-check.py"):
            shutil.copyfile(ROOT / "scripts" / name, self.root / "scripts" / name)
        assets = self.root / "crates/animatix-gui/assets"
        assets.mkdir(parents=True)
        (assets / "icon.icns").write_bytes(icns())
        (self.root / "Cargo.toml").write_text('[workspace.package]\nversion = "0.1.0"\n')
        (self.root / "LICENSE").write_text("fixture license\n")
        (self.root / "bin").mkdir()

    def package(self, platform):
        env = dict(os.environ)
        env.pop("ANIMATIX_RELEASE_VERSION", None)
        env.pop("ANIMATIX_RELEASE_LABEL", None)
        subprocess.run(["bash", "scripts/dist-package.sh", platform, "bin"], cwd=self.root,
                       env=env, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        return next((self.root / "dist").glob("*.zip"))

    def test_macos_archive_contains_relocatable_bundle_and_cli(self):
        self.fixture_project()
        # Dependency-free arm64 Mach-O header for the structural package checker.
        binary = struct.pack("<IIIIIIII", 0xFEEDFACF, 0x100000C, 0, 2, 0, 0, 0, 0)
        for name in ("animatix", "animatix-gui"):
            (self.root / "bin" / name).write_bytes(binary)
        archive = self.package("macos")
        with zipfile.ZipFile(archive) as zipped:
            self.assertIn("animatix", zipped.namelist())
            prefix = "Animatix.app/Contents/"
            info = plistlib.loads(zipped.read(prefix + "Info.plist"))
            self.assertEqual(info["CFBundleVersion"], "0.1.0")
            self.assertEqual(info["CFBundleIconFile"], "Animatix.icns")
            self.assertEqual(zipped.read(prefix + "MacOS/animatix-gui"), binary)
            self.assertEqual(zipped.read(prefix + "Resources/Animatix.icns"), icns())
            self.assertTrue(zipped.getinfo(prefix + "MacOS/animatix-gui").external_attr >> 16 & 0o111)
        bundle = self.root / "target/native-package-macos/Animatix.app"
        CHECK["check_macos"](bundle)
        # It remains complete when moved away from the root command-line binaries.
        shutil.move(str(bundle), str(self.root / "Moved.app"))
        CHECK["check_macos"](self.root / "Moved.app")

    def test_windows_archive_requires_embedded_icon(self):
        self.fixture_project()
        for name in ("animatix.exe", "animatix-gui.exe"):
            (self.root / "bin" / name).write_bytes(pe_with_icon())
        with zipfile.ZipFile(self.package("windows")) as zipped:
            self.assertEqual(zipped.read("animatix-gui.exe"), pe_with_icon())
        data = pe_with_icon()
        struct.pack_into("<II", data, 0x98 + 128, 0, 0)
        (self.root / "bin/animatix-gui.exe").write_bytes(data)
        with self.assertRaises(subprocess.CalledProcessError):
            self.package("windows")

    def test_build_script_uses_target_not_host_and_tracks_icon(self):
        source = (ROOT / "crates/animatix-gui/build.rs").read_text()
        self.assertIn('CARGO_CFG_TARGET_OS', source)
        self.assertIn('cargo:rerun-if-changed=assets/icon.ico', source)
        self.assertIn('.set_icon("assets/icon.ico")', source)
        manifest = (ROOT / "crates/animatix-gui/Cargo.toml").read_text()
        self.assertIn('[build-dependencies]', manifest)
        self.assertIn('winresource = { version = "0.1.34", default-features = false }', manifest)


if __name__ == "__main__":
    unittest.main()
