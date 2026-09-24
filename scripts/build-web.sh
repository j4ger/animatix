#!/usr/bin/env bash
# Build the animatix-web wasm module and generate JS glue into web/pkg.
#
# Usage: scripts/build-web.sh [--dev]
#   (default) release build — what you serve/deploy
#   --dev     debug build    — faster iterate, larger wasm, no opt
#
# Prereqs: rustup target wasm32-unknown-unknown, wasm-bindgen-cli matching the
# `wasm-bindgen` version pinned in crates/animatix-web/Cargo.toml.
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

echo "wasm ready: web/pkg/"
