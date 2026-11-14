"""Experimental CI layouts and verified native artifacts; no gate definitions."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


def transform(jobs, topology):
    if topology == "baseline":
        return jobs, ""
    by_id = {job["id"]: job for job in jobs}
    native_ids = ("check-lint", "test-examples", "render-scenes")
    if any(identifier not in by_id for identifier in native_ids):
        raise ValueError("topology experiments require the full native matrix")
    native = [by_id[identifier] for identifier in native_ids]
    if any(job["shell"] != "nix" or job["os"] != "ubuntu-latest" for job in native):
        raise ValueError("cannot merge jobs with incompatible environments")
    if topology == "shared-cli":
        return [job for job in jobs if job["id"] not in {"test-examples", "render-scenes"}], native[-1]["gate"]
    if topology != "grouped":
        raise ValueError(f"unknown CI topology: {topology}")
    gates = list(dict.fromkeys(gate for job in native for gate in job["gate"].split()))
    merged = {**native[0], "id": "native-grouped", "name": "Native Grouped (ubuntu-latest)",
              "gate": " ".join(gates)}
    return [job for job in jobs if job["id"] not in native_ids] + [merged], ""


def identity():
    def output(*command):
        return subprocess.run(command, check=True, text=True, capture_output=True).stdout.strip()
    def digest(path):
        return hashlib.sha256(Path(path).read_bytes()).hexdigest()
    return {"commit": output("git", "rev-parse", "HEAD"),
            "worktree_diff": hashlib.sha256(output("git", "diff", "HEAD", "--").encode()).hexdigest(),
            "rustc": output("rustc", "-vV"), "cargo_lock": digest("Cargo.lock"),
            "flake_lock": digest("flake.lock"), "profile": "dev",
            "features": "workspace-defaults/bin:animatix", "rustflags": os.environ.get("RUSTFLAGS", ""),
            "encoded_rustflags": os.environ.get("CARGO_ENCODED_RUSTFLAGS", ""),
            "build_target": os.environ.get("CARGO_BUILD_TARGET", ""),
            "cargo_environment": {key: value for key, value in os.environ.items()
                                  if key.startswith(("CARGO_PROFILE_", "CARGO_TARGET_"))
                                  and key != "CARGO_TARGET_DIR"}}


def artifact(mode, source, destination):
    source, destination = Path(source), Path(destination)
    if mode == "stage-cli":
        destination.mkdir(parents=True, exist_ok=True)
        binary = destination / "animatix"
        shutil.copy2(source, binary)
        provenance = identity()
        provenance["sha256"] = hashlib.sha256(binary.read_bytes()).hexdigest()
        (destination / "identity.json").write_text(json.dumps(provenance, indent=2) + "\n")
        return
    binary = source / "animatix"
    provenance = json.loads((source / "identity.json").read_text())
    if any(provenance.get(key) != value for key, value in identity().items()):
        raise ValueError("CLI artifact source, toolchain, flags or profile mismatch")
    if provenance.get("sha256") != hashlib.sha256(binary.read_bytes()).hexdigest():
        raise ValueError("CLI artifact digest mismatch")
    binary.chmod(binary.stat().st_mode | 0o111)
    print(f"verified CLI artifact: {binary.resolve()}")
