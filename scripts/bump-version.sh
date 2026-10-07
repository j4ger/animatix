#!/usr/bin/env bash
# The version half of a release.
#
# `cog bump` writes the changelog commit and the tag, but it never edits a
# manifest — so the version number in Cargo.toml has to come from somewhere. The
# obvious answer was cargo-edit's `cargo set-version`, which is a second tool to
# pin, to build, and to keep in step with the workspace. It is not needed:
# `[workspace.package]` put the version in exactly one line, and cargo refreshes
# `Cargo.lock` itself once resolution sees the change.
#
# What is left to get wrong is only: change the right line, prove every member
# followed, and refuse a number that is not a version.
#
# Usage: scripts/bump-version.sh 0.2.0
#        scripts/bump-version.sh 0.2.0-rc.1
#
# Called from cog.toml's pre_bump_hooks as `scripts/bump-version.sh {{version}}`,
# which runs it before the bump commit, so the tag and the manifests agree.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

new="${1:?usage: bump-version.sh <version>, no leading v (e.g. 0.1.0 or 0.2.0-rc.1)}"

case "$new" in
  [0-9]*.[0-9]*.[0-9]*)
    # `1.2.3extra` is not a version. Anything after the patch number must start
    # with a hyphen (prerelease) or a plus (build metadata), per semver.
    rest="${new#[0-9]*.[0-9]*.[0-9]*}"
    case "$rest" in
      ""|-*|+*) : ;;
      *) echo "error: '$new' is not a version (expected MAJOR.MINOR.PATCH[-pre][+build])" >&2; exit 1 ;;
    esac
    ;;
  *) echo "error: '$new' is not a version (expected MAJOR.MINOR.PATCH[-pre][+build])" >&2; exit 1 ;;
esac

old=$(sed -nE '/^\[workspace\.package\]/,/^\[/s/^version = "([^"]+)".*/\1/p' Cargo.toml | head -1)
if [ -z "$old" ]; then
  echo "error: no version found under [workspace.package] in Cargo.toml" >&2
  exit 1
fi
if [ "$old" = "$new" ]; then
  echo "already at $new — nothing to do" >&2
  exit 1
fi

# The address range matters: `version = "…"` also appears in every dependency
# table, and an unanchored substitution would rewrite those too.
sed -i "/^\[workspace\.package\]/,/^\[/s/^version = \"[^\"]*\"/version = \"$new\"/" Cargo.toml

now=$(sed -nE '/^\[workspace\.package\]/,/^\[/s/^version = "([^"]+)".*/\1/p' Cargo.toml | head -1)
if [ "$now" != "$new" ]; then
  echo "error: Cargo.toml still reads $now after the edit" >&2
  exit 1
fi

# Cargo.lock records every workspace member's version. Resolution rewrites it;
# `metadata` is the cheapest command that forces a resolve without compiling.
cargo metadata --format-version 1 >/dev/null

# Prove it, rather than assuming: the same check CI runs, run here so a bump
# cannot leave a member behind.
scripts/ci.sh gate meta-version

echo "version $old -> $new"
echo "staged by cog's post_bump_hooks: Cargo.toml, Cargo.lock"
