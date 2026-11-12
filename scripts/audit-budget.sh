#!/usr/bin/env bash
# Generate or check the ratchetable audit budget.
#
# The budget freezes the current per-file error/warning counts so legacy debt
# cannot grow: a new warning fails CI, and cleaning a scene lets the budget
# shrink. Only `--update` writes; `--check` is what CI runs.
#
# Usage:
#   scripts/audit-budget.sh --check      # fail if any count exceeds the budget
#   scripts/audit-budget.sh --update     # rewrite counts from the current corpus
set -euo pipefail

MODE="check"
case "${1:-}" in
  ""|--check) MODE="check" ;;
  --update) MODE="update" ;;
  --help|-h)
    echo "usage: $0 [--check|--update]"
    exit 0
    ;;
  *)
    echo "unknown argument: $1" >&2
    exit 2
    ;;
esac

BUDGET_FILE="${ANIMATIX_AUDIT_BUDGET:-.animatix/audit-budget.json}"
BIN="${ANIMATIX_BIN:-target/debug/animatix}"
if [ ! -x "$BIN" ]; then
  echo "missing $BIN — build it first (cargo build --bin animatix)" >&2
  exit 2
fi
mkdir -p "$(dirname "$BUDGET_FILE")"

# The same scene walk the `scene-audit` gate uses: component libraries, site
# copies, the plugin showcase and the pure-`pub let` module are not standalone
# scenes; multi-file projects are driven through their `main.amx`.
scenes() {
  find examples web/scenes web/demos -name '*.amx' \
    -not -path '*/lib/*' \
    -not -path '*/_site/*' \
    -not -path '*/plugin_pulse.amx' \
    -not -path '*/epicycles/fourier.amx' |
    sort |
    while IFS= read -r scene; do
      [ -z "$scene" ] && continue
      dir=$(dirname "$scene")
      base=$(basename "$scene")
      if [ -f "$dir/main.amx" ] && [ "$base" != "main.amx" ]; then
        continue
      fi
      printf '%s\n' "$scene"
    done
}

if [ "$MODE" = "update" ]; then
  # Updating mutates a shared file, so this walk must remain serial.
  while IFS= read -r scene; do
    [ -z "$scene" ] && continue
    "$BIN" audit "$scene" --render never --policy default \
      --budget "$BUDGET_FILE" --update-budget --format json >/dev/null
    printf 'OK budget-update: %s\n' "$scene"
  done < <(scenes)
  echo "budget updated: $BUDGET_FILE"
  exit 0
fi

jobs="${ANIMATIX_SCENE_AUDIT_JOBS:-2}"
if ! [[ "$jobs" =~ ^[1-4]$ ]]; then
  echo "error: ANIMATIX_SCENE_AUDIT_JOBS must be between 1 and 4" >&2
  exit 2
fi
export BIN BUDGET_FILE

audit_one() {
  local scene="$1"
  if ! "$BIN" audit "$scene" --render never --policy default \
    --budget "$BUDGET_FILE" --format json >/dev/null; then
    echo "FAIL budget-audit: $scene" >&2
    return 1
  fi
  printf 'OK budget-audit: %s\n' "$scene"
}
export -f audit_one

# GNU xargs preserves spaces in each newline-delimited pathname and aggregates
# worker failures into a nonzero status. Checks are read-only; updates above are
# intentionally serial because every audit writes the same budget file.
if ! scenes | xargs -d '\n' -r -n 1 -P "$jobs" bash -c 'audit_one "$1"' _; then
  echo "budget-audit: one or more scene audits failed" >&2
  exit 1
fi
