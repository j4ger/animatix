#!/usr/bin/env bash
# file-size — enforce the source-file ceilings from docs/ecs.md §5.
#
# Production source: 800 lines. Test source: 1500 lines.
#
# A file over its ceiling must be listed in scripts/file-size-allowlist.tsv
# with the line count it had when the split programme started. The allowlist
# is a ratchet: rows are deleted as files are split, never added (except for a
# genuinely new legacy file in the same commit that lands it) and never raised.
#
# Exemptions (generated / deliberate single-source surfaces) live in
# `is_exempt` below and are documented in docs/ecs.md §5.1.
#
# Usage:
#   scripts/file-size.sh            # check — the `file-size` CI gate
#   scripts/file-size.sh --list     # print every file over its ceiling
#   scripts/file-size.sh --update   # rewrite the allowlist from the current tree
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

PROD_MAX=${PROD_MAX:-800}
TEST_MAX=${TEST_MAX:-1500}
ALLOWLIST="scripts/file-size-allowlist.tsv"

is_exempt() {
  case "$1" in
    # The native plugin ABI is one `repr(C)` surface; splitting it would make a
    # layout review span files. Kept as the one documented source exemption.
    crates/animatix-plugin-api/src/lib.rs) return 0 ;;
    # Generated: wasm-bindgen output and the esbuild bundle.
    web/pkg/*|web/pkg-*/*) return 0 ;;
    web/embed/amx-player.js) return 0 ;;
    web/tools/node_modules/*) return 0 ;;
    dist/*) return 0 ;;
  esac
  return 1
}

is_test() {
  case "$1" in
    */tests/*|*/tests.rs) return 0 ;;
  esac
  return 1
}

ceiling_for() {
  if is_test "$1"; then echo "$TEST_MAX"; else echo "$PROD_MAX"; fi
}

# Documentation only: which phase of the split programme should remove the row.
phase_for() {
  case "$1" in
    */tests/*|*/tests.rs) echo "P7" ;;
    crates/animatix-gui/*) echo "P6" ;;
    crates/animatix/src/*) echo "P6" ;;
    crates/animatix-syntax/*|crates/animatix-text/*|crates/animatix-render/*|crates/animatix-web/*|crates/animatix-lsp/*|crates/animatix-analyzer/*|crates/eparts/*)
      echo "P2" ;;
    *) echo "P6" ;;
  esac
}

collect() {
  # NUL-delimited so paths with spaces survive. --others includes a new file
  # before it is staged, so the local check cannot be fooled by `git add` order.
  git ls-files -z --cached --others --exclude-standard -- '*.rs' '*.js' '*.css' '*.html'
}

declare -A ALLOW_MAX=()
declare -A ALLOW_LINE=()

load_allowlist() {
  [[ -f "$ALLOWLIST" ]] || return 0
  local p m reason phase lineno=0
  while IFS=$'\t' read -r p m reason phase || [[ -n "${p:-}" ]]; do
    lineno=$((lineno + 1))
    [[ -z "${p:-}" || "$p" == \#* ]] && continue
    if [[ ! "$m" =~ ^[0-9]+$ ]]; then
      echo "error: $ALLOWLIST:$lineno: max_lines must be an integer, got '$m'" >&2
      return 1
    fi
    if [[ -n "${ALLOW_MAX[$p]:-}" ]]; then
      echo "error: $ALLOWLIST:$lineno: duplicate entry for '$p'" >&2
      return 1
    fi
    ALLOW_MAX["$p"]="$m"
    ALLOW_LINE["$p"]="$lineno"
  done < "$ALLOWLIST"
}

check_mode() {
  load_allowlist
  local fail=0 f n max
  declare -A seen=()

  while IFS= read -r -d '' f; do
    is_exempt "$f" && continue
    [[ -f "$f" ]] || continue
    n=$(wc -l < "$f")
    max=$(ceiling_for "$f")
    (( n > max )) || continue

    if [[ -n "${ALLOW_MAX[$f]:-}" ]]; then
      seen["$f"]=1
      if (( n > ALLOW_MAX[$f] )); then
        echo "FAIL $f: $n lines; allowlist froze it at ${ALLOW_MAX[$f]} — split it instead of raising the ceiling" >&2
        fail=1
      fi
    else
      echo "FAIL $f: $n lines (> $max) and not in $ALLOWLIST — split it, or add a legacy row in the commit that lands the file" >&2
      fail=1
    fi
  done < <(collect)

  # A split landed but the allowlist row stayed: force the ratchet forward.
  local p
  for p in "${!ALLOW_MAX[@]}"; do
    if [[ ! -f "$p" ]]; then
      echo "FAIL $p: allowlist entry (line ${ALLOW_LINE[$p]}) points at a missing file" >&2
      fail=1
      continue
    fi
    n=$(wc -l < "$p")
    max=$(ceiling_for "$p")
    if (( n <= max )); then
      echo "FAIL $p: now $n lines (<= $max); remove its allowlist entry (line ${ALLOW_LINE[$p]})" >&2
      fail=1
    fi
  done

  if (( fail == 0 )); then
    echo "file-size: ok ($(grep -vc '^#' "$ALLOWLIST" 2>/dev/null || echo 0) allowlisted files; production <= $PROD_MAX, tests <= $TEST_MAX)"
  fi
  return "$fail"
}

list_mode() {
  local f n max kind
  {
    while IFS= read -r -d '' f; do
      is_exempt "$f" && continue
      [[ -f "$f" ]] || continue
      n=$(wc -l < "$f")
      max=$(ceiling_for "$f")
      (( n > max )) || continue
      if is_test "$f"; then kind="test"; else kind="source"; fi
      printf '%6d  %-6s %s\n' "$n" "$kind" "$f"
    done < <(collect)
  } | sort -rn
}

update_mode() {
  local f n max tmp
  tmp=$(mktemp)
  {
    printf '# path\tmax_lines\treason\tremove_by_phase\n'
    printf '# Frozen at the Phase 0 baseline. This is a ratchet: delete a row when\n'
    printf '# the file is split; never add or raise one. See docs/ecs.md §5.\n'
  } > "$tmp"
  {
    while IFS= read -r -d '' f; do
      is_exempt "$f" && continue
      [[ -f "$f" ]] || continue
      n=$(wc -l < "$f")
      max=$(ceiling_for "$f")
      (( n > max )) || continue
      printf '%s\t%d\tlegacy oversized file\t%s\n' "$f" "$n" "$(phase_for "$f")"
    done < <(collect)
  } | sort >> "$tmp"
  mv "$tmp" "$ALLOWLIST"
  echo "file-size: wrote $ALLOWLIST ($(grep -vc '^#' "$ALLOWLIST") entries)"
}

case "${1:-check}" in
  check|--check) check_mode ;;
  --list) list_mode ;;
  --update) update_mode ;;
  -h|--help)
    sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
    ;;
  *)
    echo "usage: scripts/file-size.sh [check|--list|--update]" >&2
    exit 2
    ;;
esac
