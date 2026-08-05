#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo "=== Checking all example .amx files parse cleanly ==="
FAILED=0
TOTAL=0

for amx in $(find "$PROJECT_DIR/examples" -name '*.amx' -not -path '*/lib/*' -not -path '*/scenes/*' | sort); do
    TOTAL=$((TOTAL + 1))
    echo -n "Checking $(basename "$amx")... "
    if cargo run --manifest-path "$PROJECT_DIR/Cargo.toml" --bin animatix -- check "$amx" > /dev/null 2>&1; then
        echo "OK"
    else
        echo "FAILED"
        FAILED=1
    fi
done

if [ $FAILED -eq 0 ]; then
    echo "=== All $TOTAL example files parse cleanly ==="
else
    echo "=== Some examples failed to parse ==="
    exit 1
fi