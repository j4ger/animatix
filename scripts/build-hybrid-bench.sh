#!/usr/bin/env bash
# Build the hybrid-vs-compute benchmark wasm and its JS glue into
# web/dev/pkg-hybrid-bench/. Dev-only; not part of any shipped artifact.
#
# Usage: scripts/build-hybrid-bench.sh [--dev]
#   --dev   debug build (faster iterate, larger wasm, no wasm-opt/brotli)
#
# Prereqs: the .#web-build shell (wasm32 std + the pinned wasm-bindgen), the
# same toolchain scripts/build-web.sh requires.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

profile_args=(--profile wasm-release)
out_profile="wasm-release"
dev=0
for arg in "$@"; do
  case "$arg" in
    --dev) dev=1; profile_args=(); out_profile="debug" ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done

# wasm has no auto-vectorization without the explicit target feature; vello_hybrid's
# fearless_simd wants SIMD128, and every browser that can run this demo has it.
cargo_env=()
if [ "$dev" = 0 ]; then
  cargo_env=(env RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+simd128")
fi

if ! ls "$(rustc --print sysroot)/lib/rustlib/wasm32-unknown-unknown" >/dev/null 2>&1; then
  echo "error: this rustc has no wasm32-unknown-unknown std — run inside  nix develop .#web-build" >&2
  exit 1
fi

# shellcheck source=scripts/wasm-tools.sh
. scripts/wasm-tools.sh
check_wbg

"${cargo_env[@]}" cargo build -p animatix-hybrid-bench --target wasm32-unknown-unknown "${profile_args[@]}"

out_dir="web/dev/pkg-hybrid-bench"
stage_dir="target/hybrid-bench-build"
rm -rf "$stage_dir"
mkdir -p "$stage_dir"
cleanup() { if [ -d "$stage_dir" ]; then rm -rf "$stage_dir"; fi; }
trap cleanup EXIT

wasm-bindgen --target web --out-dir "$stage_dir" \
  "target/wasm32-unknown-unknown/$out_profile/animatix_hybrid_bench.wasm"

if [ "$dev" = 0 ]; then
  if command -v wasm-opt >/dev/null 2>&1; then
    wasm-opt -Oz --strip-debug -o "$stage_dir/animatix_hybrid_bench_bg.wasm" "$stage_dir/animatix_hybrid_bench_bg.wasm"
  else
    echo "warning: wasm-opt unavailable — serving the unoptimized module" >&2
  fi
  if command -v brotli >/dev/null 2>&1; then
    for f in "$stage_dir/animatix_hybrid_bench_bg.wasm" "$stage_dir/animatix_hybrid_bench.js"; do
      [ -f "$f" ] && brotli -q "${ANIMATIX_BROTLI_LEVEL:-6}" -f -k "$f"
    done
  fi
fi

rm -rf "$out_dir"
mkdir -p "$(dirname "$out_dir")"
mv "$stage_dir" "$out_dir"
echo "built $out_dir"
