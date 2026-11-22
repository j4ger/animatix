#!/usr/bin/env bash
# Render-smoke every example: each file must render at least one non-blank
# frame. Complements check_examples.sh (parse/build gate) with a render gate.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$PROJECT_DIR"
scenes="$(python3 tools/quality/audit_corpus.py)" || exit 1

command -v identify >/dev/null 2>&1 || {
    echo "error: identify (ImageMagick) not found on PATH" >&2
    exit 1
}

ANIMATIX_BIN="${ANIMATIX_BIN:-$PROJECT_DIR/target/debug/animatix}"
if [ ! -x "$ANIMATIX_BIN" ]; then
    cargo build --quiet --manifest-path "$PROJECT_DIR/Cargo.toml" --bin animatix
fi

FAILED=0
TOTAL=0

while read -r amx; do
    TOTAL=$((TOTAL + 1))
    echo -n "Render-smoke $(basename "$amx")... "
    OK=0
    for T in 1.0 3.0 6.0; do
        OUT=$(mktemp /tmp/amx_smoke_XXXXXX.png)
        if "$ANIMATIX_BIN" image "$amx" --time "$T" -o "$OUT" > /dev/null 2>&1 \
            && [ -s "$OUT" ]; then
            DEV=$(identify -format "%[fx:standard_deviation]" "$OUT" 2>/dev/null || echo 0)
            if awk -v d="$DEV" 'BEGIN { exit !(d > 0.005) }'; then
                OK=1
                rm -f "$OUT"
                break
            fi
        fi
        rm -f "$OUT"
    done
    if [ "$OK" -eq 1 ]; then
        echo "OK"
    else
        echo "FAILED (no non-blank frame at t=1/3/6)"
        FAILED=$((FAILED + 1))
    fi
done <<< "$scenes"

if [ "$FAILED" -eq 0 ]; then
    echo "=== Render smoke passed for $TOTAL examples ==="
else
    echo "=== Render smoke FAILED for $FAILED of $TOTAL examples ==="
    exit 1
fi
