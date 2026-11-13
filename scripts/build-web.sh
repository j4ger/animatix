#!/usr/bin/env bash
# Build one versioned SDK: one linked program, shared-memory cold partitions,
# external font faces, and a content/ABI manifest. No public feature profiles.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
profile=release
for arg in "$@"; do
  case "$arg" in --dev) profile=dev ;; *) echo "usage: build-web.sh [--dev]" >&2; exit 2 ;; esac
done
if ! command -v rustc >/dev/null || [ ! -d "$(rustc --print sysroot)/lib/rustlib/wasm32-unknown-unknown" ]; then
  echo 'error: build in nix develop .#web-build (wasm32 std is required)' >&2
  exit 1
fi
. scripts/wasm-tools.sh
check_wbg
command -v wasm-opt >/dev/null || { echo 'error: wasm-opt is required for the SDK build' >&2; exit 1; }
command -v zopfli >/dev/null || { echo 'error: zopfli is required for base Wasm gzip compression' >&2; exit 1; }
# Resolve dependencies before fingerprinting Cargo.lock; build itself is locked.
cargo metadata --format-version 1 --no-deps --locked >/dev/null
cargo build --locked -p animatix-web-build
npm --prefix web/tools ci --silent
npm --prefix web/tools run build:embed
stage="target/web-build/sdk-stage-$$"
mkdir -p "$stage/bindgen" "$stage/sdk"
trap 'rm -rf "$stage"' EXIT
node web/tools/sdk.mjs identity "$profile" > "$stage/identity.json"
identity=$(node -e 'process.stdout.write(JSON.parse(require("fs").readFileSync(process.argv[1])).build_id)' "$stage/identity.json")
commit=$(git rev-parse HEAD)
cargo_profile=wasm-release
if [ "$profile" = dev ]; then cargo_profile=wasm-dev; fi
env ANIMATIX_RUNTIME_BUILD_ID="$identity" ANIMATIX_GIT_SHA="$commit" \
  RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+simd128 -C link-arg=--emit-relocs -C debuginfo=2" \
  cargo build --locked -p animatix-web --target wasm32-unknown-unknown --profile "$cargo_profile"
original="target/wasm32-unknown-unknown/$cargo_profile/animatix_web.wasm"
wasm-bindgen --no-demangle --target web --out-dir "$stage/bindgen" "$original"
target/debug/animatix-web-build split "$original" "$stage/bindgen" "$stage/sdk"
if [ "$profile" = release ]; then
  for module in "$stage/sdk/"*.wasm; do wasm-opt -Oz --strip-debug -o "$module" "$module"; done
fi
target/debug/animatix-web-build fonts crates/animatix-text/assets/fonts > "$stage/fonts.json"
node web/tools/sdk.mjs assemble "$stage/sdk" "$stage/fonts.json" "$stage/identity.json"
node web/tools/sdk.mjs verify "$stage/sdk" --current
# Install immutable builds and atomically replace only the public symlink.
# Hashed module URLs in the manifest prevent a previously loaded page from
# combining a new capability module with an old shared function table.
release="target/web-build/releases/${identity}-$$"
mkdir -p target/web-build/releases
mv "$stage/sdk" "$release"
next="web/.runtime-next-$$"
ln -s "../$release" "$next"
if [ -e web/runtime ] && [ ! -L web/runtime ]; then
  echo 'error: web/runtime must be a generated symlink; refusing to replace a directory' >&2
  rm "$next"
  exit 1
fi
node -e 'require("fs").renameSync(process.argv[1], "web/runtime")' "$next"
node web/tools/sdk.mjs verify web/runtime
