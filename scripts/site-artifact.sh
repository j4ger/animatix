#!/usr/bin/env bash
# Assemble the deployable site directory — one implementation, used by both the
# Pages deploy and the release zip.
#
# This used to be a `run:` block inside pages.yml, which meant the release
# pipeline had to copy it. Two copies of "what the artifact is" is how a release
# zip and the live site end up with different layouts of the same build, and the
# copy nobody reads is the one that is wrong.
#
# The rules it encodes are not obvious:
#   - the runtime engine bundles (web/pkg-slim, web/pkg-assets) are gitignored
#     build output, so they must already exist. They must also be *release*
#     builds: a .dev marker or a non-release .build stamp is a debug bundle
#     several times the size. The full web/pkg is a release asset, not shipped
#     on the site.
#   - the `.br` twins are dropped. GitHub Pages does not negotiate brotli
#     precompression, so shipping them doubles the wasm bytes for no benefit.
#   - `.nojekyll` must exist or Pages runs the tree through Jekyll, which silently
#     drops any directory beginning with `_` and rewrites `assets/`.
#   - the embed bundle (web/embed/amx-player.js) is generated and gitignored. If
#     any file under web/embed/src is newer, it is rebuilt with npm; when npm is
#     unavailable the script fails rather than copying raw ES modules that the
#     browser cannot import.
#
# Usage: scripts/site-artifact.sh <outdir> [--zip <name.zip>]
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

out="${1:?usage: site-artifact.sh <outdir> [--zip name.zip]}"
shift
zip_name=""
while [ $# -gt 0 ]; do
  case "$1" in
    --zip) zip_name="${2:?--zip needs a filename}"; shift 2 ;;
    *) echo "error: unknown argument: $1" >&2; exit 2 ;;
  esac
done

# This script deletes its output directory. Refuse anything that could mean
# "somewhere I did not intend", because the argument arrives unquoted from a
# workflow line and a mistake here is a destroyed working tree.
case "$out" in
  ""|.|./|/|web|web/)
    echo "error: refusing to assemble into '$out' — pass a staging directory, e.g. _site" >&2
    exit 2
    ;;
esac
case "$out" in
  /*) echo "error: outdir must stay inside the repo (got an absolute path: $out)" >&2; exit 2 ;;
esac

for d in web/pkg-slim web/pkg-assets; do
  [ -f "$d/animatix_web_bg.wasm" ] || {
    echo "error: $d/animatix_web_bg.wasm is missing — run scripts/ci.sh gate web-build first." >&2
    echo "       Deploying without it ships pages whose embeds cannot load." >&2
    exit 1
  }
  # A debug bundle is valid for local iteration and wrong for every reader: it
  # is several times larger and unoptimized. The stamp is what distinguishes
  # them, so both the marker and the recorded profile must say release.
  if [ -f "$d/.dev" ] || ! grep -qx 'profile=release' "$d/.build" 2>/dev/null; then
    echo "error: $d is not a release build (missing/stale .build or a .dev marker)." >&2
    echo "       Build it with: nix develop .#web-build --command scripts/ci.sh gate web-build" >&2
    exit 1
  fi
done

# Build the distributable embed player bundle if it is missing or older than
# *any* file under web/embed/src — including the player/*.js modules the bare
# entry point imports. Copying src/amx-player.js alone is not a bundle: its
# `./player/playback.js` imports do not resolve in the browser.
stale_embed=0
if [ ! -s web/embed/amx-player.js ]; then
  stale_embed=1
elif [ -n "$(find web/embed/src -type f -newer web/embed/amx-player.js -print -quit)" ]; then
  stale_embed=1
fi
if [ "$stale_embed" -eq 1 ]; then
  command -v npm >/dev/null 2>&1 || {
    echo "error: web/embed/amx-player.js is missing or stale and npm is not on PATH." >&2
    echo "       Refusing to copy raw module sources as a bundle; run this inside" >&2
    echo "       'nix develop' (or install Node/npm) so 'npm run build:embed' can build it." >&2
    exit 1
  }
  npm --prefix web/tools ci --silent
  npm --prefix web/tools run build:embed
  [ -s web/embed/amx-player.js ] || {
    echo "error: web/embed/amx-player.js missing or empty after build" >&2
    exit 1
  }
  if [ -n "$(find web/embed/src -type f -newer web/embed/amx-player.js -print -quit)" ]; then
    echo "error: web/embed/amx-player.js is still older than web/embed/src after the build." >&2
    exit 1
  fi
fi

rm -rf "$out"
mkdir -p "$out"
# Copy only the runtime surface. `cp -r web/.` used to carry web/tools/
# (including CI's node_modules), the full pkg/ bundle, and embed sources into
# every deployed site. The list below is intentionally explicit so a new
# development directory cannot silently become public bytes.
runtime_entries=(
  index.html
  gallery.html
  site.css
  site-chrome.js
  site
  assets
  demos
  tour
  recipes
  arena
  scenes
  pkg-slim
  pkg-assets
)
for entry in "${runtime_entries[@]}"; do
  [ -e "web/$entry" ] || {
    echo "error: web/$entry is missing — site assembly would be incomplete." >&2
    exit 1
  }
  cp -R "web/$entry" "$out/$entry"
done
mkdir -p "$out/embed"
cp "web/embed/amx-player.js" "$out/embed/amx-player.js"
find "$out" -name '*.br' -delete
touch "$out/.nojekyll"

# These directories are build inputs, not runtime assets. Fail closed if a
# future change accidentally reintroduces one.
for forbidden in tools pkg embed/src node_modules; do
  if [ -e "$out/$forbidden" ]; then
    echo "error: site artifact contains forbidden path '$forbidden'." >&2
    exit 1
  fi
done

echo "site assembled at $out:"
du -sh "$out"
for d in pkg-slim pkg-assets; do
  printf '  web/%s: %s bytes\n' "$d" "$(wc -c < "$out/$d/animatix_web_bg.wasm" | tr -d ' ')"
done

if [ -n "$zip_name" ]; then
  # Zip from inside the directory so the archive holds `index.html`, not
  # `_site/index.html` — the unpack step of a site artifact should not need to
  # know what the staging directory was called.
  (cd "$out" && zip -qrX "$root/$zip_name" .)
  printf 'wrote %s (%s bytes)\n' "$zip_name" "$(wc -c < "$zip_name" | tr -d ' ')"
fi
