#!/usr/bin/env python3
"""Explicit release preparation and verified publication; never commit or tag."""
import argparse
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import tempfile
import tomllib

NUMBER = r"(?:0|[1-9][0-9]*)"
IDENT = r"(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)"
SEMVER = rf"{NUMBER}\.{NUMBER}\.{NUMBER}(?:-{IDENT}(?:\.{IDENT})*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
TAG_PATTERN = rf"^v{SEMVER}$"


def version(value):
    if not re.fullmatch(SEMVER, value, flags=re.ASCII):
        raise ValueError("expected strict MAJOR.MINOR.PATCH[-prerelease][+build], without v")
    return value


def tag_name(value):
    if not re.fullmatch(TAG_PATTERN, value, flags=re.ASCII):
        raise ValueError("expected an explicit vMAJOR.MINOR.PATCH[-prerelease][+build] tag")
    return value


def git(root, *args):
    return subprocess.check_output(["git", *args], cwd=root, stderr=subprocess.PIPE).decode().strip()


def committed(root, commit, path):
    relative = PurePosixPath(path)
    if relative.is_absolute() or ".." in relative.parts or not relative.parts:
        raise ValueError("notes path must be a repository-relative file")
    # Never read notes from the worktree: only the reviewed release commit.
    return subprocess.check_output(["git", "show", f"{commit}:{relative.as_posix()}"], cwd=root,
                                   stderr=subprocess.PIPE)


def resolve(root, tag, expected_commit=None, remote=None, notes_path=None, expected_tag_object=None):
    tag_name(tag)
    ref = f"refs/tags/{tag}"
    oid = git(root, "rev-parse", "--verify", ref)
    commit = git(root, "rev-parse", "--verify", ref + "^{commit}")
    if expected_commit is not None:
        if not re.fullmatch(r"[0-9a-f]{40}", expected_commit) or commit != expected_commit:
            raise ValueError("release tag no longer points to the expected immutable commit")
    if expected_tag_object is not None:
        if not re.fullmatch(r"[0-9a-f]{40}", expected_tag_object) or oid != expected_tag_object:
            raise ValueError("release tag object changed since resolution")
    if remote:
        if remote.startswith("-"):
            raise ValueError("invalid remote")
        rows = git(root, "ls-remote", remote, ref, ref + "^{}").splitlines()
        advertised = dict(row.split("\t", 1)[::-1] for row in rows)
        if advertised.get(ref) != oid or advertised.get(ref + "^{}", advertised.get(ref)) != commit:
            raise ValueError("remote release tag is missing or differs from the resolved local tag")
    manifest = tomllib.loads(committed(root, commit, "Cargo.toml").decode())
    if manifest["workspace"]["package"]["version"] != tag[1:]:
        raise ValueError("release tag does not match committed workspace version")
    path = notes_path or f"docs/releases/{tag}.md"
    notes = committed(root, commit, path)
    text = notes.decode("utf-8")
    substantive = [line for line in text.splitlines() if line.strip() and not line.lstrip().startswith("#")]
    placeholder = re.compile(r"(?im)^\s*(?:<!--\s*)?(?:[>#*+\-]\s*|\d+[.)]\s*|\[ ?\]\s*)*(?:TODO|TBD|PLACEHOLDER|DRAFT)\b")
    if not substantive or placeholder.search(text):
        raise ValueError("committed release notes are empty or contain a placeholder")
    return {"tag": tag, "version": tag, "package_version": tag[1:], "commit": commit,
            "tag_object": oid, "notes_path": path}


def draft(root, new_version, output, base_tag=None):
    version(new_version)
    tag = "v" + new_version
    # Do not replace an already-finalized release's notes by generating a draft.
    tags = git(root, "tag", "--list").splitlines()
    if tag in tags:
        raise ValueError("this release tag already exists")
    candidates = [name for name in git(root, "tag", "--merged", "HEAD").splitlines()
                  if re.fullmatch(TAG_PATTERN, name, flags=re.ASCII)]
    if base_tag:
        tag_name(base_tag)
        if base_tag not in candidates:
            raise ValueError("base tag must be a reachable semantic-version release")
    elif candidates:
        # Nearest ancestor, independent of nightly/Cloudflare tag timestamps.
        base_tag = min(candidates, key=lambda name: (int(git(root, "rev-list", "--count", f"{name}..HEAD")), name))
    destination = Path(output)
    if not destination.is_absolute():
        destination = Path(root) / destination
    if destination.exists():
        raise ValueError("draft output already exists; choose a new file or edit it manually")
    destination.parent.mkdir(parents=True, exist_ok=True)
    command = ["git-cliff", "--config", str(Path(root) / "cliff.toml"), "--tag-pattern", TAG_PATTERN,
               "--tag", tag, "--output", str(destination)]
    if base_tag:
        command.append(f"{base_tag}..HEAD")
    subprocess.run(command, cwd=root, check=True)
    return {"tag": tag, "base_tag": base_tag, "output": str(destination)}


def github_repository(url):
    match = re.fullmatch(r"(?:https://github\.com/|git@github\.com:|ssh://git@github\.com/)([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+?)(?:\.git)?/?", url)
    if not match:
        raise ValueError("origin must be an explicit github.com HTTPS or SSH repository")
    return match.group(1)


def release_assets(directory, tag):
    directory = Path(directory)
    if not directory.is_dir():
        raise ValueError("release assets directory does not exist")
    assets = sorted(directory.iterdir())
    if any(path.is_symlink() or not path.is_file() or path.stat().st_size == 0 for path in assets):
        raise ValueError("release assets must be nonempty regular files")
    prefix = f"animatix-{tag}-"
    fixed = {prefix + suffix for suffix in (
        "x86_64-linux-gnu-video.tar.gz", "x86_64-linux-gnu-video-source.tar.gz",
        "x86_64-windows.zip", "site.zip", "web-sdk.zip",
    )}
    macos = {prefix + architecture + "-macos.zip" for architecture in ("aarch64", "x86_64")}
    names = {path.name for path in assets}
    selected_macos = names & macos
    if len(selected_macos) != 1 or names != fixed | selected_macos:
        raise ValueError("release assets must contain exactly Linux binary+source, one macOS archive, Windows, site, and SDK")
    return assets


def publish(root, tag, expected_commit, expected_tag_object, assets_dir, remote="origin", notes_path=None,
            repository=None):
    if not expected_tag_object:
        raise ValueError("publishing requires the resolved tag object")
    if remote != "origin":
        raise ValueError("publication verifies origin, the repository used by gh")
    if not repository or github_repository(git(root, "remote", "get-url", "origin")) != repository:
        raise ValueError("publication repository does not match origin")
    result = resolve(root, tag, expected_commit, remote, notes_path, expected_tag_object)
    directory = Path(assets_dir)
    if not directory.is_absolute():
        directory = Path(root) / directory
    directory = directory.resolve()
    assets = release_assets(directory, tag)
    # The body is immutable source, not a previously generated worktree file.
    # `gh release create` fails if the release already exists. Never follow it
    # with edit/upload/delete or retry against a potentially published release.
    with tempfile.TemporaryDirectory(prefix="animatix-release-notes-") as temporary:
        notes = Path(temporary) / "notes.md"
        notes.write_bytes(committed(root, result["commit"], result["notes_path"]))
        command = ["gh", "release", "create", tag, "--repo", repository, "--verify-tag", "--target", result["commit"],
                   "--title", tag, "--notes-file", str(notes)]
        if "-" in result["package_version"].split("+", 1)[0]:
            command.append("--prerelease")
        command.extend(str(path) for path in assets)
        subprocess.run(command, cwd=root, check=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    sub = parser.add_subparsers(dest="command", required=True)
    validate = sub.add_parser("validate-version")
    validate.add_argument("version")
    drafting = sub.add_parser("draft")
    drafting.add_argument("version")
    drafting.add_argument("--output", required=True)
    drafting.add_argument("--base-tag")
    for name in ("resolve", "verify", "notes", "publish"):
        item = sub.add_parser(name)
        item.add_argument("tag")
        item.add_argument("--expected-commit", required=name != "resolve")
        item.add_argument("--remote")
        item.add_argument("--expected-tag-object", required=name == "publish")
        if name == "resolve":
            item.add_argument("--github-output", type=Path)
        item.add_argument("--notes-path")
        if name == "notes":
            item.add_argument("--output", type=Path, required=True)
        if name == "publish":
            item.add_argument("--assets-dir", type=Path, required=True)
            item.add_argument("--repository", required=True)
    args = parser.parse_args()
    try:
        if args.command == "validate-version":
            print(version(args.version))
            return
        if args.command == "draft":
            result = draft(args.repo, args.version, args.output, args.base_tag)
        elif args.command == "publish":
            result = publish(args.repo, args.tag, args.expected_commit, args.expected_tag_object,
                             args.assets_dir, args.remote or "origin", args.notes_path, args.repository)
        else:
            result = resolve(args.repo, args.tag, args.expected_commit, args.remote, args.notes_path,
                             args.expected_tag_object)
            if args.command == "resolve" and args.github_output:
                with args.github_output.open("a") as stream:
                    for key in ("version", "commit", "tag_object"):
                        stream.write(f"{key}={result[key]}\n")
            if args.command == "notes":
                args.output.parent.mkdir(parents=True, exist_ok=True)
                args.output.write_bytes(committed(args.repo, result["commit"], result["notes_path"]))
        print(json.dumps(result, sort_keys=True))
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"release: {error}\n")


if __name__ == "__main__":
    main()
