#!/usr/bin/env bash
# Source contracts retained after the old public HTML/manifest was retired.
# These check generated AMX, actual import closure, font provenance and SDK
# integrity. Current website contracts live with Astro, not a second generator.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

FAILED=0
fail() { echo "FAIL: $*" >&2; FAILED=1; }
ok() { echo "ok: $*"; }

# Gradient math, parameter bounds and opt-out wiring must stay consistent.
if python3 tools/content/check-gradient-content.py; then
  ok "gradient math and parameter contracts"
else
  fail "gradient content contract mismatch"
fi

# Gradient uses the same single-source assembly policy as Epicycles.
if python3 tools/content/gradient-plate.py --check; then
  ok "gradient plate matches canonical acts"
else
  fail "gradient plate is stale"
fi

# ── 1. generated plates match their sources ───────────────────────────────────
if python3 tools/content/epicycles-plate.py --check >/dev/null 2>&1; then
  ok "web/demos/epicycles/scene.amx is what epicycles-plate.py generates"
else
  fail "web/demos/epicycles/scene.amx is stale — run: python3 tools/content/epicycles-plate.py"
  python3 tools/content/epicycles-plate.py --check 2>&1 | sed 's/^/       /' >&2 || true
fi

# Scene imports are real source dependencies, not an embedded engine catalog.
python3 tools/content/check-scene-imports.py || fail "scene import graph is incomplete"
python3 tools/content/test-check-scene-imports.py || fail "scene import checker regression"
python3 web/tools/site/test-site-source-contracts.py || fail "site source contract regression"

# ── 3. one font hash table ────────────────────────────────────────────────────
FONTS_README=crates/animatix-text/assets/fonts/README.md
script_hashes=$(sed -nE 's/^\s*\["([^"]+)"\]="([0-9a-f]{64})".*/\1 \2/p' web/tools/build/refresh-fonts.sh | sort)
readme_hashes=$(sed -nE 's/^\s*\|\s*([A-Za-z0-9_.-]+\.ttf)\s*\|\s*`([0-9a-f]{64})`\s*\|.*/\1 \2/p' "$FONTS_README" | sort)
if [ -z "$script_hashes" ]; then
  fail "could not read the hash table out of web/tools/build/refresh-fonts.sh — the format changed, update this check"
elif [ -z "$readme_hashes" ]; then
  fail "could not read the hash table out of $FONTS_README — the format changed, update this check"
elif [ "$script_hashes" = "$readme_hashes" ]; then
  ok "font SHA-256 table agrees between refresh-fonts.sh and assets/fonts/README.md ($(printf '%s\n' "$script_hashes" | wc -l) faces)"
else
  fail "font SHA-256 tables disagree:"
  diff <(printf '%s\n' "$script_hashes") <(printf '%s\n' "$readme_hashes") | sed 's/^/       /' >&2 || true
fi

# ── 6. SDK size is reported directly from its verified manifest ───────────────
if [ -f web/runtime/runtime-manifest.json ]; then
  node web/tools/sdk.mjs verify web/runtime --current >/dev/null || fail "web SDK is stale or invalid"
else
  echo "SKIP: SDK size report — build web/runtime with tools/ci/ci.sh gate web-sdk"
fi

if [ "$FAILED" -ne 0 ]; then
  echo "=== content-sync FAILED ===" >&2
  exit 1
fi
echo "=== content-sync passed ==="

