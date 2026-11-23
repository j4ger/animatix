#!/usr/bin/env bash
# Verify the integrity of bundled font assets against the pinned SHA-256 table.
# Fails loudly if any vendored font drifts from the recorded hash, so accidental
# edits / partial re-vendors are caught. Re-vendoring on purpose = replace the
# files from a trusted source, update the hashes in this script AND in
# crates/animatix-text/assets/fonts/README.md, then re-run this script.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FONTS_DIR="$SCRIPT_DIR/../../../crates/animatix-text/assets/fonts"

# file → expected sha256 (keep in sync with assets/fonts/README.md)
declare -A EXPECTED=(
    ["OpenSans-Regular.ttf"]="8ab4aa561e7db0eb3e1af8b0bed2a315e0a33fe2ed3070e645d1b89f8efc1d5c"
    ["OpenSans-Variable.ttf"]="36643644f318a812aab2d2ed3bb98f8cf0872527f835fe9398d95fe6b9adb878"
    ["OpenSans-Italic-Variable.ttf"]="fe269381e992f32e135801740998544d6235061e37c93ec067ad2be3edd5b17b"
    ["OpenSans-Bold.ttf"]="1a6bc6775358bfed0e4191b6f2c4d7d75d122f0c6e5a255f264ab455c67237b7"
    ["OpenSans-Italic.ttf"]="e5178be12cd740aeafebea15ec563fe577bbb4fab42d9e40500bd49ec8c9ce16"
    ["OpenSans-BoldItalic.ttf"]="b5c44af3cb55f65fadb2f1b20edc38e1008bb71388d04ad127c5ad340c9329f2"
    ["DejaVuSansMono.ttf"]="b4a6c3e4faab8773f4ff761d56451646409f29abedd68f05d38c2df667d3c582"
    ["DejaVuSansMono-Bold.ttf"]="bce60f1b4421acd9ea51ba6623d7024ecbe6817a953e3654df62a5e6bdf8f769"
    ["DejaVuSansMono-Oblique.ttf"]="742097840c541870e8d6dc5c9b37bb1ceeea6c0dedd1d475faf903ef9df734b0"
    ["DejaVuSansMono-BoldOblique.ttf"]="91713a71d550bba22c2a6b2bb2a9ad8f9a159e12e4e9f0a5b2677998ba21213e"
)

FAILED=0
for name in "${!EXPECTED[@]}"; do
    file="$FONTS_DIR/$name"
    if [ ! -f "$file" ]; then
        echo "MISSING $name (expected at $file)" >&2
        FAILED=1
        continue
    fi
    actual=$(sha256sum "$file" | awk '{print $1}')
    if [ "$actual" != "${EXPECTED[$name]}" ]; then
        echo "HASH MISMATCH $name: got $actual, expected ${EXPECTED[$name]}" >&2
        FAILED=1
    else
        echo "ok  $name"
    fi
done

if [ "$FAILED" -ne 0 ]; then
    echo "=== Bundled font integrity check FAILED ===" >&2
    exit 1
fi
echo "=== Bundled font integrity check passed ==="
