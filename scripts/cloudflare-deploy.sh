#!/usr/bin/env bash
# Only transport an already verified static site. Never edit DNS or build SDKs.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
: "${CLOUDFLARE_API_TOKEN:?set the GitHub Actions secret, not a tracked file}"
: "${CLOUDFLARE_ACCOUNT_ID:?missing account ID}"
: "${CLOUDFLARE_PAGES_PROJECT:?missing Pages project}"
: "${ANIMATIX_ARTIFACT_COMMIT:?missing verified source commit}"
[[ "$CLOUDFLARE_ACCOUNT_ID" =~ ^[0-9a-f]{32}$ ]] || { echo 'invalid Cloudflare account ID' >&2; exit 1; }
[[ "$CLOUDFLARE_PAGES_PROJECT" =~ ^[a-z0-9][a-z0-9-]*$ ]] || { echo 'invalid Pages project name' >&2; exit 1; }
sha="$(git rev-parse HEAD)"
[ "$sha" = "$ANIMATIX_ARTIFACT_COMMIT" ] || { echo 'checkout/CI commit mismatch' >&2; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo 'refuse dirty deployment' >&2; exit 1; }
# Serialized deploys can still finish out of order when CI durations differ.
remote="$(git ls-remote origin refs/heads/main | cut -f1)"
[ "$sha" = "$remote" ] || { echo 'refuse stale deployment: main has advanced; deploy its successful CI instead' >&2; exit 1; }
python3 scripts/cloudflare-site-check.py _site
SITE_BASE=/ python3 scripts/site-check.py _site --assembled
node web/tools/sdk.mjs verify _site/runtime --release --current --commit "$sha"
export WRANGLER_SEND_METRICS=false
# Installed in a separate no-secret step. No unpinned npx download at deployment.
web/deploy/node_modules/.bin/wrangler pages deploy _site \
  --project-name "$CLOUDFLARE_PAGES_PROJECT" --branch main \
  --commit-hash "$sha" --commit-dirty=false
