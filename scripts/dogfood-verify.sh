#!/usr/bin/env bash
# Dogfood verification gate.
#
# Real-content dogfood projects must pass `animatix check` with only the
# documented warning categories allowed, and — when a `verify.txt` is present —
# pass `animatix verify` content checks (see dogfood/README.md).
#
# Why this exists: `check`/`lint` see only the build model and a whole-frame
# render smoke stays green while one actor silently disappears. `verify.txt`
# asserts what is actually on screen (an actor is visible, a region changes, a
# frame is not blank). This gate makes a dogfood project's known-good rendering
# a checked fact instead of a human eyeball pass.
#
# This is the local counterpart to scripts/check_examples.sh. It is
# intentionally NOT wired into CI: dogfood content tracks the language as it
# changes and the render checks need a GPU. Run it before promoting a project
# to examples/ and after any engine fix pass that touches rendering.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BIN="$PROJECT_DIR/target/debug/animatix"

# Warning categories that are intentional in dogfood content:
#   unused-label            — actors declared for visual content, never referenced
#   always-overrides-keyframes — reactive readouts deliberately override styling
#   unknown-type            — analyzer is single-file; imported/plugin types unresolved
ALLOWED_WARNINGS='unused-label|always-overrides-keyframes|unknown-type'

echo "=== Building animatix CLI ==="
if ! cargo build --quiet --manifest-path "$PROJECT_DIR/Cargo.toml" --bin animatix; then
    echo "FAILED to build animatix CLI"
    exit 1
fi

FAILED=0
TOTAL=0

for entry in "$PROJECT_DIR"/dogfood/projects/*/entry.amx; do
    [ -f "$entry" ] || continue
    name="$(basename "$(dirname "$entry")")"
    TOTAL=$((TOTAL + 1))

    printf 'Check  %s... ' "$name"
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

    checks_file="$(dirname "$entry")/verify.txt"
    if [ -f "$checks_file" ]; then
        printf 'Verify %s... ' "$name"
        if VERIFY_OUTPUT="$("$BIN" verify "$entry" 2>&1)"; then
            summary="$(echo "$VERIFY_OUTPUT" | grep -oE '[0-9]+ of [0-9]+ checks passed' | head -1)"
            echo "OK (${summary:-rendered})"
        else
            echo "FAILED"
            echo "$VERIFY_OUTPUT" | grep -E 'FAIL|verify:' | head -10
            FAILED=1
        fi
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
