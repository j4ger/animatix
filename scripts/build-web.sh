#!/usr/bin/env bash
# Build the animatix-web wasm module, generate JS glue, optimize, precompress,
# and copy demo examples into web/pkg.
#
# Usage: scripts/build-web.sh [--dev]
#   (default) release build — what you serve/deploy
#   --dev     debug build    — faster iterate, larger wasm, no opt
#
# Prereqs: rustup target wasm32-unknown-unknown, wasm-bindgen-cli matching the
# `wasm-bindgen` version pinned in crates/animatix-web/Cargo.toml.
# wasm-opt (binaryen) and brotli are fetched through `nix shell` when
# available; missing tools skip their step with a warning instead of failing.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

profile_args=(--release)
out_profile="release"
if [ "${1:-}" = "--dev" ]; then
  profile_args=()
  out_profile="debug"
fi

cargo build -p animatix-web --target wasm32-unknown-unknown "${profile_args[@]}"

mkdir -p web/pkg
wasm-bindgen --target web --out-dir web/pkg \
  "target/wasm32-unknown-unknown/$out_profile/animatix_web.wasm"

# Size-optimize the wasm (binaryen). -Oz aggressively shrinks; the one-shot
# cost is ~a minute, the payoff ~25-35% of the served bytes.
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Oz --strip-debug -o web/pkg/animatix_web_bg.wasm web/pkg/animatix_web_bg.wasm
elif [ -n "${IN_NIX_SHELL:-}" ] || command -v nix >/dev/null 2>&1; then
  echo "wasm-opt not on PATH; trying nix shell nixpkgs#binaryen..."
  if nix shell nixpkgs#binaryen -c bash -c \
    'wasm-opt -Oz --strip-debug -o "$1" "$2"' _ \
    web/pkg/animatix_web_bg.wasm web/pkg/animatix_web_bg.wasm 2>/dev/null; then
    :
  else
    echo "warning: wasm-opt unavailable — serving the unoptimized module"
  fi
else
  echo "warning: wasm-opt unavailable — serving the unoptimized module"
fi

# Precompress the served artifacts (level 11 brotli). A static host or CDN
# serves the `.br` twin when the client advertises brotli; scripts/serve-web.py
# negotiates them locally to prove the setup end to end.
if command -v brotli >/dev/null 2>&1; then
  brotli_avail=true
elif command -v nix >/dev/null 2>&1; then
  brotli_avail=false
  if nix shell nixpkgs#brotli -c bash -c 'brotli --version' >/dev/null 2>&1; then
    BROTLI="nix shell nixpkgs#brotli -c brotli"
    brotli_avail=true
  fi
fi

if [ "${brotli_avail:-false}" = true ]; then
  br() { if [ -n "${BROTLI:-}" ]; then $BROTLI -q 11 -f -k "$1"; else brotli -q 11 -f -k "$1"; fi; }
  for f in web/pkg/animatix_web_bg.wasm web/pkg/animatix_web.js \
           web/vendor/cm.js web/css/main.css web/js/main.js web/index.html; do
    [ -f "$f" ] && br "$f"
  done
else
  echo "warning: brotli unavailable — skipping precompression"
fi

# Demo scenes: copied from the repo's examples so the shown sources are
# exactly the tracked ones (the examples' lib/ imports are embedded in the
# wasm itself, see crates/animatix-web/src/host.rs).
example_dir="web/pkg/examples"
mkdir -p "$example_dir"
copy_example() { cp "examples/$1" "$example_dir/$2"; }
copy_example "basics/00_hello.amx"        "hello.amx"
copy_example "animation/04_motion.amx"    "motion.amx"
copy_example "basics/22_expressions.amx"  "expressions.amx"
copy_example "layout/11_colors.amx"       "colors.amx"
copy_example "basics/31_code.amx"         "code.amx"
copy_example "data/07_plots.amx"          "plots.amx"
copy_example "generation/15_for_loop.amx" "for_loop.amx"
copy_example "composition/14_multiscene.amx" "multiscene.amx"
copy_example "gallery/sorting_theatre.amx"   "sorting_theatre.amx"
copy_example "gallery/epicycles.amx"         "epicycles.amx"

wasm_bytes=$(wc -c < web/pkg/animatix_web_bg.wasm)
gz_bytes=$(gzip -9 -c web/pkg/animatix_web_bg.wasm | wc -c)
br_bytes=$([ -f web/pkg/animatix_web_bg.wasm.br ] && wc -c < web/pkg/animatix_web_bg.wasm.br || echo "n/a")
echo "wasm ready: web/pkg/"
echo "  raw:    $wasm_bytes"
echo "  gzip:   $gz_bytes"
echo "  brotli: $br_bytes"
