#!/usr/bin/env bash
# Materialize/check font transport files. The SDK build reads metadata directly
# from font tables and includes the catalog in runtime-manifest.json.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
case "${1:-sync}" in
  sync) node "$root/web/tools/font-pack.mjs" sync ;;
  --check) node "$root/web/tools/font-pack.mjs" --check ;;
  *) echo "usage: web-fonts.sh [sync|--check]" >&2; exit 2 ;;
esac
