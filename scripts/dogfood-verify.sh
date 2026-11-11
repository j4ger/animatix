#!/usr/bin/env bash
# Dogfood verification gate.
#
# Dogfood projects must pass `animatix check` with only the
# documented warning categories allowed, and pass `animatix baseline`
# for regression testing of visual output.
#
# `baseline` is a regression asset check that requires a GPU, so
# this is a local/scripted gate, not part of CI.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BIN=${ANIMATIX_BIN:-target/debug/animatix}
[ -x "$BIN" ] || { echo "no $BIN — run: cargo build -p animatix-cli"; exit 1; }

# Warning categories that are intentional in dogfood content:
#   unused-label            — actors declared for visual content, never referenced
#   always-overrides-keyframes — reactive readouts deliberately override styling
#   unknown-type            — analyzer is single-file; imported/plugin types unresolved
ALLOWED_WARNINGS='unused-label|always-overrides-keyframes|unknown-type'

FAILED=0
TOTAL=0

for entry in "$PROJECT_DIR"/dogfood/projects/*/entry.amx; do
    [ -f "$entry" ] || continue
    name="$(basename "$(dirname "$entry")")"
    TOTAL=$((TOTAL + 1))

    printf 'Check %s... ' "$name"
    OUTPUT="$("$BIN" check "$entry" 2>&1)" || {
        echo "FAILED"
        echo "$OUTPUT" | head -5
        FAILED=1
        continue
    }
    WARNINGS="$(echo "$OUTPUT" | grep -E 'warning|info' | grep -vE "$ALLOWED_WARNINGS" || true)"
    if [ -n "$WARNINGS" ]; then
        echo "WARN"
        echo "$WARNINGS" | head -3
        FAILED=1
        continue
    fi
    echo "OK"

    printf 'Baseline %s... ' "$name"
    if "$BIN" baseline "$entry" --max-frames 8 >/dev/null 2>&1; then
        echo "OK"
    else
        echo "FAILED"
        FAILED=1
    fi
done

if [ "$TOTAL" -eq 0 ]; then
    echo "=== No dogfood projects found under dogfood/projects/*/entry.amx ==="
    exit 1
fi

if [ "$FAILED" -eq 0 ]; then
    echo "=== All $TOTAL dogfood project(s) verified ==="
else
    echo "=== Dogfood verification FAILED ==="
    exit 1
fi
