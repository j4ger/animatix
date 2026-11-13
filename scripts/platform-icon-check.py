#!/usr/bin/env python3
"""Fail packaging if the app bundle / Windows executable has lost its icon."""
from pathlib import Path
import plistlib
import struct
import sys


def check_macos(bundle):
    contents = Path(bundle) / "Contents"
    with (contents / "Info.plist").open("rb") as source:
        info = plistlib.load(source)
    assert info["CFBundleIdentifier"] == "io.github.j4ger.animatix", "wrong bundle identity"
    assert info["CFBundlePackageType"] == "APPL", "not an application bundle"
    assert info["CFBundleExecutable"] == "animatix-gui", "wrong bundle executable"
    assert info["CFBundleIconFile"] == "Animatix.icns", "wrong bundle icon reference"
    executable = contents / "MacOS" / info["CFBundleExecutable"]
    assert executable.is_file() and executable.stat().st_mode & 0o111, "missing executable"
    icon = (contents / "Resources" / info["CFBundleIconFile"]).read_bytes()
    assert icon[:4] == b"icns" and struct.unpack_from(">I", icon, 4)[0] == len(icon), "invalid ICNS header"
    cursor, chunks = 8, []
    while cursor < len(icon):
        kind, length = struct.unpack_from(">4sI", icon, cursor)
        assert length > 8 and cursor + length <= len(icon), "invalid ICNS element"
        chunks.append(kind)
        cursor += length
    assert cursor == len(icon) and chunks, "empty or truncated ICNS"
    return info


def check_windows(binary):
    data = Path(binary).read_bytes()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    assert data[pe:pe + 4] == b"PE\0\0", "invalid PE signature"
    sections, optional_size = struct.unpack_from("<H12xH", data, pe + 6)
    optional = pe + 24
    magic = struct.unpack_from("<H", data, optional)[0]
    assert magic in (0x10B, 0x20B), "unsupported PE optional header"
    directory = optional + (112 if magic == 0x20B else 96)
    resource_rva, resource_size = struct.unpack_from("<II", data, directory + 16)
    assert resource_rva and resource_size, "Windows GUI has no embedded resources"

    def file_offset(rva, length=1):
        for index in range(sections):
            section = optional + optional_size + index * 40
            virtual_size, address, raw_size, offset = struct.unpack_from("<IIII", data, section + 8)
            relative = rva - address
            if 0 <= relative and relative + length <= raw_size:
                assert offset + relative + length <= len(data), "truncated resource data"
                return offset + relative
        raise ValueError("unmapped resource RVA")

    base = file_offset(resource_rva)
    resources = {}

    def visit(relative, path):
        assert len(path) <= 3 and relative + 16 <= resource_size, "invalid resource tree"
        named, numeric = struct.unpack_from("<HH", data, base + relative + 12)
        assert relative + 16 + (named + numeric) * 8 <= resource_size, "truncated resource tree"
        for index in range(named + numeric):
            key, target = struct.unpack_from("<II", data, base + relative + 16 + index * 8)
            if key & 0x80000000:
                continue  # Icon resources emitted by winresource use numeric IDs.
            child = path + (key,)
            if target & 0x80000000:
                visit(target & 0x7FFFFFFF, child)
            else:
                assert target + 16 <= resource_size, "invalid resource leaf"
                rva, size = struct.unpack_from("<II", data, base + target)
                start = file_offset(rva, size)
                resources[child] = data[start:start + size]

    visit(0, ())
    icons = {key[1]: payload for key, payload in resources.items() if len(key) == 3 and key[0] == 3}
    groups = [payload for key, payload in resources.items() if len(key) == 3 and key[0] == 14]
    assert icons and groups, "Windows GUI is missing RT_ICON / RT_GROUP_ICON"
    sizes = set()
    for group in groups:
        reserved, kind, count = struct.unpack_from("<HHH", group)
        assert reserved == 0 and kind == 1 and count > 0, "invalid group icon"
        assert len(group) == 6 + count * 14, "truncated group icon"
        for index in range(count):
            width, height, _, _, _, _, length, resource_id = struct.unpack_from("<BBBBHHIH", group, 6 + index * 14)
            assert resource_id in icons and len(icons[resource_id]) == length, "group icon references missing image"
            sizes.add((width or 256, height or 256))
    return sorted(sizes)


if __name__ == "__main__":
    if len(sys.argv) != 3 or sys.argv[1] not in {"macos", "windows"}:
        raise SystemExit("usage: platform-icon-check.py <macos|windows> BUNDLE_OR_EXE")
    result = check_macos(sys.argv[2]) if sys.argv[1] == "macos" else check_windows(sys.argv[2])
    print(f"{sys.argv[1]} native icon: ok ({result})")
