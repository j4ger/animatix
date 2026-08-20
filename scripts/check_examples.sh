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
WARN_TOTAL=0

# Lint categories that are intentional in demos and therefore allowed:
#   unused-label        — actors/curves declared for visual content, never re-referenced
#   always-overrides    — `always` deliberately overrides keyframes in reactive demos
#   unknown-type        — analyzer is single-file; imported/plugin types are unresolved
ALLOWED_WARNINGS='unused-label|always-overrides-keyframes|unknown-type'

for amx in $(find "$PROJECT_DIR/examples" -name '*.amx' -not -path '*/lib/*' -not -path '*/scenes/*' | sort); do
    TOTAL=$((TOTAL + 1))
    echo -n "Checking $(basename "$amx")... "
    PLUGIN_FLAGS=()
    if [ "$amx" = "$PROJECT_DIR/examples/projects/plugin_pulse.amx" ]; then
        PLUGIN_FLAGS=(--plugin "$PLUGIN_MANIFEST")
    fi
    OUTPUT=$(cargo run --quiet --manifest-path "$PROJECT_DIR/Cargo.toml" --bin animatix -- check "$amx" "${PLUGIN_FLAGS[@]}" 2>&1) || {
        echo "FAILED"
        FAILED=1
        continue
    }
    WARNINGS=$(echo "$OUTPUT" | grep -E 'warning|info' | grep -vE "$ALLOWED_WARNINGS" || true)
    if [ -n "$WARNINGS" ]; then
        echo "WARN"
        echo "$WARNINGS" | head -3
        FAILED=1
    else
        echo "OK"
    fi
    WARN_TOTAL=$((WARN_TOTAL + $(echo "$OUTPUT" | grep -cE 'warning|info' || true)))
done

if [ $FAILED -eq 0 ]; then
    echo "=== All $TOTAL example files clean (warnings: $WARN_TOTAL, all allowed categories) ==="
else
    echo "=== Some examples failed to parse or have unexpected warnings ==="
    exit 1
fi
