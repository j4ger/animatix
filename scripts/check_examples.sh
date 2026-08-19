#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# The plugin showcase example needs the native demo plugin loaded; build it
# so `check --plugin <manifest>` can install the library.
echo "=== Building animatix-plugin-demo (needed by projects/plugin_pulse.amx) ==="
cargo build --manifest-path "$PROJECT_DIR/Cargo.toml" -p animatix-plugin-demo

PLUGIN_MANIFEST="$PROJECT_DIR/crates/animatix-plugin-demo/demo.amx-plugin.toml"

echo "=== Checking all example .amx files parse cleanly ==="
FAILED=0
TOTAL=0

for amx in $(find "$PROJECT_DIR/examples" -name '*.amx' -not -path '*/lib/*' -not -path '*/scenes/*' | sort); do
    TOTAL=$((TOTAL + 1))
    echo -n "Checking $(basename "$amx")... "
    PLUGIN_FLAGS=()
    if [ "$amx" = "$PROJECT_DIR/examples/projects/plugin_pulse.amx" ]; then
        PLUGIN_FLAGS=(--plugin "$PLUGIN_MANIFEST")
    fi
    if cargo run --manifest-path "$PROJECT_DIR/Cargo.toml" --bin animatix -- check "$amx" "${PLUGIN_FLAGS[@]}" > /dev/null 2>&1; then
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
