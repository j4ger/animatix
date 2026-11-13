#!/usr/bin/env python3
"""Build a relocatable Finder application alongside the command-line binaries."""
from pathlib import Path
import plistlib
import re
import shutil
import sys

ROOT = Path(__file__).resolve().parent.parent


def package(binary: Path, destination: Path, version: str):
    """The caller owns a fresh staging directory; never merge a stale bundle."""
    match = re.fullmatch(r"(\d+\.\d+\.\d+)(?:[-+].*)?", version)
    if not match:
        raise ValueError(f"invalid application version: {version}")
    contents = destination / "Contents"
    (contents / "MacOS").mkdir(parents=True)
    (contents / "Resources").mkdir()
    executable = contents / "MacOS/animatix-gui"
    shutil.copy2(binary, executable)
    executable.chmod(executable.stat().st_mode | 0o111)
    shutil.copyfile(ROOT / "crates/animatix-gui/assets/icon.icns", contents / "Resources/Animatix.icns")
    info = {
        "CFBundleDevelopmentRegion": "en",
        "CFBundleDisplayName": "Animatix",
        "CFBundleExecutable": "animatix-gui",
        "CFBundleIdentifier": "io.github.j4ger.animatix",
        "CFBundleIconFile": "Animatix.icns",
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleName": "Animatix",
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": match[1],
        "CFBundleVersion": match[1],
        "LSApplicationCategoryType": "public.app-category.graphics-design",
        "NSHighResolutionCapable": True,
    }
    with (contents / "Info.plist").open("wb") as output:
        plistlib.dump(info, output, sort_keys=True)
    (contents / "PkgInfo").write_bytes(b"APPL????")


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit("usage: package-macos.py GUI_BINARY APP_DESTINATION VERSION")
    package(Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3])
