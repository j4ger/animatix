#!/usr/bin/env bash
# Content-sync: the checks that the *content* the site and engine ship still
# agrees with the source that generates or references it.
#
# These are not Rust correctness checks. Each one is a pair of files that must
# agree, where the engine compiles fine and every page renders while the two have
# silently separated — the class of defect no `cargo test` can see, because both
# sides are internally consistent:
#
#   - `web/demos/epicycles/scene.amx` is generated from five act files. A hand edit
#     or an edited act leaves a deployed plate that no source produces.
#   - `examples/lib/*.amx` are embedded into the wasm engine by name in
#     `crates/animatix-web/src/host.rs`. A library added to the directory and not
#     to that list builds in the desktop app and 404s in the browser.
#   - The font SHA-256 table exists twice (`scripts/refresh-fonts.sh` and the
#     vendored-assets README). Two tables means the integrity check verifies
#     hashes nobody reads.
#   - `web/**/lib/*.amx` are the servable copies the embed fetches over HTTP
#     (imports resolve relative to the scene URL, so the files have to exist next
#     to the page). A copy that drifts from `examples/lib/<name>.amx` without being
#     declared forked means the desktop and the browser run different components.
#   - `web/demos/posters/` ships PNGs no page references: bytes deployed to every
#     visitor for a picture nothing draws.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

FAILED=0
fail() { echo "FAIL: $*" >&2; FAILED=1; }
ok() { echo "ok: $*"; }

# ── 1. generated plates match their sources ───────────────────────────────────
if python3 scripts/epicycles-plate.py --check >/dev/null 2>&1; then
  ok "web/demos/epicycles/scene.amx is what epicycles-plate.py generates"
else
  fail "web/demos/epicycles/scene.amx is stale — run: python3 scripts/epicycles-plate.py"
  python3 scripts/epicycles-plate.py --check 2>&1 | sed 's/^/       /' >&2 || true
fi

# ── 2. examples/lib ↔ the engine's bundled library ────────────────────────────
# The wasm player has no filesystem: `bundled_library()` is how an example's
# `import "../lib/theme.amx"` resolves in a browser. Files in the directory but
# not in the list are invisible to the embed only.
HOST_RS=crates/animatix-web/src/host.rs
bundled_keys=$(grep -oE '"\.\./lib/[A-Za-z0-9_.-]+\.amx"' "$HOST_RS" | sed 's|"\.\./lib/||; s/"$//' | sort -u)
dir_libs=$(ls examples/lib/*.amx 2>/dev/null | xargs -n1 basename | sort -u)
missing=$(comm -23 <(printf '%s\n' "$dir_libs") <(printf '%s\n' "$bundled_keys") | grep . || true)
if [ -n "$missing" ]; then
  for f in $missing; do
    fail "examples/lib/$f is not in bundled_library() — the desktop app builds scenes that import it, the web player cannot"
  done
fi
absent=$(comm -13 <(printf '%s\n' "$dir_libs") <(printf '%s\n' "$bundled_keys") | grep . || true)
if [ -n "$absent" ]; then
  for f in $absent; do
    fail "bundled_library() lists ../lib/$f but examples/lib/$f does not exist (the include_str! would not compile, so this list and the manifest are disagreeing)"
  done
fi
if [ -z "$missing" ] && [ -z "$absent" ]; then
  ok "bundled_library() covers all $(printf '%s\n' "$dir_libs" | wc -l) files in examples/lib"
fi

# ── 3. one font hash table ────────────────────────────────────────────────────
FONTS_README=crates/animatix-text/assets/fonts/README.md
script_hashes=$(sed -nE 's/^\s*\["([^"]+)"\]="([0-9a-f]{64})".*/\1 \2/p' scripts/refresh-fonts.sh | sort)
readme_hashes=$(sed -nE 's/^\s*\|\s*([A-Za-z0-9_.-]+\.ttf)\s*\|\s*`([0-9a-f]{64})`\s*\|.*/\1 \2/p' "$FONTS_README" | sort)
if [ -z "$script_hashes" ]; then
  fail "could not read the hash table out of scripts/refresh-fonts.sh — the format changed, update this check"
elif [ -z "$readme_hashes" ]; then
  fail "could not read the hash table out of $FONTS_README — the format changed, update this check"
elif [ "$script_hashes" = "$readme_hashes" ]; then
  ok "font SHA-256 table agrees between refresh-fonts.sh and assets/fonts/README.md ($(printf '%s\n' "$script_hashes" | wc -l) faces)"
else
  fail "font SHA-256 tables disagree:"
  diff <(printf '%s\n' "$script_hashes") <(printf '%s\n' "$readme_hashes") | sed 's/^/       /' >&2 || true
fi

# ── 4. web lib copies vs their source ─────────────────────────────────────────
# Deliberate forks, with the reason. Anything identical-shaped that differs and
# is not listed here is drift, not a fork.
FORKS='web/demos/lib/theme.amx web/demos/transformer/lib/components.amx'
forked() { local w; for w in $FORKS; do [ "$w" = "$1" ] && return 0; done; return 1; }
checked_copies=0
while read -r f; do
  name=$(basename "$f")
  src="examples/lib/$name"
  [ -f "$src" ] || continue
  if cmp -s "$f" "$src"; then
    checked_copies=$((checked_copies + 1))
    continue
  fi
  if ! forked "$f"; then
    fail "$f differs from $src and is not a declared fork — either sync it or add it to FORKS in scripts/content-sync.sh with a reason"
  fi
done < <(find web -path '*/lib/*.amx' | sort)
if [ "$FAILED" -eq 0 ]; then
  ok "$checked_copies servable lib copies byte-identical to examples/lib; $(printf '%s\n' $FORKS | wc -l) declared forks"
fi

# ── 5. shipped-but-unreferenced assets ────────────────────────────────────────
if [ -d web/demos/posters ]; then
  orphans=()
  for png in web/demos/posters/*.png; do
    [ -e "$png" ] || continue
    base=$(basename "$png")
    if ! grep -rqF "$base" web --include='*.html' --include='*.js' --include='*.css' --include='*.amx' --include='*.json'; then
      orphans+=("$base")
    fi
  done
  if [ "${#orphans[@]}" -gt 0 ]; then
    fail "web/demos/posters/ ships ${#orphans[@]} PNG(s) no page references: ${orphans[*]}"
    echo "       The demo hub plays live \`amx-player[data-hoverplay]\` embeds instead of" >&2
    echo "       stills, so these are bytes every visitor downloads for a picture" >&2
    echo "       nothing draws. Wire them into a page, or remove the directory." >&2
  else
    ok "every web/demos/posters/*.png is referenced by a page"
  fi
fi

# ── 6. the size numbers the README quotes ─────────────────────────────────────
# `web/README.md` quotes bundle sizes because they are the first thing someone
# embedding the player wants. They are true of a build, and builds are gitignored,
# so the only honest form of this check measures them against a bundle when one
# exists and says plainly when it cannot — a silently skipped gate is a gate that
# passes.
for spec in "slim:--slim" "full:full profile"; do
  label="${spec%%:*}"
  marker="${spec#*:}"
  # A `case`, not `[ … ] && x=`: under `set -e` a false test in a command
  # substitution kills the whole script mid-run, which is how this check silently
  # stopped reporting the second of its two profiles.
  case "$label" in
    slim) d="web/pkg-slim" ;;
    *) d="web/pkg" ;;
  esac
  wasm="$d/animatix_web_bg.wasm"
  if [ ! -f "$wasm" ]; then
    echo "SKIP: README '$label' size claim not checked — no $wasm. Build it with: scripts/ci.sh gate web-build"
    continue
  fi
  line=$(grep -n -- "$marker" web/README.md | head -1 | cut -d: -f1)
  [ -n "$line" ] || { echo "SKIP: README '$label' size claim not checked (marker '$marker' not found)"; continue; }
  claim=$(sed -n "${line}p" web/README.md | grep -oE '~[0-9.]+ MB raw' | grep -oE '[0-9.]+' | head -1)
  [ -n "$claim" ] || { echo "SKIP: README line $line carries no 'MB raw' figure for $label"; continue; }
  actual=$(python3 -c "import os;print('%.2f' % (os.path.getsize('$wasm')/1048576))")
  drift=$(python3 -c "print('%.1f' % (abs($actual-$claim)/$claim*100))")
  if python3 -c "import sys; sys.exit(0 if float('$drift') <= 10 else 1)"; then
    ok "README quotes $claim MB for $label, the bundle is $actual MB (${drift}% off)"
  else
    fail "README line $line says $label is ~$claim MB raw; the built bundle is $actual MB (${drift}% off) — update web/README.md from scripts/build-web.sh's own output"
  fi
done

if [ "$FAILED" -ne 0 ]; then
  echo "=== content-sync FAILED ===" >&2
  exit 1
fi
echo "=== content-sync passed ==="

