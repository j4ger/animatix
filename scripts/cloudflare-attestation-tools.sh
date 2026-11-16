#!/usr/bin/env bash
# Source this from the verified consumer; install only under its private temp dir.
set -euo pipefail
out="${1:?private output directory required}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$out"
archive="$out/gh.tar.gz"
curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
  --max-time 120 --max-filesize 100000000 \
  https://github.com/cli/cli/releases/download/v2.104.0/gh_2.104.0_linux_amd64.tar.gz -o "$archive"
echo "98cb101ad87dbbd3f6b2f2bfcbd43197ccd07643140121d8a9cfaa91eadbc24e  $archive" | sha256sum -c -
echo "65ca537f6ed8a47fd0e560c421baa1f6c1efb8b25fc200d8c5c02c0e92eb2b9c  $root/.github/cloudflare/trusted-root.jsonl" | sha256sum -c -
tar -xzf "$archive" -C "$out"
"$out/gh_2.104.0_linux_amd64/bin/gh" --version
