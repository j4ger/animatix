#!/usr/bin/env bash
# scripts/check-docs.sh
#
# CI-friendly documentation consistency check:
#   - every relative Markdown link under docs/ must resolve to an existing file
#   - docs/roadmap.md must not keep completed-status rows under Active Work
#   - active docs must not carry stale known-gap wording for completed roadmap items
#
# Usage:
#   bash scripts/check-docs.sh
#
# Exit codes:
#   0  all checks passed
#   1  one or more checks failed

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FAILED=0

echo "========================================"
echo "Animatix documentation consistency check"
echo "========================================"

# ── 1. Relative Markdown links ───────────────────────────────────────────────
echo ""
echo "1. Relative Markdown links under docs/"
echo "----------------------------------------"
LINK_COUNT=0
while IFS= read -r file; do
    [ -f "$file" ] || continue
    while IFS= read -r link; do
        [ -n "$link" ] || continue
        LINK_COUNT=$((LINK_COUNT + 1))

        target="${link#*](}"
        target="${target%)}"
        target="${target%%#*}"
        target="${target%%\?*}"

        case "$target" in
            http://* | https://* | mailto:* | '#'* | '')
                continue
                ;;
        esac

        target="${target#<}"
        target="${target%>}"

        if (cd "$(dirname "$file")" && [ ! -e "$target" ]); then
            echo "  MISSING LINK: $file -> $target"
            FAILED=1
        fi
    done < <(grep -oE '\[[^]]+\]\([^)]+\)' "$file" || true)
done < <(find "$ROOT/docs" -name '*.md' -type f | sort)

if [ "$FAILED" -eq 0 ]; then
    echo "  Checked $LINK_COUNT Markdown link(s): PASSED"
else
    echo "  Checked $LINK_COUNT Markdown link(s): FAILED"
fi

# ── 2. Roadmap completed-status rows ─────────────────────────────────────────
echo ""
echo "2. Roadmap Active Work completed-status rows"
echo "-------------------------------------------"
COMPLETED_ROWS="$ROOT/docs/roadmap.md"
if grep -nE '^\|.*\| (Done|Completed|Resolved|Shipped) \|' "$COMPLETED_ROWS"; then
    echo "  FAILED: completed rows should be removed from Active Work"
    FAILED=1
else
    echo "  PASSED"
fi

# ── 3. Stale known-gap wording for completed roadmap items ───────────────────
echo ""
echo "3. Stale known-gap wording"
echo "--------------------------"
STALE_PATTERNS=(
    'Svg\.url.*immediate/static'
    'Svg\.url.*immediately/static'
    'Svg\.url.*not timed'
    'not source-parseable yet'
    'Colorscheme dotted token parser'
    'Verify `Theme::light\(\)` contrast'
    'Verify the light-theme contrast'
    'Multi-target invocation.*not supported'
)

ACTIVE_DOCS=(
    "$ROOT/docs/roadmap.md"
    "$ROOT/docs/spec.md"
    "$ROOT/docs/properties.md"
    "$ROOT/docs/primitives.md"
    "$ROOT/docs/gui_design_language.md"
)

STALE_FOUND=0
for file in "${ACTIVE_DOCS[@]}"; do
    for pattern in "${STALE_PATTERNS[@]}"; do
        if grep -nE "$pattern" "$file" 2>/dev/null; then
            echo "  STALE STATUS: $file matches '$pattern'"
            STALE_FOUND=1
            FAILED=1
        fi
    done
done

if [ "$STALE_FOUND" -eq 0 ]; then
    echo "  PASSED"
fi

# ── Summary ──────────────────────────────────────────────────────────────────
echo ""
echo "========================================"
echo "Summary"
echo "========================================"
if [ "$FAILED" -eq 0 ]; then
    echo "DOCS CHECK PASSED"
    exit 0
fi

echo "DOCS CHECK FAILED"
echo ""
echo "Fix broken relative links, remove completed Active Work rows, or update"
echo "known-gap wording when a roadmap item ships."
exit 1
