#!/usr/bin/env bash
# Render every correctness probe to /tmp/animatix-audit and build a contact
# sheet per category for review. PNGs are disposable; the .amx probes are the
# committed regression artifacts.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
PROBE_DIR="$PROJECT_DIR/dogfood/probes/008-render-correctness"
OUT_DIR="${AUDIT_OUT:-/tmp/animatix-audit}"

mkdir -p "$OUT_DIR"

# Category demos + focused probes. Render each at t=1.0 (all reveals done).
render_one() {
    local name="$1"
    local file="${PROBE_DIR}/$2"
    local out="$OUT_DIR/$name.png"
    if cargo run --quiet --manifest-path "$PROJECT_DIR/Cargo.toml" --bin animatix -- \
        image "$file" --time 1.0 -o "$out" > /dev/null 2>&1; then
        echo "rendered $name -> $out"
    else
        echo "FAILED to render $name ($file)" >&2
    fi
}

for demo in shapes text media plots containers annotations; do
    render_one "$demo" "$demo.amx"
done

for focus in "$PROBE_DIR"/focus/*.amx; do
    base="$(basename "$focus" .amx)"
    render_one "focus_$base" "focus/$base.amx"
done

# Contact sheet per category for review.
for demo in shapes text media plots containers annotations; do
    if [ -f "$OUT_DIR/$demo.png" ]; then
        montage "$OUT_DIR/$demo.png" -tile 1x1 -geometry +4+4 -background black \
            "$OUT_DIR/sheet_$demo.png" > /dev/null 2>&1 || true
    fi
done

echo "=== Done. Review PNGs and contact sheets in $OUT_DIR ==="
