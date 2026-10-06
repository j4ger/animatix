#!/usr/bin/env bash
# Keyframe acceptance for the transformer walkthrough's seven scenes.
#
# Each scene carries `<name>.verify.txt`: a `differs` floor per beat (every
# keyframe must change the picture), `visible`/`invisible` pairs at the cue of
# every actor whose region the engine can observe, and an `ink` floor on the
# frame the web player holds between loops (the finished figure, not an empty
# plate). Floors were derived from the rendered frames, and the ink check was
# confirmed to fail when an outro erases the composition again.
#
# Needs a GPU like every render gate, so it is a local/scripted gate rather
# than part of `cargo test`. Usage: scripts/transformer-verify.sh
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"
DIR=web/demos/transformer/scenes
BIN=${ANIMATIX_BIN:-target/debug/animatix}
[ -x "$BIN" ] || { echo "no $BIN — run: cargo build -p animatix-cli"; exit 1; }
fail=0
for scene in "$DIR"/*.amx; do
  checks="${scene%.amx}.verify.txt"
  [ -f "$checks" ] || { echo "$(basename "$scene"): no checks file"; fail=1; continue; }
  out=$("$BIN" verify "$scene" --checks "$checks" 2>/dev/null)
  line=$(printf '%s\n' "$out" | tail -1)
  printf '%-14s %s\n' "$(basename "$scene" .amx)" "$line"
  if printf '%s\n' "$out" | grep -q "FAIL"; then fail=1; fi
done
if [ "$fail" -ne 0 ]; then
  echo "transformer keyframe checks FAILED"
  "$BIN" verify "$DIR/tokens.amx" --checks "$DIR/tokens.verify.txt" 2>/dev/null | grep FAIL
  exit 1
fi
echo "transformer keyframe checks passed"
