#!/usr/bin/env bash
# Ensure the single SDK matches all source/tool inputs. --check is read-only.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
check=false
force=false
profile=release
for arg in "$@"; do
  case "$arg" in
    --check) check=true ;;
    --force) force=true ;;
    --dev) profile=dev ;;
    *) echo 'usage: ensure-web-pkg.sh [--check] [--force] [--dev]' >&2; exit 2 ;;
  esac
done
if ! command -v rustc >/dev/null || ! command -v wasm-bindgen >/dev/null || \
   [ ! -d "$(rustc --print sysroot)/lib/rustlib/wasm32-unknown-unknown" ]; then
  exec nix develop .#web-build --command bash scripts/ensure-web-pkg.sh "$@"
fi
if [ "$force" = false ] && [ -f web/runtime/runtime-manifest.json ]; then
  expected=$(node web/tools/sdk.mjs identity "$profile")
  if SDK_EXPECTED="$expected" node --input-type=module -e '
    import {verify} from "./web/tools/sdk.mjs";
    const actual=verify("web/runtime",{current:true});
    if(actual.build_id!==JSON.parse(process.env.SDK_EXPECTED).build_id)process.exit(1);
  '; then
    echo 'Web SDK is current.'
    exit 0
  fi
fi
if [ "$check" = true ]; then echo 'error: Web SDK is missing or stale' >&2; exit 1; fi
args=()
if [ "$profile" = dev ]; then args+=(--dev); fi
exec bash scripts/build-web.sh "${args[@]}"
