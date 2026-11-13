#!/usr/bin/env bash
# Assemble the site and release site ZIP from the same verified SDK.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
out="${1:?usage: site-artifact.sh OUT [--zip NAME.zip]}"
shift
zip_name=""
while [ $# -gt 0 ]; do
  case "$1" in
    --zip) zip_name="${2:?--zip needs a filename}"; shift 2 ;;
    *) echo "error: unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "/$out/" in
  //|/./|/web/|/*/../*|/*/./*) echo "error: unsafe output directory '$out'" >&2; exit 2 ;;
esac
case "$out" in /*) echo 'error: output must be relative to the repository' >&2; exit 2 ;; esac
case "$out" in
  _site|_site-*|target/*) ;;
  *) echo 'error: output must be _site, _site-* or a directory under target/' >&2; exit 2 ;;
esac
version="$(sed -nE '/^\[workspace\.package\]/,/^\[/s/^version = "([^"]+)".*/\1/p' Cargo.toml | head -1)"
node web/tools/sdk.mjs verify web/runtime --release --current --commit "$(git rev-parse HEAD)" --version "$version"
stage="${out}.staging-$$"
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage"
entries=(logo.svg index.html gallery.html site.css site-chrome.js site assets demos tour recipes arena scenes)
for entry in "${entries[@]}"; do
  [ -e "web/$entry" ] || { echo "error: missing runtime content web/$entry" >&2; exit 1; }
  cp -R "web/$entry" "$stage/$entry"
done
# The build's public runtime is a symlink into target/. Distributions contain
# real files; they must never retain a link to the builder's private directory.
cp -RL web/runtime "$stage/runtime"
node web/tools/sdk.mjs verify "$stage/runtime" --release
if find "$stage/runtime" -type l -print -quit | grep -q .; then
  echo 'error: SDK assembly retained a builder symlink' >&2; exit 1
fi
touch "$stage/.nojekyll"
for forbidden in tools pkg pkg-slim pkg-assets fonts embed node_modules; do
  [ ! -e "$stage/$forbidden" ] || { echo "error: unexpected legacy/development path $forbidden" >&2; exit 1; }
done
bash scripts/size-report.sh --check --site "$stage"
rm -rf "$out"
mv "$stage" "$out"
trap - EXIT
if [ -n "$zip_name" ]; then
  zip_path="$root/$zip_name"
  mkdir -p "$(dirname "$zip_path")"
  rm -f "$zip_path"
  (cd "$out" && zip -qrX "$zip_path" .)
fi
echo "site assembled at $out"
