#!/usr/bin/env bash
# Materialize and verify the external web font pack.
#
# The source of truth is crates/animatix-text/assets/fonts; web/fonts only
# contains generated gzip twins plus the tracked manifest and license. Keeping
# the bytes out of git means a font refresh has one copy to update and the
# browser pack cannot silently drift from the native bundle.
#
# Usage:
#   scripts/web-fonts.sh sync     # regenerate web/fonts
#   scripts/web-fonts.sh --check  # verify manifest, gzip and source SHA-256
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

src="crates/animatix-text/assets/fonts"
out="web/fonts"
manifest="$out/manifest.json"
mode="${1:-sync}"

[ -f "$manifest" ] || {
  echo "error: $manifest is missing." >&2
  exit 1
}

manifest_files() {
  node - "$manifest" <<'JS'
const fs = require("node:fs");
const manifest = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
for (const [profile, files] of Object.entries(manifest.profiles)) {
  for (const name of files) console.log(`${profile}\t${name}`);
}
JS
}

check() {
  local failures=0
  local profile file source gz
  while IFS=$'\t' read -r profile file; do
    source="$src/${file%.gz}"
    gz="$out/$file"
    if [ ! -f "$source" ]; then
      echo "error: $profile entry '$file' has no source '$source'." >&2
      failures=$((failures + 1))
      continue
    fi
    if [ ! -f "$gz" ]; then
      echo "error: $profile entry '$file' is missing from $out." >&2
      failures=$((failures + 1))
      continue
    fi
    if ! gzip -t "$gz" 2>/dev/null; then
      echo "error: $file is not a valid gzip stream." >&2
      failures=$((failures + 1))
      continue
    fi
    if [ "$(gzip -dc "$gz" | sha256sum | awk '{print $1}')" != "$(sha256sum "$source" | awk '{print $1}')" ]; then
      echo "error: $file does not decompress to $source." >&2
      failures=$((failures + 1))
    fi
  done < <(manifest_files)
  if [ "$failures" -ne 0 ]; then
    echo "font pack check failed: $failures problem(s)." >&2
    return 1
  fi
  local unique_count profile_count
  unique_count="$(manifest_files | cut -f2 | sort -u | wc -l | tr -d ' ')"
  profile_count="$(node - "$manifest" <<'JS'
const fs = require("node:fs");
const manifest = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
console.log(Object.keys(manifest.profiles).length);
JS
)"
  echo "font pack ok: $unique_count files across $profile_count profiles"
}

synced() {
  local profile file source gz tmp
  mkdir -p "$out"
  while IFS=$'\t' read -r profile file; do
    source="$src/${file%.gz}"
    gz="$out/$file"
    [ -f "$source" ] || {
      echo "error: $profile entry '$file' has no source '$source'." >&2
      exit 1
    }
    tmp="$gz.tmp"
    gzip -9 -n -c "$source" > "$tmp"
    mv "$tmp" "$gz"
  done < <(manifest_files)
  cp "$src/LICENSE-OpenSans.txt" "$out/LICENSE-OpenSans.txt"
  echo "font pack synced to $out"
  check
}

case "$mode" in
  sync) synced ;;
  --check) check ;;
  *)
    echo "usage: $0 [sync|--check]" >&2
    exit 2
    ;;
esac
