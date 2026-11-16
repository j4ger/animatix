#!/usr/bin/env bash
# Explicit workspace version edit. Never stages, commits, tags, or pushes.
# Usage: tools/release/bump-version.sh [--dry-run] MAJOR.MINOR.PATCH[-pre][+build]
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

dry_run=0
if [ "${1:-}" = "--dry-run" ]; then
  dry_run=1
  shift
fi

new="${1:?usage: bump-version.sh [--dry-run] <version>, no leading v (e.g. 0.1.0 or 0.2.0-rc.1)}"

if [ "$#" -ne 1 ]; then
  echo "error: expected exactly one explicit version" >&2
  exit 1
fi
python3 tools/release/release.py validate-version "$new" >/dev/null

# Only touch a clean, tracked version pair. Staged or unstaged edits belong to
# the author and cannot be used as a rollback baseline for this operation.
for manifest in Cargo.toml Cargo.lock; do
  if [ ! -f "$manifest" ] || [ -L "$manifest" ]; then
    echo "error: $manifest must be a regular file" >&2
    exit 1
  fi
  git ls-files --error-unmatch -- "$manifest" >/dev/null
done
if ! git diff --quiet -- Cargo.toml Cargo.lock || ! git diff --cached --quiet -- Cargo.toml Cargo.lock; then
  echo "error: Cargo.toml and Cargo.lock must have no staged or unstaged changes" >&2
  exit 1
fi

old=$(sed -nE '/^\[workspace\.package\]/,/^\[/s/^version = "([^"]+)".*/\1/p' Cargo.toml | head -1)
if [ -z "$old" ]; then
  echo "error: no version found under [workspace.package] in Cargo.toml" >&2
  exit 1
fi
if [ "$old" = "$new" ]; then
  echo "already at $new — nothing to do" >&2
  exit 1
fi

if [ "$dry_run" -eq 1 ]; then
  echo "dry-run: would rewrite Cargo.toml [workspace.package] version $old -> $new"
  echo "dry-run: cargo metadata would refresh Cargo.lock's workspace-member versions"
  echo "dry-run: the bump commit must include both Cargo.toml and Cargo.lock"
  exit 0
fi

# Roll back the pair together on failure or a catchable interruption. SIGKILL
# cannot run traps; the retained files should be reviewed after such a kill.
backup="$(mktemp -d "${TMPDIR:-/tmp}/animatix-version.XXXXXX")"
cp -p Cargo.toml "$backup/Cargo.toml"
cp -p Cargo.lock "$backup/Cargo.lock"
succeeded=0
restore_on_exit() {
  local status=$? restore_failed=0
  trap - EXIT INT TERM
  if [ "$succeeded" -ne 1 ]; then
    cp -p "$backup/Cargo.toml" Cargo.toml || restore_failed=1
    cp -p "$backup/Cargo.lock" Cargo.lock || restore_failed=1
  fi
  if [ "$restore_failed" -ne 0 ]; then
    echo "error: rollback incomplete; original files retained at $backup" >&2
    exit 1
  fi
  rm -rf "$backup"
  exit "$status"
}
trap restore_on_exit EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

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

# A bump that changes only one of the two files is a half-bump. Leave staging
# and the release commit to the author, but reject an inconsistent worktree.
for changed in Cargo.toml Cargo.lock; do
  if ! git status --porcelain -- "$changed" | grep -q .; then
    echo "error: $changed was not changed by the bump; refusing a half-bump" >&2
    exit 1
  fi
done

# Prove it, rather than assuming: the same check CI runs, run here so a bump
# cannot leave a member behind.
tools/ci/ci.sh gate meta-version

succeeded=1
echo "version $old -> $new"
echo "review and commit Cargo.toml and Cargo.lock with ordinary git; no tag or push was made"
