#!/usr/bin/env bash
# Safe launcher for agent-browser in this development environment.
#
# Solves two common environment failure modes:
# 1. Nix devshells prepend to LD_LIBRARY_PATH, which causes host Chromium/Firefox
#    dynamic linkers to fail against mismatched OpenSSL libraries.
# 2. Host Chromium path detection and default WebGPU enablement.
# 3. Warns if web/pkg WASM packages are stale relative to crates/ engine sources.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# 1. Check for stale WASM artifacts before browser commands
if [ -x "$root/scripts/ensure-web-pkg.sh" ]; then
  "$root/scripts/ensure-web-pkg.sh" --check >&2 || true
fi

# 2. Detect host browser if AGENT_BROWSER_EXECUTABLE_PATH is not already set
if [ -z "${AGENT_BROWSER_EXECUTABLE_PATH:-}" ]; then
  for candidate in /usr/bin/chromium-browser /usr/bin/chromium /usr/bin/google-chrome /usr/bin/firefox; do
    if [ -x "$candidate" ]; then
      export AGENT_BROWSER_EXECUTABLE_PATH="$candidate"
      break
    fi
  done
fi

# 3. Default to WebGPU enablement if not explicitly turned off
if [ -z "${AGENT_BROWSER_WEBGPU:-}" ]; then
  export AGENT_BROWSER_WEBGPU=1
fi

# 4. Strip LD_LIBRARY_PATH so host browser linkers do not collide with Nix libraries
exec env -u LD_LIBRARY_PATH AGENT_BROWSER_EXECUTABLE_PATH="${AGENT_BROWSER_EXECUTABLE_PATH:-}" \
  AGENT_BROWSER_WEBGPU="${AGENT_BROWSER_WEBGPU:-1}" \
  npx agent-browser "$@"
