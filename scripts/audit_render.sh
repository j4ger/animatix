#!/usr/bin/env bash
# Render every correctness probe to /tmp/animatix-audit and build a contact
# sheet per category for review. PNGs are disposable; the .amx probes are the
# committed regression artifacts.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
PROBE_DIR="$PROJECT_DIR/dogfood/probes/008-render-correctness"
OUT_DIR="${AUDIT_OUT:-/tmp/animatix-audit}"

mkdir -p "$OUT_DIR"

fail=0
expected_outputs=()

# Category demos + focused probes. Render each at t=1.0 (all reveals done).
render_one() {
    local name="$1"
    local file="${PROBE_DIR}/$2"
    local out="$OUT_DIR/$name.png"
    expected_outputs+=("$out")
    if cargo run --quiet --manifest-path "$PROJECT_DIR/Cargo.toml" --bin animatix -- \
        image "$file" --time 1.0 -o "$out" > /dev/null 2>&1; then
        echo "rendered $name -> $out"
    else
        echo "FAILED to render $name ($file)" >&2
        fail=1
    fi
}

for demo in shapes text media plots containers annotations; do
    render_one "$demo" "$demo.amx"
done

for focus in "$PROBE_DIR"/focus/*.amx; do
    base="$(basename "$focus" .amx)"
    render_one "focus_$base" "focus/$base.amx"
done

# Contact sheet per category for review. A missing montage is an audit failure:
# the sheet is the review artifact, not an optional extra.
if ! command -v montage >/dev/null 2>&1; then
    echo "FAILED: montage (ImageMagick) not found; cannot build contact sheets" >&2
    fail=1
else
    for demo in shapes text media plots containers annotations; do
        src="$OUT_DIR/$demo.png"
        [ -s "$src" ] || continue
        if ! montage "$src" -tile 1x1 -geometry +4+4 -background black \
            "$OUT_DIR/sheet_$demo.png" > /dev/null 2>&1; then
            echo "FAILED: montage could not build sheet_$demo.png" >&2
            fail=1
        fi
    done
fi

for out in "${expected_outputs[@]}"; do
    if [ ! -s "$out" ]; then
        echo "FAILED: expected render missing or empty: $out" >&2
        fail=1
    fi
done

if [ "$fail" -ne 0 ]; then
    echo "audit render: FAILED — see errors above" >&2
    exit 1
fi
echo "=== Done. Review PNGs and contact sheets in $OUT_DIR ==="
