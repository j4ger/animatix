#!/usr/bin/env bash
# scripts/check-docs.sh
#
# CI-friendly documentation consistency check:
#   - every relative Markdown link under docs/ must resolve to an existing file
#   - docs/roadmap.md must not keep completed-status rows in any table
#   - active docs must not carry stale known-gap wording for completed roadmap items
#   - docs/properties.md Applies-to actor lists must match PROPERTY_DESCRIPTORS
#     for the rows pinned below
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

# ── 4. Property reference applicability ─────────────────────────────────────
echo ""
echo "4. Property reference Applies-to actor lists"
echo "--------------------------------------------"
PROPERTY_DOC="$ROOT/docs/properties.md"
PROPERTY_SOURCE="$ROOT/crates/animatix-core/src/property.rs"
APPLIES_FAILED=0

normalize_actor_list() {
    tr ',' '\n' | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//' | sed '/^$/d' \
        | sort | paste -sd ',' -
}

# Read the actor names from a one-line `Applicable::Actors(&[...])` descriptor row.
descriptor_actor_list() {
    local prop="$1"
    local row
    row=$(grep -F "\"$prop\" => (Applicable::Actors(&[" "$PROPERTY_SOURCE" | head -n 1)
    [ -n "$row" ] || return 1
    printf '%s\n' "$row" \
        | sed 's/.*Applicable::Actors(&\[//; s/\].*//' \
        | grep -o '"[A-Za-z][A-Za-z0-9]*"' \
        | tr -d '"' \
        | normalize_actor_list
}

documented_applies_to() {
    local prop="$1"
    grep -E "^\| \`$prop\` \|" "$PROPERTY_DOC" | head -n 1 | cut -d'|' -f6 \
        | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//'
}

check_actor_row() {
    local prop="$1"
    local expected actual
    if ! expected=$(descriptor_actor_list "$prop"); then
        echo "  MISSING DESCRIPTOR: $prop"
        APPLIES_FAILED=1
        return
    fi
    actual=$(documented_applies_to "$prop" || true)
    if [ "$(printf '%s' "$actual" | normalize_actor_list)" != \
         "$(printf '%s' "$expected" | normalize_actor_list)" ]; then
        echo "  APPLIES-TO MISMATCH: $prop"
        echo "    docs:       $actual"
        echo "    descriptor: $expected"
        APPLIES_FAILED=1
    fi
}

# The explicit actor-list rows that have drifted before. Keep this list growing
# when a capability-shaped row is rewritten as an `Applicable::Actors` list.
for prop in from to corner_radius padding x_range y_range; do
    check_actor_row "$prop"
done

# `head_size` is an `Any` row: an Arrow shape kind plus the Callout capability,
# so its documentation is a two-actor list rather than one `Actors` array.
HEAD_SIZE_ROW=$(grep -F '"head_size" => (Applicable::Any' -A4 "$PROPERTY_SOURCE" || true)
if ! grep -q 'Applicable::ShapeKinds(&\[ShapeKind::Arrow\])' <<<"$HEAD_SIZE_ROW" \
    || ! grep -q 'Applicable::Callout' <<<"$HEAD_SIZE_ROW"; then
    echo "  MISSING DESCRIPTOR: head_size (Any row changed shape)"
    APPLIES_FAILED=1
else
    actual=$(documented_applies_to head_size || true)
    if [ "$(printf '%s' "$actual" | normalize_actor_list)" != \
         "$(printf '%s' 'Arrow, Callout' | normalize_actor_list)" ]; then
        echo "  APPLIES-TO MISMATCH: head_size"
        echo "    docs:       $actual"
        echo "    descriptor: Arrow, Callout"
        APPLIES_FAILED=1
    fi
fi

if [ "$APPLIES_FAILED" -eq 0 ]; then
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
echo "update known-gap wording when a roadmap item ships, or correct a"
echo "documented Applies-to actor list."
exit 1
