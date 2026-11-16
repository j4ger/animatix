#!/usr/bin/env python3
"""Read the shipped binary's architecture/dependencies without executing tools."""
import json
from pathlib import Path
import struct
import sys


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


def check(platform, directory):
    binaries = []
    architectures = set()
    for name in ("animatix", "animatix-gui"):
        path = directory / (name + (".exe" if platform == "windows" else ""))
        data = path.read_bytes()
        architecture, dependencies = pe(data) if platform == "windows" else macho(data)
        architectures.add(architecture)
        for dependency in dependencies:
            lowered = dependency.lower()
            if any(token in lowered for token in ("libavcodec", "libavformat", "libavutil", "libswscale", "ffmpeg", "/nix/store/")):
                raise ValueError(f"no-video package has unexpected dependency: {dependency}")
            if platform == "macos" and not dependency.startswith(("/System/Library/", "/usr/lib/")):
                # No private dylibs are shipped by the current no-video package.
                raise ValueError(f"unbundled macOS dependency: {dependency}")
            if platform == "windows":
                system = {
                    "advapi32.dll", "avrt.dll", "bcrypt.dll", "cfgmgr32.dll", "comctl32.dll", "comdlg32.dll",
                    "crypt32.dll", "d3d11.dll", "d3d12.dll", "dcomp.dll", "dinput8.dll", "dwmapi.dll",
                    "d2d1.dll", "dwrite.dll", "dxgi.dll", "gdi32.dll", "hid.dll", "imm32.dll", "iphlpapi.dll",
                    "kernel32.dll", "mmdevapi.dll", "msimg32.dll", "msvcrt.dll", "netapi32.dll", "normaliz.dll", "ntdll.dll",
                    "ole32.dll", "oleaut32.dll", "opengl32.dll", "powrprof.dll", "propsys.dll", "psapi.dll",
                    "rpcrt4.dll", "secur32.dll", "setupapi.dll", "shell32.dll", "shlwapi.dll", "user32.dll",
                    "userenv.dll", "usp10.dll", "uxtheme.dll", "version.dll", "winmm.dll", "winspool.drv",
                    "win32u.dll", "wintrust.dll", "ws2_32.dll", "wtsapi32.dll", "xinput1_4.dll",
                }
                if lowered not in system and not lowered.startswith(("api-ms-win-", "ext-ms-win-")):
                    # The current ZIP ships only executables and licenses.
                    raise ValueError(f"unbundled Windows dependency: {dependency}")
        binaries.append({"file": path.name, "architecture": architecture, "dependencies": dependencies})
    if len(architectures) != 1:
        raise ValueError("CLI/GUI architectures differ")
    return {"platform": platform, "architecture": architectures.pop(), "video": False, "binaries": binaries}


if __name__ == "__main__":
    if len(sys.argv) != 3 or sys.argv[1] not in {"macos", "windows"}:
        raise SystemExit("usage: native-package-check.py <macos|windows> BIN_DIR")
    print(json.dumps(check(sys.argv[1], Path(sys.argv[2])), indent=2))
