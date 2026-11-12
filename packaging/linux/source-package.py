#!/usr/bin/env python3
"""Create the pinned, offline corresponding-source archive for Linux packages."""

import argparse
import hashlib
import inspect
import json
import os
from pathlib import Path
import posixpath
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import tarfile


ROOT_FILES = {
    "Cargo.toml", "Cargo.lock", "LICENSE", "rust-toolchain.toml",
}
SOURCE_DIRS = {
    "crates", "vendor", "tools", "examples", "docs", "packaging/linux",
    "web/demos/lib",
}
SOURCE_SCRIPTS = {
    "scripts/dist-linux.sh", "scripts/dist-linux-container.sh",
    "scripts/dist-package.sh", "scripts/test-dist-linux.sh",
    "scripts/native-package-check.py", "scripts/native-package-tests.py",
}
SECRET_NAME = re.compile(r"(?:^\.env(?:$|\.)|^(?:credentials|secrets)(?:\.(?:json|toml|txt|yaml|yml|env))?$|^id_(?:rsa|ed25519)(?:$|[._-])|\.(?:pem|key|p12|pfx)$|^\.netrc$|^\.npmrc$)", re.I)
MEDIA_KEYS = ("FFMPEG_ARCHIVE", "X264_ARCHIVE", "LIBVPX_ARCHIVE", "OPUS_ARCHIVE")


def allowed(path):
    path = path.as_posix()
    if path in ROOT_FILES or path in SOURCE_SCRIPTS:
        return True
    if path in {".cargo/config.toml", ".cargo/project-config.toml"}:
        return True
    return any(path == directory or path.startswith(directory + "/") for directory in SOURCE_DIRS)


def secret_path(path):
    return any(SECRET_NAME.search(part) for part in path.parts)


def paths_under(root):
    root = Path(root).resolve()
    untracked = set()
    try:
        git_dir = subprocess.run(["git", "rev-parse", "--show-toplevel"], cwd=root, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
    except OSError:
        git_dir = None
    if git_dir and git_dir.returncode == 0 and Path(git_dir.stdout.strip()).resolve() == root:
        tracked_raw = subprocess.check_output(["git", "ls-files", "--cached", "-z"], cwd=root)
        untracked_raw = subprocess.check_output(["git", "ls-files", "--others", "--exclude-standard", "-z"], cwd=root)
        tracked = {Path(os.fsdecode(path)) for path in tracked_raw.split(b"\0") if path}
        untracked = {Path(os.fsdecode(path)) for path in untracked_raw.split(b"\0") if path}
        paths = tracked | untracked
    else:
        paths = set()
        for relative in (*ROOT_FILES, ".cargo/config.toml", ".cargo/project-config.toml", *SOURCE_DIRS, *SOURCE_SCRIPTS):
            candidate = root / relative
            if candidate.is_file() or candidate.is_symlink():
                paths.add(Path(relative))
            elif candidate.is_dir():
                for directory, dirs, names in os.walk(candidate, followlinks=False):
                    kept_dirs = []
                    for name in sorted(dirs):
                        child = Path(directory) / name
                        relative_parts = child.relative_to(root).parts
                        if name in {".git", "node_modules", "__pycache__"} or relative_parts == ("target",):
                            continue
                        if child.is_symlink():
                            paths.add(child.relative_to(root))
                        else:
                            kept_dirs.append(name)
                    dirs[:] = kept_dirs
                    for name in names:
                        paths.add((Path(directory) / name).relative_to(root))
    selected = []
    has_generated_vendor = (root / "cargo-vendor").is_dir()
    for path in sorted(paths, key=lambda item: item.as_posix()):
        if path.suffix == ".pyc" or path.parts[0] == "target" or any(part in {".git", "node_modules", "__pycache__"} for part in path.parts):
            continue
        if path.as_posix() == ".cargo/config.toml" and has_generated_vendor:
            continue  # The generated vendor config is not part of source identity.
        if not allowed(path) or (path in untracked and secret_path(path)):
            continue
        absolute = root / path
        if absolute.is_file() or absolute.is_symlink():
            selected.append(path)
    if not {"Cargo.toml", "Cargo.lock", "LICENSE"}.issubset({path.as_posix() for path in selected}):
        raise ValueError("source snapshot is missing Cargo.toml, Cargo.lock or LICENSE")
    return selected


def file_record(path, relative):
    mode = stat.S_IMODE(path.lstat().st_mode)
    if path.is_symlink():
        payload = os.fsencode(os.readlink(path))
        kind = "symlink"
    else:
        payload = path.read_bytes()
        kind = "file"
    return {"path": relative.as_posix(), "mode": mode, "kind": kind,
            "size_bytes": len(payload), "sha256": hashlib.sha256(payload).hexdigest()}, payload


def tree_digest(directory):
    directory = Path(directory)
    digest = hashlib.sha256()
    count = 0
    for path in sorted(directory.rglob("*")):
        if not path.is_file() and not path.is_symlink():
            continue
        relative = path.relative_to(directory)
        record, payload = file_record(path, relative)
        encoded = relative.as_posix().encode()
        digest.update(len(encoded).to_bytes(8, "big")); digest.update(encoded)
        digest.update(record["mode"].to_bytes(4, "big")); digest.update(len(payload).to_bytes(8, "big")); digest.update(payload)
        count += 1
    return digest.hexdigest(), count


def source_manifest(root):
    root = Path(root).resolve()
    records = []
    seen_paths = set()
    digest = hashlib.sha256()
    for physical in paths_under(root):
        canonical = Path(".cargo/config.toml") if physical.as_posix() == ".cargo/project-config.toml" else physical
        if canonical in seen_paths:
            raise ValueError(f"multiple source files map to the same snapshot path: {canonical.as_posix()}")
        seen_paths.add(canonical)
        source = root / physical
        record, payload = file_record(source, canonical)
        records.append(record)
        encoded = canonical.as_posix().encode()
        digest.update(len(encoded).to_bytes(8, "big")); digest.update(encoded)
        digest.update(record["mode"].to_bytes(4, "big")); digest.update(len(payload).to_bytes(8, "big")); digest.update(payload)
    return digest.hexdigest(), records


def git_identity(root):
    root = Path(root).resolve()
    commit = os.environ.get("ANIMATIX_SOURCE_COMMIT")
    dirty = os.environ.get("ANIMATIX_SOURCE_DIRTY")
    if commit and dirty is not None:
        return commit, dirty == "true"
    try:
        commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True, stderr=subprocess.DEVNULL).strip()
        dirty = bool(subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=all"], cwd=root, stderr=subprocess.DEVNULL))
    except (OSError, subprocess.CalledProcessError):
        identity = json.loads((root / "SOURCE-IDENTITY.json").read_text())
        return identity["source_commit"], identity["source_dirty"]
    return commit, dirty


def snapshot(root, destination):
    root, destination = Path(root).resolve(), Path(destination).resolve()
    digest, records = source_manifest(root)
    commit, dirty = git_identity(root)
    expected_digest = os.environ.get("ANIMATIX_SOURCE_TREE_SHA256")
    if expected_digest and digest != expected_digest:
        raise ValueError("source tree does not match ANIMATIX_SOURCE_TREE_SHA256")
    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True)
    for record in records:
        relative = Path(record["path"])
        source_relative = Path(".cargo/project-config.toml") if relative.as_posix() == ".cargo/config.toml" and (root / ".cargo/project-config.toml").is_file() else relative
        source = root / source_relative
        target_relative = Path(".cargo/project-config.toml") if relative.as_posix() == ".cargo/config.toml" else relative
        target = destination / target_relative
        target.parent.mkdir(parents=True, exist_ok=True)
        if source.is_symlink():
            target.symlink_to(os.readlink(source))
        else:
            shutil.copyfile(source, target)
            target.chmod(record["mode"])
    # A released source tree already has a complete offline Cargo vendor set.
    # It is generated input, so it is copied for rebuilding but excluded from
    # the source snapshot identity above.
    if (root / "cargo-vendor").is_dir():
        shutil.copytree(root / "cargo-vendor", destination / "cargo-vendor", symlinks=True)
    if (root / "third-party-sources").is_dir():
        shutil.copytree(root / "third-party-sources", destination / "third-party-sources", symlinks=True)
    if (root / ".cargo/config.toml").is_file() and (root / "cargo-vendor").is_dir():
        cargo_target = destination / ".cargo/config.toml"
        cargo_target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(root / ".cargo/config.toml", cargo_target)
    inherited = {}
    try:
        inherited = json.loads((root / "SOURCE-IDENTITY.json").read_text())
    except (OSError, json.JSONDecodeError):
        pass
    identity = {**inherited, "schema_version": 1, "source_commit": commit, "source_dirty": dirty,
                "source_tree_sha256": digest, "files": records}
    (destination / "SOURCE-IDENTITY.json").write_text(json.dumps(identity, indent=2) + "\n")
    return identity


def offline_vendor(directory):
    directory = Path(directory)
    cargo_dir = directory / ".cargo"
    cargo_dir.mkdir(exist_ok=True)
    if (directory / "cargo-vendor").is_dir() and (cargo_dir / "config.toml").is_file():
        identity_path = directory / "SOURCE-IDENTITY.json"
        identity = json.loads(identity_path.read_text())
        verify_offline_inputs(directory, identity)
        subprocess.run(["cargo", "metadata", "--locked", "--offline", "--format-version", "1"],
                       cwd=directory, check=True, stdout=subprocess.DEVNULL)
        return
    project_config = cargo_dir / "project-config.toml"
    if project_config.exists():
        text = project_config.read_text()
        if re.search(r"(?im)^\s*\[source(?:\.|\])", text):
            raise ValueError("source replacement in .cargo/config.toml cannot be merged safely into the offline vendor config")
        if re.search(r"(?im)\b(?:token|password|credential)\s*=|(?:/home/|/workspace/|/nix/store/|/tmp/|/root/|/Users/|\$HOME)", text):
            raise ValueError(".cargo/config.toml contains a credential or machine-specific absolute path")
        shutil.copyfile(project_config, cargo_dir / "config.toml")
    stale_vendor = directory / "cargo-vendor"
    if stale_vendor.is_symlink():
        stale_vendor.unlink()
    elif stale_vendor.exists():
        shutil.rmtree(stale_vendor)
    try:
        result = subprocess.run(["cargo", "vendor", "--locked", "cargo-vendor"], cwd=directory,
                                check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    except Exception:
        (cargo_dir / "config.toml").unlink(missing_ok=True)
        raise
    vendor_config = result.stdout.strip()
    if not vendor_config:
        raise ValueError("cargo vendor did not emit a source configuration")
    if project_config.exists():
        vendor_config += "\n\n" + project_config.read_text().strip()
    (cargo_dir / "config.toml").write_text(vendor_config + "\n")
    identity_path = directory / "SOURCE-IDENTITY.json"
    identity = json.loads(identity_path.read_text())
    vendor_hash, vendor_files = tree_digest(directory / "cargo-vendor")
    identity["cargo_vendor_sha256"] = vendor_hash
    identity["cargo_vendor_files"] = vendor_files
    identity["cargo_config_sha256"] = hashlib.sha256((cargo_dir / "config.toml").read_bytes()).hexdigest()
    identity_path.write_text(json.dumps(identity, indent=2) + "\n")


def verify_offline_inputs(directory, identity):
    directory = Path(directory)
    vendor = directory / "cargo-vendor"
    config = directory / ".cargo/config.toml"
    if not vendor.is_dir() or not config.is_file():
        raise ValueError("offline Cargo vendor set or config is missing")
    vendor_hash, vendor_files = tree_digest(vendor)
    if vendor_hash != identity.get("cargo_vendor_sha256") or vendor_files != identity.get("cargo_vendor_files"):
        raise ValueError("Cargo vendor set does not match SOURCE-IDENTITY.json")
    config_hash = hashlib.sha256(config.read_bytes()).hexdigest()
    if config_hash != identity.get("cargo_config_sha256"):
        raise ValueError("Cargo vendor config does not match SOURCE-IDENTITY.json")


def package(snapshot_dir, media_dir, output, version):
    snapshot_dir, media_dir, output = Path(snapshot_dir).resolve(), Path(media_dir).resolve(), Path(output).resolve()
    source_identity = json.loads((snapshot_dir / "SOURCE-IDENTITY.json").read_text())
    verify_offline_inputs(snapshot_dir, source_identity)
    source_digest, _ = source_manifest(snapshot_dir)
    if source_digest != source_identity["source_tree_sha256"]:
        raise ValueError("source snapshot changed after the native build")
    archive_root = f"animatix-v{version}-x86_64-linux-gnu-video-source"
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="animatix-source-package-") as temporary:
        stage = Path(temporary) / archive_root
        shutil.copytree(snapshot_dir, stage, symlinks=True)
        sources = stage / "third-party-sources"
        sources.mkdir(exist_ok=True)
        lockfile = stage / "packaging/linux/sources.lock"
        lock_text = lockfile.read_text()
        checksum_keys = {"FFMPEG_ARCHIVE": "FFMPEG_SHA256", "X264_ARCHIVE": "X264_SHA256",
                         "LIBVPX_ARCHIVE": "LIBVPX_SHA256", "OPUS_ARCHIVE": "OPUS_SHA256"}
        checksums = dict(re.findall(r"^([A-Z0-9_]+)='([^']+)'\s*$", lock_text, re.M))
        media_records = []
        for key in MEDIA_KEYS:
            name = checksums[key]
            source = media_dir / name
            payload = source.read_bytes()
            digest = hashlib.sha256(payload).hexdigest()
            if digest != checksums[checksum_keys[key]]:
                raise ValueError(f"third-party source archive hash mismatch: {name}")
            existing = sources / name
            if existing.exists() and existing.read_bytes() != payload:
                raise ValueError(f"inherited third-party source differs from the locked archive: {name}")
            inherited_media = {item.get("path"): item for item in source_identity.get("media_sources", [])}
            previous = inherited_media.get(f"third-party-sources/{name}")
            if previous and (previous.get("sha256") != digest or previous.get("size_bytes") != len(payload)):
                raise ValueError(f"source identity has a different third-party source: {name}")
            shutil.copyfile(source, sources / name)
            media_records.append({"path": f"third-party-sources/{name}", "sha256": digest, "size_bytes": len(payload)})
        identity = dict(source_identity)
        identity["media_sources"] = media_records
        (stage / "SOURCE-IDENTITY.json").write_text(json.dumps(identity, indent=2) + "\n")
        (stage / "REBUILD.md").write_text(
            "# Rebuilding the Linux package from corresponding source\n\n"
            "This archive contains the exact dirty project source snapshot, Cargo.lock, vendored Rust dependencies, "
            "the pinned FFmpeg/codec source archives, and the Linux build recipe. It excludes build outputs and Git metadata.\n\n"
            "Install Docker, then run from this directory:\n\n"
            "```sh\n"
            "sha256sum --check SHA256SUMS\n"
            "export ANIMATIX_SOURCE_COMMIT=$(python3 -c 'import json; print(json.load(open(\"SOURCE-IDENTITY.json\"))[\"source_commit\"])')\n"
            "export ANIMATIX_SOURCE_DIRTY=$(python3 -c 'import json; print(str(json.load(open(\"SOURCE-IDENTITY.json\"))[\"source_dirty\"]).lower())')\n"
            "export ANIMATIX_SOURCE_TREE_SHA256=$(python3 -c 'import json; print(json.load(open(\"SOURCE-IDENTITY.json\"))[\"source_tree_sha256\"])')\n"
            "export ANIMATIX_DIST_OFFLINE=true\n"
            "scripts/dist-linux.sh target/dist-linux-package\n"
            "```\n\n"
            "The source identity is checked before and after compilation. The build uses Cargo's offline vendor configuration. "
            "The Ubuntu apt tool layer is sourced from signed repositories and is recorded in the binary package metadata; "
            "it is not claimed as a byte-for-byte reproducible toolchain.\n"
        )
        sums = []
        for path in sorted(stage.rglob("*")):
            if not path.is_file() or path.name == "SHA256SUMS":
                continue
            relative = path.relative_to(stage).as_posix()
            sums.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {relative}")
        (stage / "SHA256SUMS").write_text("\n".join(sums) + "\n")
        output_tmp = output.with_suffix(output.suffix + ".tmp")
        if output_tmp.exists():
            output_tmp.unlink()
        tar = subprocess.Popen(["tar", "--sort=name", "--mtime=UTC 1970-01-01", "--owner=0", "--group=0", "--numeric-owner", "-cf", "-", "-C", str(stage.parent), stage.name], stdout=subprocess.PIPE)
        try:
            with output_tmp.open("wb") as raw:
                compressor = subprocess.Popen(["gzip", "-n", "-9"], stdin=tar.stdout, stdout=raw)
                tar.stdout.close()
                compressor.wait()
                if compressor.returncode != 0:
                    raise subprocess.CalledProcessError(compressor.returncode, compressor.args)
            if tar.wait() != 0:
                raise subprocess.CalledProcessError(tar.returncode or 1, tar.args)
        except Exception:
            tar.kill()
            output_tmp.unlink(missing_ok=True)
            raise
        output_tmp.replace(output)
    verify_archive(output)
    return output


def verify_archive(archive):
    with tempfile.TemporaryDirectory(prefix="animatix-source-verify-") as temporary:
        destination = Path(temporary)
        with tarfile.open(archive, "r:gz") as source:
            members = source.getmembers()
            roots = {Path(member.name).parts[0] for member in members if Path(member.name).parts}
            if len(roots) != 1:
                raise ValueError("source archive must have exactly one top-level directory")
            for member in members:
                path = Path(member.name)
                if path.is_absolute() or ".." in path.parts or path.parts[1:2] in {(".git",), ("target",), ("node_modules",)}:
                    raise ValueError(f"source archive contains an unsafe path: {member.name}")
                if not (member.isfile() or member.isdir() or member.issym()):
                    raise ValueError(f"source archive contains an unsupported entry: {member.name}")
                if member.issym():
                    target = member.linkname
                    resolved = posixpath.normpath(posixpath.join(posixpath.dirname(member.name), target))
                    if target.startswith("/") or Path(resolved).parts[:1] != (next(iter(roots)),):
                        raise ValueError(f"source archive contains an unsafe symlink: {member.name}")
            if "filter" in inspect.signature(source.extractall).parameters:
                def source_mode_filter(member, _destination):
                    # The data filter makes read-only project inputs writable,
                    # which changes SOURCE-IDENTITY.json's mode digest. Paths
                    # and symlink targets were validated above, so preserve
                    # ordinary permission bits while clearing setuid/sticky.
                    if member.mode is not None:
                        member.mode &= 0o777
                    return member

                source.extractall(destination, filter=source_mode_filter)
            else:
                source.extractall(destination)
        root = destination / next(iter(roots))
        identity = json.loads((root / "SOURCE-IDENTITY.json").read_text())
        digest, _ = source_manifest(root)
        if digest != identity.get("source_tree_sha256"):
            raise ValueError("source archive project files do not match SOURCE-IDENTITY.json")
        vendor_digest, vendor_files = tree_digest(root / "cargo-vendor")
        if vendor_digest != identity.get("cargo_vendor_sha256") or vendor_files != identity.get("cargo_vendor_files"):
            raise ValueError("source archive Cargo vendor set does not match its identity")
        config_digest = hashlib.sha256((root / ".cargo/config.toml").read_bytes()).hexdigest()
        if config_digest != identity.get("cargo_config_sha256"):
            raise ValueError("source archive Cargo vendor config does not match its identity")
        lock_values = dict(re.findall(r"^([A-Z0-9_]+)='([^']+)'\s*$", (root / "packaging/linux/sources.lock").read_text(), re.M))
        checksum_keys = {"FFMPEG_ARCHIVE": "FFMPEG_SHA256", "X264_ARCHIVE": "X264_SHA256",
                         "LIBVPX_ARCHIVE": "LIBVPX_SHA256", "OPUS_ARCHIVE": "OPUS_SHA256"}
        media = identity.get("media_sources", [])
        expected_media = {f"third-party-sources/{lock_values[key]}" for key in MEDIA_KEYS}
        if {item.get("path") for item in media} != expected_media:
            raise ValueError("source archive media source list does not match sources.lock")
        for item in media:
            path = root / item["path"]
            payload = path.read_bytes()
            archive_name = path.name
            key = next(key for key in MEDIA_KEYS if lock_values[key] == archive_name)
            digest = hashlib.sha256(payload).hexdigest()
            if digest != lock_values[checksum_keys[key]] or digest != item.get("sha256") or len(payload) != item.get("size_bytes"):
                raise ValueError(f"source archive media source mismatch: {archive_name}")
        sums_file = root / "SHA256SUMS"
        expected = {}
        for line in sums_file.read_text().splitlines():
            sha, name = line.split("  ", 1)
            if name in expected:
                raise ValueError(f"duplicate source checksum entry: {name}")
            expected[name] = sha
        actual = {path.relative_to(root).as_posix() for path in root.rglob("*") if path.is_file() and path.name != "SHA256SUMS"}
        if actual != set(expected):
            raise ValueError("source archive SHA256SUMS is incomplete or has unlisted files")
        for name, sha in expected.items():
            if hashlib.sha256((root / name).read_bytes()).hexdigest() != sha:
                raise ValueError(f"source archive checksum mismatch: {name}")
    return identity


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    create = sub.add_parser("snapshot")
    create.add_argument("--root", required=True)
    create.add_argument("--destination", required=True)
    check = sub.add_parser("digest")
    check.add_argument("root")
    archive = sub.add_parser("package")
    archive.add_argument("--snapshot", required=True)
    archive.add_argument("--media-dir", required=True)
    archive.add_argument("--output", required=True)
    archive.add_argument("--version", required=True)
    vendor = sub.add_parser("vendor")
    vendor.add_argument("directory")
    verify = sub.add_parser("verify")
    verify.add_argument("archive")
    args = parser.parse_args()
    try:
        if args.command == "snapshot":
            print(json.dumps(snapshot(args.root, args.destination)))
        elif args.command == "digest":
            print(source_manifest(args.root)[0])
        elif args.command == "package":
            print(package(args.snapshot, args.media_dir, args.output, args.version))
        elif args.command == "vendor":
            offline_vendor(args.directory)
        else:
            print(json.dumps(verify_archive(args.archive), indent=2))
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
