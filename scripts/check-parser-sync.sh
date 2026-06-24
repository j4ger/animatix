#!/usr/bin/env bash
# scripts/check-parser-sync.sh
#
# CI-friendly sync check: run both the Chumsky PEG parser (via cargo test)
# and the tree-sitter grammar over every examples/*.amx file and report
# if either fails to parse.
#
# Usage:
#   bash scripts/check-parser-sync.sh
#
# Requirements:
#   - cargo   (Rust toolchain, for PEG parser tests)
#   - tree-sitter CLI (for grammar parse checks)
#     Install:  npm install -g tree-sitter-cli
#               or: cargo install tree-sitter-cli
#
# Exit codes:
#   0  all checks passed
#   1  one or more checks failed

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
EXAMPLES="$ROOT/examples"
TS_DIR="$ROOT/tree-sitter-animatix"

PEG_ERRORS=0
TS_ERRORS=0
TS_SKIPPED=0
TOTAL=0

echo "========================================"
echo "Animatix parser sync check"
echo "========================================"

# ── 1. PEG parser (Chumsky / animatix-syntax) ──────────────────────────────
echo ""
echo "1. PEG parser: cargo test -p animatix-syntax"
echo "----------------------------------------"
if cargo test -p animatix-syntax --lib --quiet 2>&1; then
    echo "PEG parser tests: PASSED"
else
    echo "PEG parser tests: FAILED"
    PEG_ERRORS=$((PEG_ERRORS + 1))
fi

# ── 2. Tree-sitter corpus tests ─────────────────────────────────────────────
echo ""
echo "2. Tree-sitter corpus: tree-sitter test"
echo "----------------------------------------"
if ! command -v tree-sitter &>/dev/null; then
    echo "WARNING: tree-sitter CLI not found; skipping corpus tests"
    echo "  Install with: npm install -g tree-sitter-cli"
    TS_SKIPPED=$((TS_SKIPPED + 1))
else
    set +e
    (cd "$TS_DIR" && tree-sitter test 2>&1)
    ts_exit=$?
    set -e
    if [ "$ts_exit" -eq 0 ]; then
        echo "Tree-sitter corpus tests: PASSED"
    else
        echo "Tree-sitter corpus tests: FAILED"
        TS_ERRORS=$((TS_ERRORS + 1))
    fi
fi

# ── 3. Tree-sitter parse over all examples/*.amx ────────────────────────────
echo ""
echo "3. Tree-sitter parse: examples/*.amx"
echo "----------------------------------------"
if ! command -v tree-sitter &>/dev/null; then
    echo "WARNING: tree-sitter CLI not found; skipping example parse checks"
    TS_SKIPPED=$((TS_SKIPPED + 1))
else
    PARSE_FAIL=0
    for f in "$EXAMPLES"/*.amx; do
        [ -f "$f" ] || continue
        TOTAL=$((TOTAL + 1))
        name="$(basename "$f")"
        result=$(cd "$TS_DIR" && tree-sitter parse --quiet "$f" 2>&1)
        if echo "$result" | grep -q "ERROR"; then
            echo "  FAIL: $name"
            echo "$result" | grep "ERROR" | head -3 | sed 's/^/    /'
            PARSE_FAIL=$((PARSE_FAIL + 1))
        else
            echo "  OK:   $name"
        fi
    done
    if [ "$PARSE_FAIL" -gt 0 ]; then
        echo ""
        echo "Tree-sitter parse: $PARSE_FAIL / $TOTAL file(s) FAILED"
        TS_ERRORS=$((TS_ERRORS + 1))
    else
        echo ""
        echo "Tree-sitter parse: all $TOTAL file(s) PASSED"
    fi
fi

# ── Summary ──────────────────────────────────────────────────────────────────
echo ""
echo "========================================"
echo "Summary"
echo "========================================"
echo "  PEG parser errors:       $PEG_ERRORS"
echo "  Tree-sitter errors:      $TS_ERRORS"
[ "$TS_SKIPPED" -gt 0 ] && echo "  Tree-sitter skipped:     $TS_SKIPPED (tree-sitter CLI not installed)"
echo ""

if [ "$PEG_ERRORS" -gt 0 ] || [ "$TS_ERRORS" -gt 0 ]; then
    echo "SYNC CHECK FAILED"
    echo ""
    echo "If grammars have drifted, update BOTH:"
    echo "  - PEG parser:       crates/animatix-syntax/src/parser/"
    echo "  - Tree-sitter:      tree-sitter-animatix/grammar.js"
    echo "  - Regenerate:       cd tree-sitter-animatix && tree-sitter generate"
    echo "  - Run this script:  bash scripts/check-parser-sync.sh"
    exit 1
fi

echo "SYNC CHECK PASSED"
