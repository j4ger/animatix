#!/usr/bin/env bash
# scripts/check-docs.sh
#
# CI-friendly documentation consistency check:
#   - every relative Markdown link under docs/ must resolve to an existing file
#   - docs/roadmap.md must not keep completed-status rows in any table
#   - active docs must not carry stale known-gap wording for completed roadmap items
# Generated property cells are verified by the doc gate, not parsed from Rust.
#   - code spans that name a repo path (`crates/…`, `scripts/…`, `web/…`) must resolve
#   - active docs must cite symbols, not `file.rs:NNN` line numbers
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
echo "2. Roadmap completed-status rows (all tables)"
echo "--------------------------------------------"
ROADMAP="$ROOT/docs/roadmap.md"
# Scan every table cell in the file, not one section or one status column:
# finished rows belong in docs/history/, wherever they were left behind. A cell
# is rejected when it starts with Done/Completed/Resolved/Shipped/Closed even
# when followed by a note ("Closed, not worth it").
COMPLETED_ROWS=$(awk -F'|' '
    function trim(s) { gsub(/^[[:space:]]+|[[:space:]]+$/, "", s); return s }
    /^\|/ {
        for (i = 2; i < NF; i++) {
            cell = tolower(trim($i));
            if (cell ~ /^(done|completed|resolved|shipped|closed)([^a-z]|$)/) {
                print NR ": " $0;
                break;
            }
        }
    }
' "$ROADMAP")
if [ -n "$COMPLETED_ROWS" ]; then
    echo "$COMPLETED_ROWS"
    echo "  FAILED: completed rows must move to docs/history/"
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

# ── 4. Code-span repo paths ──────────────────────────────────────────────────
# A path in backticks is navigation: if it does not resolve, the reader follows
# a dead end. Only repo-root paths are checked (a bare `utils.rs` is ambiguous
# prose shorthand); commands, placeholders and globs are skipped.
echo ""
echo "4. Code-span repo paths"
echo "-----------------------"
if python3 "$ROOT/scripts/check-doc-paths.py" "$ROOT"; then
    echo "  PASSED"
else
    FAILED=1
fi

# ── 5. Line-number citations ─────────────────────────────────────────────────
# A `file.rs:123` citation is stale the moment the file is edited, and these
# have already pointed at the wrong line repeatedly. Cite a symbol instead.
# History pages are exempt: they record what a file looked like at the time.
echo ""
echo "5. Line-number citations"
echo "-----------------------"
LINE_FAILED=0
while IFS= read -r file; do
    [ -f "$file" ] || continue
    if matches=$(grep -nE '`[^`]+\.rs:[0-9]+`' "$file"); then
        echo "  LINE-NUMBER CITATION FOUND: $file"
        printf '%s\n' "$matches"
        LINE_FAILED=1
    fi
done < <(find "$ROOT/docs" -name '*.md' -type f -not -path "*/history/*" | sort)
if [ "$LINE_FAILED" -eq 0 ]; then
    echo "  PASSED"
else
    FAILED=1
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
echo "Fix broken relative links, remove completed roadmap rows in any table,"
echo "update known-gap wording when a roadmap item ships, correct a documented"
echo "Applies-to actor list, repair a stale repo path, or replace a line-number"
echo "citation with a symbol reference."
exit 1
