#!/usr/bin/env python3
"""Bounded rootless Nix experiment. Always exits 1 to prevent deployment."""
import hashlib
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time

VERSION = "2.34.9"
SHA256 = "e8912e087582b3799e0e5a6b06f9d63386c54821f77ad4de3b9b748736ad364e"
RESERVE = 3 * 1024**3
TIME_LIMIT = 900


def verify_archive(path):
    if hashlib.sha256(path.read_bytes()).hexdigest() != SHA256:
        raise RuntimeError("official tarball SHA256 mismatch")


def run(args, work, started):
    print("stage:", Path(args[0]).name, flush=True)
    process = subprocess.Popen(args, start_new_session=True)
    while process.poll() is None:
        time.sleep(2)
        if time.monotonic() - started > TIME_LIMIT or shutil.disk_usage(work).free < RESERVE:
            print("STOP time/space budget reached", flush=True)
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            raise RuntimeError("resource budget")
    if process.returncode:
        raise RuntimeError("stage failed, exit=" + str(process.returncode))


def main():
    repo = Path(__file__).resolve().parent.parent
    for tool in ["curl", "tar", "unshare", "mount", "chroot", "bash"]:
        if not shutil.which(tool):
            print("BLOCKED missing tool:", tool, flush=True)
            return 1
    if shutil.disk_usage(tempfile.gettempdir()).free < 6 * 1024**3:
        print("BLOCKED less than 6GiB free", flush=True)
        return 1
    work = Path(tempfile.mkdtemp(prefix="animatix-nix-probe-"))
    started = time.monotonic()
    lock_before = (repo / "flake.lock").read_bytes()
    try:
        archive = work / "nix.tar.xz"
        run(["curl", "--fail", "--silent", "--show-error", "--location", "--proto", "=https",
             "--proto-redir", "=https", "--tlsv1.2", "--max-time", "120", "--max-filesize", "100000000",
             f"https://releases.nixos.org/nix/nix-{VERSION}/nix-{VERSION}-x86_64-linux.tar.xz",
             "-o", str(archive)], work, started)
        verify_archive(archive)
        print("OFFICIAL_NIX_SHA256_OK", flush=True)
        run(["tar", "-xJf", str(archive), "-C", str(work)], work, started)
        unpacked = work / f"nix-{VERSION}-x86_64-linux"
        root = work / "root"
        (root / "nix").mkdir(parents=True)
        shutil.move(str(unpacked / "store"), str(root / "nix/store"))
        shutil.copyfile(unpacked / ".reginfo", root / "nix/bootstrap-reginfo")
        script = work / "stage.sh"
        shutil.copyfile(repo / "scripts/cloudflare-nix-rootless-stage.sh", script)
        run(["unshare", "--user", "--map-root-user", "--mount", "--fork", "bash",
             str(script), str(root), str(repo), "devshell",
             os.readlink("/proc/self/ns/user"), os.readlink("/proc/self/ns/mnt"),
             str(os.getuid())], work, started)
        if (repo / "flake.lock").read_bytes() != lock_before:
            raise RuntimeError("flake.lock changed unexpectedly")
        print("BOOTSTRAP_AND_DEVSHELL_PASS", flush=True)
    except Exception as error:
        print("BOOTSTRAP_BLOCKED:", str(error), flush=True)
    finally:
        print("elapsed_seconds=", round(time.monotonic() - started),
              "free_mib=", shutil.disk_usage(work).free // 1024**2, flush=True)
        print("Intentional experiment stop; no deployment", flush=True)
        # Retain only ephemeral build-host files; no persistent install or cache.
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
