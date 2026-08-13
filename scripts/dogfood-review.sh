#!/usr/bin/env bash
# Run a dogfood review in the GUI and block until the reviewer finishes.
#
# Usage:
#   bash scripts/dogfood-review.sh <slug>
#
# The script validates the run, builds the GUI once, and launches the review
# window. It exits 0 after the reviewer closes the window or clicks the
# "mark done" button (which writes review.done).
#
# This script is designed for an agent workflow: an agent creates a run, starts
# this script in the background, and resumes when the script exits.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

SLUG="${1:-}"
if [[ -z "$SLUG" ]]; then
  echo "usage: bash scripts/dogfood-review.sh <slug>" >&2
  exit 2
fi

RUN_DIR="$PROJECT_DIR/dogfood/runs/$SLUG"
if [[ ! -d "$RUN_DIR" ]]; then
  echo "review run not found: $RUN_DIR" >&2
  exit 2
fi

AMX_COUNT="$(find "$RUN_DIR" -maxdepth 1 -name '*.amx' | wc -l | tr -d ' ')"
if [[ "$AMX_COUNT" -lt 2 ]]; then
  echo "review run needs at least two .amx variants: $RUN_DIR" >&2
  exit 2
fi

echo "Validating dogfood run: $SLUG"
for amx in "$RUN_DIR"/*.amx; do
  echo "  check $(basename "$amx")"
  cargo run --quiet --manifest-path "$PROJECT_DIR/Cargo.toml" --bin animatix -- check "$amx"
done

echo "Building review GUI"
cargo build --quiet --manifest-path "$PROJECT_DIR/Cargo.toml" --bin animatix-gui

echo "Starting review GUI: $RUN_DIR"
cargo run --quiet --manifest-path "$PROJECT_DIR/Cargo.toml" --bin animatix-gui -- --review "$RUN_DIR"

if [[ -f "$RUN_DIR/review.done" ]]; then
  echo "Review marked done: $SLUG"
else
  echo "Review window closed: $SLUG"
fi
