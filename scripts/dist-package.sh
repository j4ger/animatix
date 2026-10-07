#!/usr/bin/env bash
# Package a release artifact — the one place that knows what a shipped file is.
#
# The alternative was `tar`/`zip` lines inside release.yml, which is how the Pages
# deploy and the release zip became two different answers to "what is the site
# artifact". Keeping it here also means a developer can produce exactly what CI
# will ship: `scripts/dist-package.sh linux target/release`.
#
# Usage: scripts/dist-package.sh <platform> [<bin-dir>]
#   linux          animatix + animatix-gui, `video` built, .tar.gz
#   macos          same two binaries, no `video`, .zip
#   windows        same two .exe, no `video`, .zip
#   site           the deployed tree (scripts/site-artifact.sh), .zip
#   wasm           web/pkg-slim and web/pkg, one .zip each
#
# The version in the filename is the workspace version from Cargo.toml. When a tag
# triggered the build, ANIMATIX_RELEASE_VERSION says which one, and a mismatch is
# an error: a v0.1.2 tag whose manifests still say 0.1.1 means the checkout is not
# what the tag names, and every binary in the release would lie about it.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

platform="${1:?usage: dist-package.sh <linux|macos|windows|site|wasm> [bin-dir]}"
bindir="${2:-}"

dist="$root/dist"
mkdir -p "$dist"

version() {
  sed -nE '/^\[workspace\.package\]/,/^\[/s/^version = "([^"]+)".*/\1/p' Cargo.toml | head -1
}

V="$(version)"
if [ -z "$V" ]; then
  echo "error: no [workspace.package] version in Cargo.toml" >&2
  exit 1
fi
if [ -n "${ANIMATIX_RELEASE_VERSION:-}" ]; then
  want="${ANIMATIX_RELEASE_VERSION#v}"
  if [ "$want" != "$V" ]; then
    echo "error: building for tag '$ANIMATIX_RELEASE_VERSION' but the manifests say $V." >&2
    echo "       The checkout does not match the tag; the bump hook (cog's" >&2
    echo "       pre_bump_hooks running cargo set-version) should have committed the" >&2
    echo "       new version into the same commit as the tag." >&2
    exit 1
  fi
fi

# Nightly builds ship the same binaries but must not be named as if they were a
# release, so ANIMATIX_RELEASE_LABEL replaces the version in every filename.
NAME_V="v${V}"
if [ -n "${ANIMATIX_RELEASE_LABEL:-}" ]; then
  NAME_V="$ANIMATIX_RELEASE_LABEL"
fi

case "$platform" in
  linux)
    [ -n "$bindir" ] || { echo "error: linux needs a bin dir" >&2; exit 2; }
    name="animatix-${NAME_V}-x86_64-linux-gnu-video"
    for b in animatix animatix-gui; do
      [ -x "$bindir/$b" ] || { echo "error: $bindir/$b missing or not executable" >&2; exit 1; }
    done
    tar -czf "$dist/$name.tar.gz" -C "$bindir" animatix animatix-gui
    echo "wrote dist/$name.tar.gz"
    ;;
  macos|windows)
    [ -n "$bindir" ] || { echo "error: $platform needs a bin dir" >&2; exit 2; }
    # Written as ifs, not `[ … ] && x=1` one-liners: under `set -e` a failing
    # test at the end of a branch is also that branch's exit status.
    if [ "$platform" = windows ]; then
      ext=".exe"
      triple=x86_64
    else
      ext=""
      if [ "$(uname -m)" = arm64 ]; then triple=aarch64; else triple=x86_64; fi
    fi
    name="animatix-${NAME_V}-${triple}-${platform}"
    for b in animatix animatix-gui; do
      [ -f "$bindir/$b$ext" ] || { echo "error: $bindir/$b$ext missing" >&2; exit 1; }
    done
    # zip, not tar: both platforms unpack it natively, and a tar.gz of .exe files
    # is a thing users have to know to unpack with a flag.
    (cd "$bindir" && zip -qX "$dist/$name.zip" animatix$ext animatix-gui$ext)
    echo "wrote dist/$name.zip"
    ;;
  site)
    bash scripts/site-artifact.sh _site --zip "dist/animatix-${NAME_V}-site.zip"
    ;;
  wasm)
    for spec in "slim:web/pkg-slim" "full:web/pkg"; do
      label="${spec%%:*}"
      d="${spec#*:}"
      [ -f "$d/animatix_web_bg.wasm" ] || {
        echo "error: $d/animatix_web_bg.wasm missing — run scripts/ci.sh gate web-build first." >&2
        exit 1
      }
      (cd "$d" && zip -qrX "$dist/animatix-${NAME_V}-wasm-${label}.zip" .)
      echo "wrote dist/animatix-${NAME_V}-wasm-${label}.zip"
    done
    ;;
  *)
    echo "error: unknown platform: $platform" >&2
    exit 2
    ;;
esac

ls -la "$dist"
