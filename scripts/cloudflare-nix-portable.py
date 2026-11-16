#!/usr/bin/env python3
"""Pinned upstream nix-portable probe; intentionally never allows deployment."""
import hashlib
import importlib.util
import os
from pathlib import Path
import shutil
import tempfile
import time

VERSION = "v012"
SHA256 = "b409c55904c909ac3aeda3fb1253319f86a89ddd1ba31a5dec33d4a06414c72a"
URL = f"https://github.com/DavHau/nix-portable/releases/download/{VERSION}/nix-portable-x86_64"
spec = importlib.util.spec_from_file_location(
    "bounded", Path(__file__).with_name("cloudflare-nix-bootstrap.py"))
bounded = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bounded)


def verify(path):
    if hashlib.sha256(path.read_bytes()).hexdigest() != SHA256:
        raise RuntimeError("pinned release SHA256 mismatch")


def environment(work):
    # No inherited credentials, debugging hooks, alternate runtimes or Nix overrides.
    return {"PATH": os.defpath, "HOME": str(work / "home"), "TMPDIR": str(work),
            "LANG": "C.UTF-8", "NP_LOCATION": str(work), "NP_RUNTIME": "nix",
            "NP_CONF_SANDBOX": "true", "NP_GIT": shutil.which("git"),
            "NIX_CONFIG": "max-jobs = 2\ncores = 2\naccept-flake-config = false\nrequire-sigs = true\n"}


def main():
    if os.uname().machine != "x86_64":
        print("BLOCKED unsupported architecture")
        return 1
    for tool in ["curl", "git", "bash"]:
        if not shutil.which(tool):
            print("BLOCKED missing tool:", tool)
            return 1
    if shutil.disk_usage(tempfile.gettempdir()).free < 6 * 1024**3:
        print("BLOCKED less than 6GiB free")
        return 1
    repo = Path(__file__).resolve().parent.parent
    before = (repo / "flake.lock").read_bytes()
    work = Path(tempfile.mkdtemp(prefix="animatix-nix-portable-"))
    (work / "home").mkdir()
    started = time.monotonic()
    previous = dict(os.environ)
    try:
        env = environment(work)
        os.environ.clear()
        os.environ.update(env)
        binary = work / "nix-portable"
        bounded.run(["curl", "--fail", "--silent", "--show-error", "--location",
                     "--proto", "=https", "--proto-redir", "=https", "--tlsv1.2",
                     "--max-time", "120", "--max-filesize", "150000000",
                     URL, "-o", str(binary)], work, started)
        verify(binary)
        print("PINNED_UPSTREAM_RELEASE_SHA256_OK", flush=True)
        binary.chmod(0o700)
        bounded.run([str(binary), "nix", "--version"], work, started)
        print("PORTABLE_VERSION_PASS runtime=nix sandbox=true", flush=True)
        bounded.run([str(binary), "nix", "develop", "--no-write-lock-file",
                     str(repo) + "#web-build", "--command", "bash", "-eu", "-c",
                     "rustc --version; node --version; wasm-bindgen --version; "
                     "test -d \"$(rustc --print target-libdir --target wasm32-unknown-unknown)\"; "
                     "command -v wasm-opt >/dev/null; command -v zopfli >/dev/null"], work, started)
        if (repo / "flake.lock").read_bytes() != before:
            raise RuntimeError("flake.lock changed unexpectedly")
        print("PORTABLE_DEVSHELL_PASS", flush=True)
    except Exception as error:
        print("PORTABLE_BLOCKED:", str(error), flush=True)
    finally:
        os.environ.clear()
        os.environ.update(previous)
        print("elapsed_seconds=", round(time.monotonic() - started),
              "free_mib=", shutil.disk_usage(work).free // 1024**2, flush=True)
        print("Intentional experiment stop; no deployment", flush=True)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
