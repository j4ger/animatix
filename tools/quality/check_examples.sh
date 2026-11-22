#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"
bin="${ANIMATIX_BIN:-$root/target/debug/animatix}"
if [ ! -x "$bin" ]; then
    cargo build --quiet --bin animatix
fi
scenes="$(python3 tools/quality/audit_corpus.py)"
check_scene() {
    printf 'Checking %s\n' "$1"
    "$bin" check --format json "$@" | python3 -c '
import json
import sys

result = json.load(sys.stdin)
for diagnostic in result.get("errors", []):
    if diagnostic["severity"] not in ("hint", "info"):
        print("{severity} [{code}] {message}".format(**diagnostic), file=sys.stderr)
        result["passed"] = False
sys.exit(not result["passed"])
'
}

while IFS= read -r scene; do
    check_scene "$scene"
done <<< "$scenes"

# This native extension has its own sample in the plugin crate.
cargo build -p animatix-plugin-demo
check_scene crates/animatix-plugin-demo/demo.amx \
    --plugin crates/animatix-plugin-demo/demo.amx-plugin.toml
