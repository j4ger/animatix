#!/usr/bin/env python3
"""Isolate a Windows offscreen render failure from the FFmpeg build."""

import os
from pathlib import Path
import shutil
import subprocess
from tempfile import TemporaryDirectory


def record(command, cwd, log):
    with log.open("wb") as stream:
        return subprocess.run(command, cwd=cwd, stdout=stream, stderr=subprocess.STDOUT, timeout=180).returncode


def debugger_path():
    debugger = shutil.which("cdb.exe")
    if debugger:
        return debugger
    sdk = Path(os.environ["ProgramFiles(x86)"]) / "Windows Kits/10/Debuggers/x64/cdb.exe"
    return str(sdk) if sdk.is_file() else None


def stage_sdk_dxc(package):
    sdk_bin = Path(os.environ["ProgramFiles(x86)"]) / "Windows Kits/10/bin"
    for compiler in sorted(sdk_bin.glob("*/x64/dxcompiler.dll"), reverse=True):
        validator = compiler.with_name("dxil.dll")
        if validator.is_file():
            for dll in (compiler, validator):
                shutil.copy2(dll, package / dll.name)
            print(f"Testing the DXC from Windows SDK {compiler.parents[1].name}.")
            return
    raise RuntimeError("A Windows SDK with both dxcompiler.dll and dxil.dll is required for this comparison.")


def main():
    root = Path.cwd()
    binary = root / "target/dist/animatix.exe"
    evidence = root / "target/native-windows-render"
    evidence.mkdir(parents=True, exist_ok=True)
    os.environ["RUST_LOG"] = "info,wgpu_core::instance=debug,wgpu_hal::dx12::instance=debug"
    stage_sdk_dxc(binary.parent)
    failed = False
    with TemporaryDirectory(prefix="animatix-native-render-") as directory:
        project = Path(directory)
        subprocess.run([str(binary), "init", "Narrated Project"], cwd=project, check=True)
        scenes = [
            ("hello", root / "examples/basics/00_hello.amx", "1"),
            ("narrated", project / "Narrated Project/main.amx", "4"),
        ]
        for name, scene, time in scenes:
            command = [str(binary), "--no-color", "image", str(scene), "--time", time,
                       "--output", str(evidence / f"{name}.png")]
            log = evidence / f"{name}.log"
            status = record(command, project, log)
            print(f"{name} exited with 0x{status & 0xFFFFFFFF:08X}")
            print(log.read_text(encoding="utf-8", errors="replace"))
            if status:
                failed = True
                debugger = debugger_path()
                if debugger:
                    stack = evidence / f"{name}-stack.txt"
                    try:
                        record([debugger, "-c", 'sxd -c2 ".ecxr; kv; q" av; g', *command], project, stack)
                    except subprocess.TimeoutExpired:
                        print(f"Debugger timed out for {name}.")
                    print(stack.read_text(encoding="utf-8", errors="replace"))
                else:
                    print("The Windows SDK debugger is unavailable.")

        if failed:
            errors = evidence / "windows-application-errors.txt"
            record(["powershell.exe", "-NoProfile", "-NonInteractive", "-Command",
                    "$filter = @{LogName='Application'; Id=1000; StartTime=(Get-Date).AddMinutes(-10)}; "
                    "Get-WinEvent -FilterHashtable $filter -ErrorAction SilentlyContinue | "
                    "Where-Object { $_.Message -match 'animatix[.]exe' } | "
                    "Select-Object -First 4 -ExpandProperty Message"],
                   project, errors)
            print(errors.read_text(encoding="utf-8", errors="replace"))
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
