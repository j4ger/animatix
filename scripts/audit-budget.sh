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
    sort
}

failed=0
while IFS= read -r scene; do
  [ -z "$scene" ] && continue
  dir=$(dirname "$scene")
  base=$(basename "$scene")
  if [ -f "$dir/main.amx" ] && [ "$base" != "main.amx" ]; then
    continue
  fi
  if [ "$MODE" = "update" ]; then
    "$BIN" audit "$scene" --render never --policy default \
      --budget "$BUDGET_FILE" --update-budget --format json >/dev/null
  else
    if ! "$BIN" audit "$scene" --render never --policy default \
      --budget "$BUDGET_FILE" --format json >/dev/null; then
      echo "budget exceeded: $scene" >&2
      failed=1
    fi
  fi
done < <(scenes)

if [ "$MODE" = "update" ]; then
  echo "budget updated: $BUDGET_FILE"
fi
exit "$failed"