#!/usr/bin/env python3
"""Stage the runner's matched Windows SDK DXC pair for unpublished proofs."""

import argparse
import os
from pathlib import Path
import shutil


def stage_sdk_dxc(package):
    sdk_bin = Path(os.environ["ProgramFiles(x86)"]) / "Windows Kits/10/bin"
    for compiler in sorted(sdk_bin.glob("*/x64/dxcompiler.dll"), reverse=True):
        validator = compiler.with_name("dxil.dll")
        if validator.is_file():
            for dll in (compiler, validator):
                shutil.copy2(dll, package / dll.name)
            print(f"windows_sdk_dxc={compiler.parents[1].name}")
            return
    raise RuntimeError("The Windows proof requires an SDK with both dxcompiler.dll and dxil.dll.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package", type=Path)
    stage_sdk_dxc(parser.parse_args().package)
