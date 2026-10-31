#!/usr/bin/env bash
# Report and (optionally) enforce the size budget for the web artifacts.
#
# Usage:
#   scripts/size-report.sh [--check] [--site DIR]
#
# The native release binaries are measured by their packaging gates; this
# script owns the web/wasm/site budgets because those are the artifacts whose
# bytes are served to readers.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

check=false
site="_site"
while [ $# -gt 0 ]; do
  case "$1" in
    --check) check=true; shift ;;
    --site) site="${2:?--site needs a directory}"; shift 2 ;;
    *) echo "usage: $0 [--check] [--site DIR]" >&2; exit 2 ;;
  esac
done

bytes() {
  [ -e "$1" ] && wc -c < "$1" | tr -d ' ' || echo "missing"
}

line() {
  local label="$1" path="$2"
  [ -e "$path" ] || { printf '  %-22s missing\n' "$label"; return; }
  local n
  if [ -d "$path" ]; then
    n="$(du -sb "$path" | cut -f1)"
  else
    n="$(bytes "$path")"
  fi
  printf '  %-22s %10s bytes  %s\n' "$label" "$n" "$path"
}

status=0
echo "Web engine packages:"
for spec in \
  "pkg-slim:web/pkg-slim/animatix_web_bg.wasm:7200000" \
  "pkg-assets:web/pkg-assets/animatix_web_bg.wasm:8200000" \
  "pkg-full:web/pkg/animatix_web_bg.wasm:31500000"; do
  label="${spec%%:*}"
  rest="${spec#*:}"
  path="${rest%%:*}"
  limit="${rest##*:}"
  line "$label" "$path"
  if [ "$check" = true ] && [ -e "$path" ]; then
    actual="$(bytes "$path")"
    if [ "$actual" -gt "$limit" ]; then
      echo "size budget exceeded: $label is $actual bytes (max $limit)" >&2
      status=1
    fi
  fi
done

if [ -e "$site" ]; then
  echo "Site:"
  line "_site" "$site"
  if [ "$check" = true ]; then
    actual="$(du -sb "$site" | cut -f1)"
    if [ "$actual" -gt 20000000 ]; then
      echo "size budget exceeded: $site is $actual bytes (max 20000000)" >&2
      status=1
    fi
  fi
else
  echo "Site:"
  echo "  $site missing"
fi

echo "Brotli twins:"
for p in web/pkg-slim/animatix_web_bg.wasm.br web/pkg-assets/animatix_web_bg.wasm.br web/pkg/animatix_web_bg.wasm.br; do
  line "$(basename "$(dirname "$p")")" "$p"
done

exit "$status"
