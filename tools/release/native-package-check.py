#!/usr/bin/env python3
"""Read the shipped binary's architecture/dependencies without executing tools."""
import json
from pathlib import Path
import struct
import sys


WINDOWS_SYSTEM_DLLS = set("""
advapi32.dll avrt.dll bcrypt.dll cfgmgr32.dll comctl32.dll comdlg32.dll crypt32.dll d2d1.dll
d3d11.dll d3d12.dll dcomp.dll dinput8.dll dwmapi.dll dwrite.dll dxgi.dll dxva2.dll gdi32.dll
hid.dll imm32.dll iphlpapi.dll kernel32.dll ksuser.dll mf.dll mfplat.dll mfreadwrite.dll
mmdevapi.dll msacm32.dll msimg32.dll msvcrt.dll ncrypt.dll netapi32.dll normaliz.dll ntdll.dll
ole32.dll oleaut32.dll opengl32.dll powrprof.dll propsys.dll psapi.dll rpcrt4.dll secur32.dll
setupapi.dll shcore.dll shell32.dll shlwapi.dll ucrtbase.dll user32.dll userenv.dll usp10.dll
uxtheme.dll version.dll win32u.dll winhttp.dll winspool.drv winmm.dll wintrust.dll ws2_32.dll wtsapi32.dll
xinput1_4.dll
""".split())
WINDOWS_SYSTEM_DLLS.update({
    "avicap32.dll", "bcryptprimitives.dll", "dxcore.dll", "msasn1.dll",
    "msvfw32.dll", "mswsock.dll", "windowscodecs.dll", "wsock32.dll",
})


def cstring(data, offset):
    end = data.index(b"\0", offset)
    return data[offset:end].decode("utf-8")


def pe(data):
    pe_offset = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe_offset:pe_offset + 4] != b"PE\0\0":
        raise ValueError("invalid PE signature")
    machine, count, _, _, _, optional_size, _ = struct.unpack_from("<HHIIIHH", data, pe_offset + 4)
    if machine != 0x8664:
        raise ValueError(f"unsupported Windows architecture 0x{machine:x}; expected x86_64")
    optional = pe_offset + 24
    if struct.unpack_from("<H", data, optional)[0] != 0x20B:
        raise ValueError("expected PE32+ executable")
    sections = []
    for index in range(count):
        offset = optional + optional_size + index * 40
        virtual_size, virtual_address, raw_size, raw_offset = struct.unpack_from("<IIII", data, offset + 8)
        sections.append((virtual_address, max(virtual_size, raw_size), raw_offset))

    def file_offset(rva):
        for address, length, offset in sections:
            if address <= rva < address + length:
                return offset + rva - address
        raise ValueError(f"unmapped PE RVA 0x{rva:x}")

    import_rva, _ = struct.unpack_from("<II", data, optional + 112 + 8)
    dependencies = []
    if import_rva:
        offset = file_offset(import_rva)
        while any(data[offset:offset + 20]):
            name = struct.unpack_from("<I", data, offset + 12)[0]
            dependencies.append(cstring(data, file_offset(name)))
            offset += 20
    return "x86_64", dependencies


def macho(data):
    magic = data[:4]
    if magic == b"\xcf\xfa\xed\xfe":
        order = "<"
    elif magic == b"\xfe\xed\xfa\xcf":
        order = ">"
    else:
        raise ValueError("expected a single-architecture 64-bit Mach-O executable")
    cpu, _, _, count = struct.unpack_from(order + "IIII", data, 4)
    architecture = {0x1000007: "x86_64", 0x100000C: "aarch64"}.get(cpu)
    if not architecture:
        raise ValueError(f"unsupported Mach-O CPU 0x{cpu:x}")
    offset = 32
    dependencies = []
    for _ in range(count):
        command, size = struct.unpack_from(order + "II", data, offset)
        if size < 8:
            raise ValueError("invalid Mach-O command size")
        if (command & 0x7FFFFFFF) in {0xC, 0x18, 0x1F, 0x23}:
            string_offset = struct.unpack_from(order + "I", data, offset + 8)[0]
            dependencies.append(cstring(data, offset + string_offset))
        offset += size
    return architecture, dependencies


def macho_slices(data):
    magic = data[:4]
    if magic not in (b"\xca\xfe\xba\xbe", b"\xca\xfe\xba\xbf", b"\xbe\xba\xfe\xca", b"\xbf\xba\xfe\xca"):
        return [macho(data)]
    order = ">" if magic[0] == 0xCA else "<"
    wide = 0xBF in magic
    count, = struct.unpack_from(order + "I", data, 4)
    layout = order + ("IIQQII" if wide else "IIIII")
    if count == 0 or 8 + count * struct.calcsize(layout) > len(data):
        raise ValueError("invalid universal Mach-O header")
    slices = []
    for index in range(count):
        _, _, offset, size, *_ = struct.unpack_from(layout, data, 8 + index * struct.calcsize(layout))
        if size < 32 or offset + size > len(data):
            raise ValueError("invalid universal Mach-O slice")
        slices.append(macho(data[offset:offset + size]))
    return slices


def check(platform, directory, video=False):
    binaries = []
    missing = []
    expected_architecture = None
    extension = ".exe" if platform == "windows" else ""
    programs = ("animatix", "animatix-gui", "ffmpeg", "ffprobe") if video else ("animatix", "animatix-gui")
    dlls = {}
    if video and platform == "windows":
        dlls = {path.name.lower(): path for path in sorted(directory.glob("*.dll"))}
    for path in [*(directory / (name + extension) for name in programs), *dlls.values()]:
        data = path.read_bytes()
        try:
            slices = [pe(data)] if platform == "windows" else macho_slices(data)
        except (ValueError, struct.error) as error:
            raise ValueError(f"{path.name}: {error}") from error
        if expected_architecture is None:
            if len(slices) != 1:
                raise ValueError("expected a single architecture for the Animatix CLI")
            expected_architecture = slices[0][0]
        matches = [dependencies for architecture, dependencies in slices if architecture == expected_architecture]
        if len(matches) != 1:
            raise ValueError(f"package architectures differ at {path.name}")
        dependencies = matches[0]
        for dependency in dependencies:
            lowered = dependency.lower()
            if (not video or platform == "macos") and any(token in lowered for token in ("libavcodec", "libavformat", "libavutil", "libswscale", "ffmpeg", "/nix/store/")):
                raise ValueError(f"unexpected media library dependency: {dependency}")
            if platform == "macos" and not dependency.startswith(("/System/Library/", "/usr/lib/")):
                raise ValueError(f"unbundled macOS dependency: {dependency}")
            if platform == "windows":
                if lowered not in dlls and lowered not in WINDOWS_SYSTEM_DLLS and not lowered.startswith(("api-ms-win-", "ext-ms-win-")):
                    missing.append(f"unbundled Windows dependency in {path.name}: {dependency}")
        binaries.append({"file": path.name, "architecture": expected_architecture, "dependencies": dependencies})
    if missing:
        raise ValueError("\n".join(missing))
    return {"platform": platform, "architecture": expected_architecture, "video": video, "binaries": binaries}


if __name__ == "__main__":
    if len(sys.argv) not in {3, 4} or sys.argv[1] not in {"macos", "windows"} or sys.argv[3:] not in ([], ["--video"]):
        raise SystemExit("usage: native-package-check.py <macos|windows> BIN_DIR [--video]")
    print(json.dumps(check(sys.argv[1], Path(sys.argv[2]), len(sys.argv) == 4), indent=2))
