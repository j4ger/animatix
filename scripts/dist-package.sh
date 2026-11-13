#!/usr/bin/env bash
# Package a release artifact — the one place that knows what a shipped file is.
#
# The alternative was `tar`/`zip` lines inside release.yml, which is how the Pages
# deploy and the release zip became two different answers to "what is the site
# artifact". Keeping it here also means a developer can produce exactly what CI
# will ship: `scripts/dist-package.sh linux target/dist`.
#
# Usage: scripts/dist-package.sh <platform> [<bin-dir>]
#   linux          animatix + animatix-gui, `video` built, .tar.gz
#   macos          same two binaries, no `video`, .zip
#   windows        same two .exe, no `video`, .zip
#   site           the deployed tree (scripts/site-artifact.sh), .zip
#   web-sdk        one verified SDK, including optional code and font faces
#
# The version in the filename is the workspace version from Cargo.toml. When a tag
# triggered the build, ANIMATIX_RELEASE_VERSION says which one, and a mismatch is
# an error: a v0.1.2 tag whose manifests still say 0.1.1 means the checkout is not
# what the tag names, and every binary in the release would lie about it.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

platform="${1:?usage: dist-package.sh <linux|macos|windows|site|web-sdk> [bin-dir]}"
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
    echo "       pre_bump_hooks running scripts/bump-version.sh) should have" >&2
    echo "       committed the new version into the same commit as the tag." >&2
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
    [ -n "$bindir" ] || { echo "error: linux needs a complete package directory" >&2; exit 2; }
    name="animatix-${NAME_V}-x86_64-linux-gnu-video"
    for binary in animatix animatix-gui ffmpeg ffprobe; do
      [ -x "$bindir/bin/$binary" ] || { echo "error: $bindir/bin/$binary missing" >&2; exit 1; }
    done
    [ -d "$bindir/lib" ] && [ -d "$bindir/share" ] || { echo 'error: Linux runtime closure/metadata missing' >&2; exit 1; }
    source_archive="$bindir.source.tar.gz"
    [ -f "$source_archive" ] || { echo "error: corresponding source archive missing: $source_archive" >&2; exit 1; }
    python3 packaging/linux/source-package.py verify "$source_archive" >/dev/null
    expected_source_sha="$(sed -n 's/^corresponding_source_sha256=//p' "$bindir/share/BUILD-METADATA.txt")"
    actual_source_sha="$(sha256sum "$source_archive" | cut -d' ' -f1)"
    [ "$expected_source_sha" = "$actual_source_sha" ] || { echo 'error: source archive does not match binary build metadata' >&2; exit 1; }
    expected_source_name="$(sed -n 's/^corresponding_source_archive=//p' "$bindir/share/BUILD-METADATA.txt")"
    [ "$expected_source_name" = "${name}-source.tar.gz" ] || { echo 'error: source archive name does not match binary build metadata' >&2; exit 1; }
    cp "$source_archive" "$dist/${name}-source.tar.gz"
    tar -czf "$dist/$name.tar.gz" -C "$bindir" .
    echo "wrote dist/$name.tar.gz"
    echo "wrote dist/${name}-source.tar.gz"
    ;;
  macos|windows)
    [ -n "$bindir" ] || { echo "error: $platform needs a bin dir" >&2; exit 2; }
    # Written as ifs, not `[ … ] && x=1` one-liners: under `set -e` a failing
    # test at the end of a branch is also that branch's exit status.
    if [ "$platform" = windows ]; then ext=".exe"; else ext=""; fi
    stage="target/native-package-$platform"
    rm -rf "$stage"
    mkdir -p "$stage"
    python3 scripts/native-package-check.py "$platform" "$bindir" > "$stage/runtime-dependencies.json"
    triple="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["architecture"])' "$stage/runtime-dependencies.json")"
    name="animatix-${NAME_V}-${triple}-${platform}"
    for b in animatix animatix-gui; do
      [ -f "$bindir/$b$ext" ] || { echo "error: $bindir/$b$ext missing" >&2; exit 1; }
    done
    for b in animatix animatix-gui; do cp "$bindir/$b$ext" "$stage/"; done
    for license in LICENSE LICENSE-MIT LICENSE-APACHE; do
      if [ -f "$license" ]; then cp "$license" "$stage/"; fi
    done
    rm -f "$dist/$name.zip"
    (cd "$stage" && zip -qrX "$dist/$name.zip" .)
    echo "wrote dist/$name.zip"
    ;;
  site)
    bash scripts/site-artifact.sh _site --zip "dist/animatix-${NAME_V}-site.zip"
    ;;
  web-sdk)
    node web/tools/sdk.mjs verify web/runtime --release --current --commit "$(git rev-parse HEAD)" --version "$V"
    stage="target/web-sdk-package"
    rm -rf "$stage"
    mkdir -p "$stage"
    cp -RL web/runtime/. "$stage/"
    node web/tools/sdk.mjs verify "$stage" --release
    name="animatix-${NAME_V}-web-sdk.zip"
    rm -f "$dist/$name"
    (cd "$stage" && zip -qrX "$dist/$name" .)
    echo "wrote dist/$name"
    ;;
  *)
    echo "error: unknown platform: $platform" >&2
    exit 2
    ;;
esac

ls -la "$dist"
