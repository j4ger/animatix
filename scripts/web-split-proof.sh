#!/usr/bin/env bash
# Real optimized wasm boundary test, including failed fetch recovery and the
# shared allocator/function table; this is not a mock of the splitter.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
. scripts/wasm-tools.sh
check_wbg
out=target/web-split-proof
mkdir -p "$out/bindgen" "$out/sdk"
cargo build --locked -p animatix-web-build
cargo test --locked -p wasm-split-cli --lib
cargo test --locked -p animatix-web-build
env RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+simd128 -C link-arg=--emit-relocs -C debuginfo=2" \
  cargo build --locked -p animatix-split-probe --target wasm32-unknown-unknown --profile wasm-release
original=target/wasm32-unknown-unknown/wasm-release/animatix_split_probe.wasm
wasm-bindgen --no-demangle --target web --out-name animatix_web --out-dir "$out/bindgen" "$original"
target/debug/animatix-web-build split "$original" "$out/bindgen" "$out/sdk"
for file in "$out/sdk/"*.wasm; do wasm-opt -Oz --strip-debug -o "$file" "$file"; done
node --input-type=module - "$out/sdk" <<'JS'
import {readdirSync,readFileSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {gzipSync} from 'node:zlib';
const dir=process.argv[2];
for(const file of readdirSync(dir).filter(file=>file.endsWith('.wasm')))writeFileSync(join(dir,file+'.gz'),gzipSync(readFileSync(join(dir,file))));
writeFileSync(join(dir,'package.json'),JSON.stringify({type:'module'}));
JS
node web/tools/split-proof.mjs "$out/sdk"
