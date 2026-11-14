#!/usr/bin/env bash
# Static UI only: does not build or rewrite the independently verified SDK.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
export ASTRO_TELEMETRY_DISABLED=1
node --input-type=module -e 'const [a,b,c]=process.versions.node.split(".").map(Number); if(a<22 || (a===22 && (b<23 || (b===23&&c<4)))) throw Error("site requires Node >=22.23.4; enter the pinned Nix shell")'
python3 scripts/site-check-test.py
npm --prefix web/app ci --no-audit --no-fund
npm --prefix web/app run check
npm --prefix web/app test
npm --prefix web/app run build
python3 scripts/site-check.py web/app/dist --write-manifest

# Exercise project Pages paths in every site gate, without replacing the chosen
# deployment output. This is static path validation, not a browser GPU test.
# Astro moves prerender assets with rename(), so the scratch output must
# share a filesystem with the checkout (Nix TMPDIR may be a different mount).
mkdir -p target
base_output="$(mktemp -d "$root/target/site-base.XXXXXX")"
trap 'rm -rf "$base_output"' EXIT
SITE_BASE=/animatix/ npm --prefix web/app run build -- --outDir "$base_output"
SITE_BASE=/animatix/ python3 scripts/site-check.py "$base_output"
