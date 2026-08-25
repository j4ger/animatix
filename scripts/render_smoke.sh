#!/usr/bin/env bash
# Render-smoke every example: each file must render at least one non-blank
# frame. Complements check_examples.sh (parse/build gate) with a render gate.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

FAILED=0
TOTAL=0

for amx in $(find "$PROJECT_DIR/examples" -name '*.amx' -not -path '*/lib/*' -not -path '*/scenes/*' | sort); do
    dir=$(dirname "$amx")
    # Multi-file projects are smoked via their main.amx entry only.
    if [ -f "$dir/main.amx" ] && [ "$(basename "$amx")" != "main.amx" ]; then
        continue
    fi
    TOTAL=$((TOTAL + 1))
    echo -n "Render-smoke $(basename "$amx")... "
    OK=0
    for T in 1.0 3.0 6.0; do
        OUT=$(mktemp /tmp/amx_smoke_XXXXXX.png)
        if cargo run --quiet --manifest-path "$PROJECT_DIR/Cargo.toml" --bin animatix -- \
            image "$amx" --time "$T" -o "$OUT" > /dev/null 2>&1 \
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
done

if [ "$FAILED" -eq 0 ]; then
    echo "=== Render smoke passed for $TOTAL examples ==="
else
    echo "=== Render smoke FAILED for $FAILED of $TOTAL examples ==="
    exit 1
fi
