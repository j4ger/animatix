#!/usr/bin/env bash
# Exercise final archives, including a site using the separately unpacked SDK.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
for tool in node zip unzip; do
  command -v "$tool" >/dev/null || { echo "error: $tool is required for SDK distribution acceptance" >&2; exit 1; }
done
bash scripts/dist-package.sh web-sdk
bash scripts/dist-package.sh site
version="$(node --input-type=module -e 'import {verify} from "./web/tools/sdk.mjs"; console.log(verify("web/runtime").version)')"
label="${ANIMATIX_RELEASE_LABEL:-v$version}"
out=target/web-distribution-acceptance
rm -rf "$out"
mkdir -p "$out/sdk" "$out/site"
unzip -q "dist/animatix-${label}-web-sdk.zip" -d "$out/sdk"
unzip -q "dist/animatix-${label}-site.zip" -d "$out/site"
commit="${ANIMATIX_ARTIFACT_COMMIT:-$(git rev-parse HEAD)}"
node web/tools/sdk.mjs verify "$out/sdk" --release --current --commit "$commit"
node web/tools/sdk.mjs verify "$out/site/runtime" --release --current --commit "$commit"
cmp "$out/sdk/runtime-manifest.json" "$out/site/runtime/runtime-manifest.json"
node scripts/text-parity.mjs "$out/sdk/runtime-manifest.json"
node scripts/web-raster-proof.mjs "$out/sdk"
if find "$out" -type l -print -quit | grep -q .; then
  echo 'error: distribution archive contains a symlink' >&2; exit 1
fi
# This proves both shipping formats work, rather than only the builder symlink.
rm -rf "$out/site/runtime"
cp -R "$out/sdk" "$out/site/runtime"
ANIMATIX_WEB_ROOT="$root/$out/site" node scripts/web-transactions.mjs
