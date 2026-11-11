#!/usr/bin/env bash
# Batch baseline scene generation and comparison.
#
# Requires GPU (for rendering baseline PNGs). This is a local scripted gate and
# does not run in PR CI. The full asset set is ~300MB and is not committed.
#
# Usage:
#   scripts/baseline-scenes.sh [--update] [--prefix <path>]
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

# Re-use the multi-file project rule
project_rule() {
  local amx="$1"
  local dir
  dir=$(dirname "$amx")
  local base
  base=$(basename "$amx")
  if [ -f "$dir/main.amx" ] && [ "$base" != "main.amx" ]; then
    return 1 # skip
  fi
  return 0
}

# Collect scenes
scenes=()
# 1. examples
while read -r f; do
  if project_rule "$f"; then scenes+=("$f"); fi
done < <(find examples -name '*.amx' -not -path '*/lib/*' -not -path '*/scenes/*' | sort)

# 2. web/scenes
while read -r f; do
  if project_rule "$f"; then scenes+=("$f"); fi
done < <(find web/scenes -name '*.amx' -not -path '*/lib/*' -not -path '*/_site/*' | sort)

# 3. web/demos
while read -r f; do
  if project_rule "$f"; then scenes+=("$f"); fi
done < <(find web/demos -name '*.amx' -not -path '*/lib/*' -not -path '*/_site/*' | sort)

# Remove the plugin showcase and the pure `pub let` constant module (neither
# is a playable scene; both are imported by the scenes that are).
new_scenes=()
for f in "${scenes[@]}"; do
  case "$f" in
    */plugin_pulse.amx|*/epicycles/fourier.amx) continue ;;
  esac
  if [ -n "$prefix" ] && [[ "$f" != "$prefix"* ]]; then continue; fi
  new_scenes+=("$f")
done
scenes=("${new_scenes[@]}")

failed=0
for f in "${scenes[@]}"; do
  echo "baseline: $f"
  if ! ./target/debug/animatix baseline "$f" --max-frames 8 ${update:+$update}; then
    echo "FAIL: $f" >&2
    failed=1
  fi
done

exit $failed
