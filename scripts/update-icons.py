#!/usr/bin/env python3
"""Export checked-in app icons from logo.svg; requires Inkscape, no Python packages."""
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SIZES = (16, 32, 48, 64, 128, 256, 512, 1024)


def main():
    inkscape = shutil.which("inkscape")
    if not inkscape:
        raise SystemExit("Install Inkscape to regenerate icons")
    assets = ROOT / "crates/animatix-gui/assets"
    assets.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="animatix-icons-") as tmp:
        images = {}
        for size in SIZES:
            out = Path(tmp) / f"{size}.png"
            subprocess.run([inkscape, str(ROOT / "logo.svg"), f"--export-filename={out}",
                            f"--export-width={size}", f"--export-height={size}"], check=True)
            images[size] = out.read_bytes()
        # Windows Vista and newer support PNG-compressed ICO entries at every size.
        ico_sizes = SIZES[:6]
        offset = 6 + 16 * len(ico_sizes)
        directory = bytearray(struct.pack("<HHH", 0, 1, len(ico_sizes)))
        payload = bytearray()
        for size in ico_sizes:
            data = images[size]
            directory += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset)
            payload += data
            offset += len(data)
        (assets / "icon.ico").write_bytes(directory + payload)
        # Modern macOS ICNS chunks hold lossless PNGs, including Retina variants.
        chunks = bytearray()
        for tag, size in ((b"icp4", 16), (b"icp5", 32), (b"icp6", 64), (b"ic07", 128),
                          (b"ic08", 256), (b"ic09", 512), (b"ic10", 1024),
                          (b"ic11", 32), (b"ic12", 64), (b"ic13", 256), (b"ic14", 512)):
            data = images[size]
            chunks += tag + struct.pack(">I", len(data) + 8) + data
        (assets / "icon.icns").write_bytes(b"icns" + struct.pack(">I", len(chunks) + 8) + chunks)
        (assets / "icon.png").write_bytes(images[256])
        (assets / "icon-512.png").write_bytes(images[512])
    shutil.copyfile(ROOT / "logo.svg", ROOT / "web/logo.svg")
    print("Updated web SVG, native PNGs, ICO, and ICNS")


if __name__ == "__main__":
    main()
