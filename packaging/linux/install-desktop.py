#!/usr/bin/env python3
"""Install this unpacked package's launcher/icons for the current user only."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess

APP_ID = "io.github.j4ger.animatix"


def exec_quote(path):
    """Quote one Desktop Entry Exec argument (not shell syntax)."""
    value = str(path)
    if "\n" in value or "\r" in value:
        raise ValueError("package path must not contain line breaks")
    # Exec parsing removes two layers of escaping for a literal backslash.
    value = value.replace("\\", "\\\\\\\\").replace('"', '\\"')
    value = value.replace("`", "\\`").replace("$", "\\$").replace("%", "%%")
    return '"' + value + '"'


def install(package, data_home):
    package = Path(package).resolve()
    data_home = Path(data_home).expanduser()
    if not data_home.is_absolute():
        raise ValueError("XDG_DATA_HOME must be an absolute path")
    binary = package / "bin/animatix-gui"
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise ValueError(f"missing executable: {binary}")
    desktop_source = package / f"share/applications/{APP_ID}.desktop"
    icon_paths = [Path(f"icons/hicolor/{size}/apps/{APP_ID}.{ext}")
                  for size, ext in (("scalable", "svg"), ("256x256", "png"), ("512x512", "png"))]
    for relative in icon_paths:
        if not (package / "share" / relative).is_file():
            raise ValueError(f"missing package icon: {relative}")
    desktop = data_home / f"applications/{APP_ID}.desktop"
    if desktop.exists() and "X-Animatix-Managed=true" not in desktop.read_text():
        raise ValueError(f"refusing to overwrite an unmanaged launcher: {desktop}")
    text = desktop_source.read_text()
    if "Exec=animatix-gui %f" not in text:
        raise ValueError("unexpected desktop template")
    text = text.replace("Exec=animatix-gui %f", f"Exec={exec_quote(binary)} %f")
    # Validate everything before creating any entries.
    desktop.parent.mkdir(parents=True, exist_ok=True)
    for relative in icon_paths:
        destination = data_home / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(package / "share" / relative, destination)
    desktop.write_text(text)
    desktop.chmod(0o644)
    # Refresh existing icon caches when available; no permissions/default apps change.
    cache_tool = shutil.which("gtk-update-icon-cache")
    if cache_tool:
        subprocess.run([cache_tool, "--force", "--ignore-theme-index", str(data_home / "icons/hicolor")],
                       check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return desktop


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package-root", type=Path, default=Path(__file__).resolve().parent)
    args = parser.parse_args()
    data_home = os.environ.get("XDG_DATA_HOME") or str(Path.home() / ".local/share")
    try:
        desktop = install(args.package_root, data_home)
    except (ValueError, OSError) as error:
        parser.exit(1, f"error: {error}\n")
    print(f"Installed launcher: {desktop}")
    print("Keep the unpacked package in place. Rerun this installer after moving it.")
    print("No root permissions, MIME defaults, or system settings were changed.")


if __name__ == "__main__":
    main()
