#!/usr/bin/env bash
# Build the animatix-web wasm module, generate JS glue, optimize, and
# precompress it into web/pkg (or web/pkg-slim).
#
# Usage: scripts/build-web.sh [--slim] [--dev]
#   (default) full release build — complete engine feature set (rich text,
#             raster/SVG assets) — what the demos use
#   --slim    playback-only profile: drops the Typst rich-text stack, raster
#             image decoding, and SVG loading (~smaller wasm; Text degrades to
#             the plain fast path, Code/Math/Svg assets report diagnostics)
#   --dev     debug build    — faster iterate, larger wasm, no opt
#
# Release builds use the `wasm-release` profile (see the root Cargo.toml: fat
# LTO, one codegen unit, panic=abort) and enable wasm SIMD128. Neither touches
# the `release` profile the native benchmarks build through.
#
# Prereqs: rustup target wasm32-unknown-unknown, wasm-bindgen-cli matching the
# `wasm-bindgen` version pinned in crates/animatix-web/Cargo.toml.
# wasm-opt (binaryen) and brotli are fetched through `nix shell` when
# available; missing tools skip their step with a warning instead of failing.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

profile_args=(--profile wasm-release)
out_profile="wasm-release"
feature_args=()
out_dir="web/pkg"
for arg in "$@"; do
  case "$arg" in
    --dev) profile_args=(); out_profile="debug" ;;
    --slim) feature_args=(--no-default-features); out_dir="web/pkg-slim" ;;
  esac
done

# wasm has no auto-vectorization without an explicit target feature, and every
# browser that can run the player (WebGPU shipped long after SIMD128) has it.
cargo_env=()
if [ "$out_profile" != "debug" ]; then
  cargo_env=(env RUSTFLAGS="-C target-feature=+simd128")
fi

"${cargo_env[@]}" cargo build -p animatix-web --target wasm32-unknown-unknown "${feature_args[@]}" "${profile_args[@]}"

mkdir -p "$out_dir"
wasm-bindgen --target web --out-dir "$out_dir" \
  "target/wasm32-unknown-unknown/$out_profile/animatix_web.wasm"

# Size-optimize the wasm (binaryen). -Oz aggressively shrinks; the one-shot
# cost is ~a minute, the payoff ~25-35% of the served bytes.
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Oz --strip-debug -o "$out_dir/animatix_web_bg.wasm" "$out_dir/animatix_web_bg.wasm"
elif [ -n "${IN_NIX_SHELL:-}" ] || command -v nix >/dev/null 2>&1; then
  echo "wasm-opt not on PATH; trying nix shell nixpkgs#binaryen..."
  if nix shell nixpkgs#binaryen -c bash -c \
    'wasm-opt -Oz --strip-debug -o "$1" "$2"' _ \
    "$out_dir/animatix_web_bg.wasm" "$out_dir/animatix_web_bg.wasm" 2>/dev/null; then
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
  for f in "$out_dir/animatix_web_bg.wasm" "$out_dir/animatix_web.js"; do
    [ -f "$f" ] && br "$f"
  done
else
  echo "warning: brotli unavailable — skipping precompression"
fi

wasm_bytes=$(wc -c < "$out_dir/animatix_web_bg.wasm")
gz_bytes=$(gzip -9 -c "$out_dir/animatix_web_bg.wasm" | wc -c)
br_bytes=$([ -f "$out_dir/animatix_web_bg.wasm.br" ] && wc -c < "$out_dir/animatix_web_bg.wasm.br" || echo "n/a")
echo "wasm ready: $out_dir/"
echo "  raw:    $wasm_bytes"
echo "  gzip:   $gz_bytes"
echo "  brotli: $br_bytes"
