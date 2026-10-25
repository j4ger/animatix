#!/usr/bin/env bash
# file-size — enforce the source-file ceilings from docs/ecs.md §8.
#
# Two tiers, both ratchets:
#
#   ceiling  production 800 / tests 1500. A file over its ceiling must be in
#            scripts/file-size-allowlist.tsv, frozen at the size it had when
#            the split programme started. Rows are deleted when a file is
#            split, never added (except for a genuinely new legacy file in
#            the same commit that lands it) and never raised.
#
#   watch    production 600 / tests 1200. The forward guard: a file that
#            crosses the watch threshold must be frozen at that size in
#            scripts/file-size-watch.tsv in the same commit, and may not grow
#            past its frozen row afterwards. The watch tier keeps files from
#            ever *reaching* the ceiling: growth beyond 600 lines is always
#            a deliberate, reviewed act (raise the row in a commit that
#            justifies it), never silent drift. Rows are deleted when the
#            file drops back under the threshold.
#
# Exemptions (generated / deliberate single-source surfaces) live in
# `is_exempt` below and are documented in docs/ecs.md §8.
#
# Usage:
#   scripts/file-size.sh               # check — the `file-size` CI gate
#   scripts/file-size.sh --list        # print every file over its ceiling
#   scripts/file-size.sh --update      # rewrite the allowlist from the current tree
#   scripts/file-size.sh --seed-watch  # write the watch list from the current tree
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

PROD_MAX=${PROD_MAX:-800}
TEST_MAX=${TEST_MAX:-1500}
WATCH_PROD_MAX=${WATCH_PROD_MAX:-600}
WATCH_TEST_MAX=${WATCH_TEST_MAX:-1200}
ALLOWLIST="scripts/file-size-allowlist.tsv"
WATCHLIST="scripts/file-size-watch.tsv"

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

watch_ceiling_for() {
  if is_test "$1"; then echo "$WATCH_TEST_MAX"; else echo "$WATCH_PROD_MAX"; fi
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
declare -A WATCH_MAX=()
declare -A WATCH_LINE=()

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

load_watchlist() {
  [[ -f "$WATCHLIST" ]] || return 0
  local p m lineno=0
  while IFS=$'\t' read -r p m || [[ -n "${p:-}" ]]; do
    lineno=$((lineno + 1))
    [[ -z "${p:-}" || "$p" == \#* ]] && continue
    if [[ ! "$m" =~ ^[0-9]+$ ]]; then
      echo "error: $WATCHLIST:$lineno: max_lines must be an integer, got '$m'" >&2
      return 1
    fi
    if [[ -n "${WATCH_MAX[$p]:-}" ]]; then
      echo "error: $WATCHLIST:$lineno: duplicate entry for '$p'" >&2
      return 1
    fi
    WATCH_MAX["$p"]="$m"
    WATCH_LINE["$p"]="$lineno"
  done < "$WATCHLIST"
}

check_mode() {
  load_allowlist
  load_watchlist
  local fail=0 f n max watch
  declare -A seen=()
  declare -A seen_watch=()

  while IFS= read -r -d '' f; do
    is_exempt "$f" && continue
    [[ -f "$f" ]] || continue
    n=$(wc -l < "$f")

    # ── Watch tier: the forward guard at 600/1200. ──
    watch=$(watch_ceiling_for "$f")
    if (( n > watch )); then
      if [[ -n "${WATCH_MAX[$f]:-}" ]]; then
        seen_watch["$f"]=1
        if (( n > WATCH_MAX[$f] )); then
          echo "FAIL $f: $n lines; the watch tier froze it at ${WATCH_MAX[$f]} — split it, or raise the row in a commit that justifies the growth" >&2
          fail=1
        fi
      else
        echo "FAIL $f: $n lines crosses the watch threshold (> $watch) and is not in $WATCHLIST — split it, or freeze it there in the same commit" >&2
        fail=1
      fi
    fi

    # ── Ceiling tier: the hard 800/1500 limit. ──
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

  # A split landed but a row stayed behind: force both ratchets forward.
  local p
  for p in "${!ALLOW_MAX[@]}"; do
    [[ -n "${seen[$p]:-}" ]] && continue
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
  for p in "${!WATCH_MAX[@]}"; do
    [[ -n "${seen_watch[$p]:-}" ]] && continue
    if [[ ! -f "$p" ]]; then
      echo "FAIL $p: watch entry (line ${WATCH_LINE[$p]}) points at a missing file" >&2
      fail=1
      continue
    fi
    n=$(wc -l < "$p")
    watch=$(watch_ceiling_for "$p")
    if (( n <= watch )); then
      echo "FAIL $p: now $n lines (<= $watch watch threshold); remove its watch entry (line ${WATCH_LINE[$p]})" >&2
      fail=1
    fi
  done

  if (( fail == 0 )); then
    echo "file-size: ok ($(grep -vc '^#' "$ALLOWLIST" 2>/dev/null || echo 0) allowlisted, $(grep -vc '^#' "$WATCHLIST" 2>/dev/null || echo 0) watched files; ceilings production <= $PROD_MAX / tests <= $TEST_MAX, watch production <= $WATCH_PROD_MAX / tests <= $WATCH_TEST_MAX)"
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

write_frozen_list() {
  # write_frozen_list <dest> <threshold-for> <header-lines...>
  local dest="$1" thresh_fn="$2"
  shift 2
  local tmp f n
  tmp=$(mktemp)
  {
    for line in "$@"; do printf '# %s\n' "$line"; done
    while IFS= read -r -d '' f; do
      is_exempt "$f" && continue
      [[ -f "$f" ]] || continue
      n=$(wc -l < "$f")
      (( n > $($thresh_fn "$f") )) || continue
      printf '%s\t%d\n' "$f" "$n"
    done < <(collect)
  } | sort >> "$tmp"
  mv "$tmp" "$dest"
  echo "file-size: wrote $dest ($(grep -vc '^#' "$dest" 2>/dev/null || echo 0) entries)"
}

case "${1:-check}" in
  check|--check) check_mode ;;
  --list) list_mode ;;
  --update)
    write_frozen_list "$ALLOWLIST" ceiling_for \
      "path	max_lines	reason	remove_by_phase" \
      "Frozen at the Phase 0 baseline. This is a ratchet: delete a row when" \
      "the file is split; never add or raise one. See docs/ecs.md §8."
    ;;
  --seed-watch)
    write_frozen_list "$WATCHLIST" watch_ceiling_for \
      "path	max_lines" \
      "Frozen when the file crossed the watch threshold (production 600 / tests" \
      "1200). Ratchet: delete the row when the file drops under the threshold;" \
      "raise a row only in a commit that justifies the growth. See docs/ecs.md §8."
    ;;
  -h|--help)
    sed -n '2,32p' "$0" | sed 's/^# \{0,1\}//'
    ;;
  *)
    echo "usage: scripts/file-size.sh [check|--list|--update|--seed-watch]" >&2
    exit 2
    ;;
esac
