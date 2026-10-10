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
#   - the engine bundles (web/pkg, web/pkg-slim) are gitignored build output, so
#     they must already exist. An empty pkg/ directory deploys a page whose every
#     embed reports a load error, and nothing in the layout says so.
#   - the `.br` twins are dropped. GitHub Pages does not negotiate brotli
#     precompression, so shipping them doubles the wasm bytes for no benefit.
#   - `.nojekyll` must exist or Pages runs the tree through Jekyll, which silently
#     drops any directory beginning with `_` and rewrites `assets/`.
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

for d in web/pkg web/pkg-slim; do
  [ -f "$d/animatix_web_bg.wasm" ] || {
    echo "error: $d/animatix_web_bg.wasm is missing — run scripts/ci.sh gate web-build first." >&2
    echo "       Deploying without it ships pages whose embeds cannot load." >&2
    exit 1
  }
done

# Build the distributable embed player bundle if missing or older than src
if [ ! -f "web/embed/amx-player.js" ] || [ "web/embed/src/amx-player.js" -nt "web/embed/amx-player.js" ]; then
  if command -v npm >/dev/null 2>&1; then
    npm --prefix web/tools ci --silent
    npm --prefix web/tools run build:embed
  else
    mkdir -p web/embed
    cp web/embed/src/amx-player.js web/embed/amx-player.js
  fi
fi

rm -rf "$out"
mkdir -p "$out"
cp -r web/. "$out"/
find "$out" -name '*.br' -delete
touch "$out/.nojekyll"

echo "site assembled at $out:"
du -sh "$out"
for d in pkg pkg-slim; do
  printf '  web/%s: %s bytes\n' "$d" "$(wc -c < "$out/$d/animatix_web_bg.wasm" | tr -d ' ')"
done

if [ -n "$zip_name" ]; then
  # Zip from inside the directory so the archive holds `index.html`, not
  # `_site/index.html` — the unpack step of a site artifact should not need to
  # know what the staging directory was called.
  (cd "$out" && zip -qrX "$root/$zip_name" .)
  printf 'wrote %s (%s bytes)\n' "$zip_name" "$(wc -c < "$zip_name" | tr -d ' ')"
fi
