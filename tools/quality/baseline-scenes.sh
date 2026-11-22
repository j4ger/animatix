#!/usr/bin/env bash
# Batch baseline scene generation and comparison.
#
# Requires GPU (for rendering baseline PNGs). This is a local scripted gate and
# does not run in PR CI. The full asset set is ~300MB and is not committed.
#
# Usage:
#   tools/quality/baseline-scenes.sh [--update] [--prefix <path>]
#
# Without --update, compares existing PNGs.
# With --update, generates/updates PNGs.
set -euo pipefail

update=""
prefix=""
while [ $# -gt 0 ]; do
  case "$1" in
    --update) update="--update" ;;
    --prefix) prefix="$2"; shift ;;
    *) echo "unknown argument: $1"; exit 1 ;;
  esac
  shift
done

cd "$(dirname "${BASH_SOURCE[0]}")/../.."
scenes="$(python3 tools/quality/audit_corpus.py)"

failed=0
while IFS= read -r f; do
  if [ -n "$prefix" ] && [[ "$f" != "$prefix"* ]]; then continue; fi
  echo "baseline: $f"
  if ! ./target/debug/animatix baseline "$f" --max-frames 8 ${update:+$update}; then
    echo "FAIL: $f" >&2
    failed=1
  fi
done <<< "$scenes"

exit $failed
