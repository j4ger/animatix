#!/usr/bin/env bash
# Baseline regression for the transformer walkthrough's seven scenes.
#
# Each scene renders a storyboard and a final resting frame. This script
# verifies that the current rendered output matches the baseline PNGs.
#
# Needs a GPU, so it is a local/scripted gate rather than part of `cargo test`.
# Usage: scripts/transformer-verify.sh
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
DIR=web/demos/transformer/scenes
BIN=${ANIMATIX_BIN:-target/debug/animatix}
[ -x "$BIN" ] || { echo "no $BIN — run: cargo build -p animatix-cli"; exit 1; }
fail=0
for scene in "$DIR"/*.amx; do
  if out=$("$BIN" baseline "$scene" --max-frames 8 2>&1); then
    if [ -z "$out" ]; then
        echo "$(basename "$scene"): baseline command produced no output" >&2
        fail=1
    else
        printf '%-14s PASSED\n' "$(basename "$scene" .amx)"
    fi
  else
    rc=$?
    echo "$(basename "$scene"): baseline command failed (exit $rc)" >&2
    fail=1
  fi
done
if [ "$fail" -ne 0 ]; then
  echo "transformer baseline checks FAILED"
  exit 1
fi
echo "transformer baseline checks passed"
