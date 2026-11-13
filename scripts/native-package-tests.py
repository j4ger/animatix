#!/usr/bin/env python3
"""Boundary fixtures for the executable format and closure checks."""
from pathlib import Path
import runpy
import struct
import tempfile
import os
import tarfile
import unittest

api = runpy.run_path(str(Path(__file__).with_name("native-package-check.py")))
source_api = runpy.run_path(str(Path(__file__).resolve().parent.parent / "packaging/linux/source-package.py"))
ROOT = Path(__file__).resolve().parent.parent
ICON_SOURCE_FILES = (
    "logo.svg", "scripts/update-icons.py", "crates/animatix-gui/build.rs",
    "crates/animatix-gui/assets/icon.png", "crates/animatix-gui/assets/icon-512.png",
    "crates/animatix-gui/assets/icon.ico", "crates/animatix-gui/assets/icon.icns",
    "packaging/linux/io.github.j4ger.animatix.desktop", "packaging/linux/install-desktop.py",
)


def add_icon_sources(root):
    for name in ICON_SOURCE_FILES:
        target = root / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes((ROOT / name).read_bytes())


def macho(dependency="/usr/lib/libSystem.B.dylib", cpu=0x100000C):
    name = dependency.encode() + b"\0"
    size = (24 + len(name) + 7) & ~7
    command = struct.pack("<IIIIII", 12, size, 24, 0, 0, 0)
    header = struct.pack("<IIIIIIII", 0xFEEDFACF, cpu, 0, 2, 1, size, 0, 0)
    return header + command + name + bytes(size - 24 - len(name))


def pe(dependency="KERNEL32.dll", machine=0x8664):
    data = bytearray(1024)
    struct.pack_into("<I", data, 0x3C, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<HHIIIHH", data, 0x84, machine, 1, 0, 0, 0, 240, 0)
    struct.pack_into("<H", data, 0x98, 0x20B)
    struct.pack_into("<II", data, 0x98 + 120, 0x1000, 40)
    struct.pack_into("<IIII", data, 0x98 + 240 + 8, 512, 0x1000, 512, 0x200)
    struct.pack_into("<I", data, 0x200 + 12, 0x1040)
    name = dependency.encode() + b"\0"
    data[0x240:0x240 + len(name)] = name
    return data


class PackageTests(unittest.TestCase):
    def package(self, platform, cli, gui=None):
        temporary = tempfile.TemporaryDirectory(prefix="animatix-native-check-")
        self.addCleanup(temporary.cleanup)
        directory = Path(temporary.name)
        extension = ".exe" if platform == "windows" else ""
        (directory / ("animatix" + extension)).write_bytes(cli)
        (directory / ("animatix-gui" + extension)).write_bytes(gui or cli)
        return api["check"](platform, directory)

    def test_macos_architecture_and_system_dependencies(self):
        result = self.package("macos", macho())
        self.assertEqual(result["architecture"], "aarch64")
        self.assertFalse(result["video"])
        self.assertEqual(len(result["binaries"]), 2)
        self.assertEqual(self.package("macos", macho(cpu=0x1000007))["architecture"], "x86_64")

    def test_macos_architecture_mismatch_and_private_dependencies(self):
        with self.assertRaisesRegex(ValueError, "architectures differ"):
            self.package("macos", macho(), macho(cpu=0x1000007))
        for name in ("/nix/store/build/libfoo.dylib", "/usr/lib/libavcodec.dylib", "@rpath/libfoo.dylib"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.package("macos", macho(name))

    def test_windows_imports_and_architecture(self):
        result = self.package("windows", pe())
        self.assertEqual(result["architecture"], "x86_64")
        self.assertEqual(result["binaries"][0]["dependencies"], ["KERNEL32.dll"])
        with self.assertRaisesRegex(ValueError, "unsupported Windows architecture"):
            self.package("windows", pe(machine=0xAA64))

    def test_windows_private_and_video_dependencies_fail(self):
        for name in ("custom.dll", "avcodec-62.dll", "libavcodec-62.dll"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.package("windows", pe(name))

    def test_malformed_executables_cannot_pass(self):
        for platform in ("windows", "macos"):
            with self.subTest(platform=platform), self.assertRaises((ValueError, struct.error)):
                self.package(platform, b"wrong binary")

    def test_linux_bundle_carries_project_license_and_source_identity(self):
        container_build = (ROOT / "scripts/dist-linux-container.sh").read_text()
        runtime_check = (ROOT / "packaging/linux/test-runtime.sh").read_text()
        self.assertIn('install -m 0644 "$project/LICENSE" "$stage/share/licenses/animatix/LICENSE"', container_build)
        for field in ("source_commit", "source_dirty", "source_tree_sha256"):
            self.assertIn(f"{field}=", container_build)
            self.assertIn(f"^{field}=", runtime_check)
        self.assertIn("share/licenses/animatix/LICENSE", runtime_check)
        self.assertIn('source-package.py" package', container_build)
        self.assertIn("corresponding_source_sha256=", container_build)

    def test_source_snapshot_includes_dirty_allowlisted_files_but_excludes_builds_and_secrets(self):
        with tempfile.TemporaryDirectory(prefix="animatix-source-snapshot-") as temporary:
            root = Path(temporary)
            subprocess = __import__("subprocess")
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            subprocess.run(["git", "config", "user.email", "test@example.invalid"], cwd=root, check=True)
            subprocess.run(["git", "config", "user.name", "Source Test"], cwd=root, check=True)
            (root / "Cargo.toml").write_text("[workspace]\n")
            (root / "Cargo.lock").write_text("version = 4\n")
            (root / "LICENSE").write_text("license\n")
            (root / "crates").mkdir(); (root / "crates/core.rs").write_text("tracked\n")
            (root / "crates/server.pem").write_text("tracked certificate fixture\n")
            subprocess.run(["git", "add", "Cargo.toml", "Cargo.lock", "LICENSE", "crates/core.rs", "crates/server.pem"], cwd=root, check=True)
            subprocess.run(["git", "commit", "-qm", "fixture"], cwd=root, check=True)
            (root / "crates/dirty.rs").write_text("untracked source\n")
            (root / "crates/.env.api").write_text("secret\n")
            (root / "target").mkdir(); (root / "target/build.rs").write_text("ignored build output\n")
            add_icon_sources(root)
            destination = root / "target/source-copy"
            identity = source_api["snapshot"](root, destination)
            files = {entry["path"] for entry in identity["files"]}
            self.assertTrue(set(ICON_SOURCE_FILES).issubset(files))
            self.assertIn("crates/dirty.rs", files)
            self.assertIn("crates/server.pem", files)
            self.assertNotIn("crates/.env.api", files)
            self.assertNotIn("target/build.rs", files)
            self.assertFalse((destination / ".git").exists())
            self.assertEqual(source_api["source_manifest"](destination)[0], identity["source_tree_sha256"])

    def test_source_archive_is_self_contained_checksummed_and_deterministic(self):
        with tempfile.TemporaryDirectory(prefix="animatix-source-archive-") as temporary:
            root = Path(temporary)
            project = root / "project"; project.mkdir()
            (project / "Cargo.toml").write_text("[workspace]\n")
            (project / "Cargo.lock").write_text("version = 4\n")
            (project / "LICENSE").write_text("license\n")
            (project / "crates").mkdir(); (project / "crates/core.rs").write_text("source\n")
            add_icon_sources(project)
            media = root / "media"; media.mkdir()
            lock_values = []
            for key, archive in zip(source_api["MEDIA_KEYS"], ("ffmpeg.tar.xz", "x264.tar.gz", "vpx.tar.gz", "opus.tar.gz")):
                payload = f"source:{archive}".encode()
                (media / archive).write_bytes(payload)
                checksum_key = {"FFMPEG_ARCHIVE": "FFMPEG_SHA256", "X264_ARCHIVE": "X264_SHA256", "LIBVPX_ARCHIVE": "LIBVPX_SHA256", "OPUS_ARCHIVE": "OPUS_SHA256"}[key]
                lock_values.extend([f"{key}='{archive}'", f"{checksum_key}='{__import__('hashlib').sha256(payload).hexdigest()}'"])
            (project / "packaging/linux").mkdir(parents=True, exist_ok=True)
            (project / "packaging/linux/sources.lock").write_text("\n".join(lock_values) + "\n")
            with __import__("unittest.mock", fromlist=["patch"]).patch.dict(os.environ, {
                "ANIMATIX_SOURCE_COMMIT": "b" * 40, "ANIMATIX_SOURCE_DIRTY": "false",
            }):
                identity = source_api["snapshot"](project, root / "snapshot")
            vendor = root / "snapshot/cargo-vendor/example-1.0/src"; vendor.mkdir(parents=True)
            (vendor / "lib.rs").write_text("vendored\n")
            vendor_target = vendor / "target"; vendor_target.mkdir()
            (vendor_target / "supported.rs").write_text("legitimate vendor source\n")
            vendor_tests = root / "snapshot/cargo-vendor/example-1.0/tests"; vendor_tests.mkdir()
            (vendor_tests / "server.key").write_text("fixture key\n")
            (root / "snapshot/.cargo").mkdir()
            config = root / "snapshot/.cargo/config.toml"
            config.write_text('[source.crates-io]\nreplace-with = "vendored-sources"\n')
            vendor_digest, vendor_count = source_api["tree_digest"](root / "snapshot/cargo-vendor")
            identity["cargo_vendor_sha256"] = vendor_digest
            identity["cargo_vendor_files"] = vendor_count
            identity["cargo_config_sha256"] = __import__("hashlib").sha256(config.read_bytes()).hexdigest()
            (root / "snapshot/SOURCE-IDENTITY.json").write_text(__import__("json").dumps(identity, indent=2) + "\n")
            first, second = root / "first.tar.gz", root / "second.tar.gz"
            source_api["package"](root / "snapshot", media, first, "0.1.0")
            unpacked = root / "unpacked"; unpacked.mkdir()
            with tarfile.open(first, "r:gz") as archive:
                archive.extractall(unpacked)
            extracted_source = next(unpacked.iterdir())
            for name in ICON_SOURCE_FILES:
                self.assertEqual((extracted_source / name).read_bytes(), (ROOT / name).read_bytes())
            with __import__("unittest.mock", fromlist=["patch"]).patch.dict(os.environ, {
                "ANIMATIX_SOURCE_COMMIT": "b" * 40, "ANIMATIX_SOURCE_DIRTY": "false",
                "ANIMATIX_SOURCE_TREE_SHA256": identity["source_tree_sha256"],
            }):
                source_api["snapshot"](extracted_source, root / "snapshot-again")
            source_api["package"](root / "snapshot-again", media, second, "0.1.0")
            self.assertEqual(first.read_bytes(), second.read_bytes())
            self.assertEqual(source_api["verify_archive"](first)["source_tree_sha256"], identity["source_tree_sha256"])

    def test_existing_offline_vendor_and_config_are_checked_before_cargo_metadata(self):
        with tempfile.TemporaryDirectory(prefix="animatix-existing-vendor-") as temporary:
            root = Path(temporary)
            vendor_file = root / "cargo-vendor/example-1.0/src/lib.rs"
            vendor_file.parent.mkdir(parents=True)
            vendor_file.write_text("vendored source\n")
            config = root / ".cargo/config.toml"
            config.parent.mkdir()
            config.write_text("[net]\noffline = true\n")
            vendor_hash, vendor_count = source_api["tree_digest"](root / "cargo-vendor")
            identity = {
                "cargo_vendor_sha256": vendor_hash,
                "cargo_vendor_files": vendor_count,
                "cargo_config_sha256": __import__("hashlib").sha256(config.read_bytes()).hexdigest(),
            }
            (root / "SOURCE-IDENTITY.json").write_text(__import__("json").dumps(identity))
            subprocess_module = source_api["subprocess"]
            with __import__("unittest.mock", fromlist=["patch"]).patch.object(subprocess_module, "run") as run:
                source_api["offline_vendor"](root)
                run.assert_called_once()

            vendor_file.write_text("modified vendor source\n")
            with __import__("unittest.mock", fromlist=["patch"]).patch.object(subprocess_module, "run") as run:
                with self.assertRaisesRegex(ValueError, "Cargo vendor set"):
                    source_api["offline_vendor"](root)
                run.assert_not_called()

            vendor_file.write_text("vendored source\n")
            config.write_text("[net]\noffline = false\n")
            with __import__("unittest.mock", fromlist=["patch"]).patch.object(subprocess_module, "run") as run:
                with self.assertRaisesRegex(ValueError, "Cargo vendor config"):
                    source_api["offline_vendor"](root)
                run.assert_not_called()

    def test_source_manifest_rejects_duplicate_canonical_cargo_config_paths(self):
        with tempfile.TemporaryDirectory(prefix="animatix-config-collision-") as temporary:
            root = Path(temporary)
            (root / "Cargo.toml").write_text("[workspace]\n")
            (root / "Cargo.lock").write_text("version = 4\n")
            (root / "LICENSE").write_text("license\n")
            cargo = root / ".cargo"
            cargo.mkdir()
            (cargo / "config.toml").write_text("[build]\njobs = 1\n")
            (cargo / "project-config.toml").write_text("[build]\njobs = 2\n")
            with self.assertRaisesRegex(ValueError, "multiple source files map"):
                source_api["source_manifest"](root)

    def test_linux_bundle_uses_a_logical_ffmpeg_prefix(self):
        container_build = (ROOT / "scripts/dist-linux-container.sh").read_text()
        dockerfile = (ROOT / "packaging/linux/Dockerfile").read_text()
        ffmpeg_build = container_build.split('cd "$src/ffmpeg-$FFMPEG_VERSION"', 1)[1].split("make distclean", 1)[0]
        self.assertIn("./configure --prefix=/usr", ffmpeg_build)
        self.assertIn('make DESTDIR="$prefix" install', container_build)
        self.assertNotIn('./configure --prefix="$prefix"', ffmpeg_build)
        self.assertNotIn('--extra-cflags="-I$prefix', ffmpeg_build)
        self.assertNotIn('--extra-ldflags="-L$prefix', ffmpeg_build)
        self.assertNotIn("binding_headers=$FFMPEG_INCLUDE_DIR", container_build)
        self.assertIn("clang-14 curl file git", dockerfile)

    def test_linux_runtime_documents_host_alsa_contract(self):
        readme = (ROOT / "packaging/linux/README.md").read_text()
        self.assertIn("/usr/share/alsa/alsa.conf", readme)
        self.assertIn("PCM plugins", readme)
        self.assertIn("audio playback is unavailable", readme)

    def test_linux_release_gate_exercises_the_extracted_archive(self):
        ci = (ROOT / "scripts/ci.sh").read_text()
        gate = ci.split("gate_dist_linux() {", 1)[1].split("\ngate_dist_macos()", 1)[0]
        self.assertIn('scripts/dist-package.sh linux target/dist-linux-package', gate)
        self.assertIn('tar -xzf "dist/animatix-', gate)
        self.assertIn('bash scripts/test-dist-linux.sh "$unpacked"', gate)


if __name__ == "__main__":
    unittest.main()
