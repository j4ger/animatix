#!/usr/bin/env bash
# Generate bounded theme-aware widget screenshots for visual review.
#
# Usage:
#   bash scripts/gui-screenshots.sh [output-dir]
#
# Outputs PNGs under target/gui-screenshots/. Each widget is rendered once per
# theme and the binary closes itself after saving.

set -euo pipefail

OUT_DIR="${1:-target/gui-screenshots}"
FEATURES="dev-screenshots"
BIN="widget-screenshot"

mkdir -p "$OUT_DIR"

TIMEOUT_CMD=()
if command -v timeout >/dev/null 2>&1; then
  TIMEOUT_CMD=(timeout 45s)
fi

RUN_BASE=(cargo run --quiet --features "$FEATURES" --bin "$BIN" --)
RUN_CMD=("${RUN_BASE[@]}")
if [[ -z "${DISPLAY:-}" ]] && command -v xvfb-run >/dev/null 2>&1; then
  RUN_CMD=(xvfb-run -a "${RUN_CMD[@]}")
fi

widgets=(overview buttons rows card section-headers field empty-state palette)
themes=(dark light)

for theme in "${themes[@]}"; do
  for widget in "${widgets[@]}"; do
    output="$OUT_DIR/${theme}-${widget}.png"
    echo "Rendering $theme/$widget -> $output"
    "${TIMEOUT_CMD[@]}" "${RUN_CMD[@]}" \
      --widget "$widget" \
      --output "$output" \
      --width 520 \
      --height 320 \
      --theme "$theme"
  done
done

echo "Screenshots written to $OUT_DIR"
