#!/usr/bin/env bash
# Ensure the animatix-web WASM packages in web/pkg (and web/pkg-slim) are built
# and not stale relative to engine changes in crates/.
#
# Usage:
#   scripts/ensure-web-pkg.sh [--check] [--dev] [--slim] [--force] [--prompt]
#
# Flags:
#   --check   Check freshness only; exit 0 if up to date, 1 if missing or stale.
#   --dev     Build with debug profile (much faster for local iteration, no opt).
#   --slim    Build only the slim profile (web/pkg-slim).
#   --force   Rebuild unconditionally even if timestamps appear current.
#   --prompt  If stale, ask before rebuilding (interactive use).
#
# When a rebuild is needed and the current shell lacks the wasm32 toolchain,
# this script automatically enters `nix develop .#web-build` using a clean
# environment so that nested devshells and PATH conflicts do not fail the build.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

mode_check=false
mode_dev=false
mode_slim=false
mode_force=false
mode_prompt=false

for arg in "$@"; do
  case "$arg" in
    --check)  mode_check=true ;;
    --dev)    mode_dev=true ;;
    --slim)   mode_slim=true ;;
    --force)  mode_force=true ;;
    --prompt) mode_prompt=true ;;
    *)
      echo "Usage: $0 [--check] [--dev] [--slim] [--force] [--prompt]" >&2
      exit 1
      ;;
  esac
done

# Primary WASM artifact to check
target_dir="web/pkg"
if [ "$mode_slim" = true ]; then
  target_dir="web/pkg-slim"
fi
wasm_file="$target_dir/animatix_web_bg.wasm"

# 1. Check artifact existence
is_missing=false
if [ ! -f "$wasm_file" ]; then
  is_missing=true
fi

# 2. Check staleness against crates/
is_stale=false
latest_crates_commit_time=0
latest_crates_commit_hash="unknown"
wasm_mtime=0

if command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  latest_crates_commit_time=$(git log -1 --format=%ct crates/ 2>/dev/null || echo 0)
  latest_crates_commit_hash=$(git log -1 --format="%h (%s)" crates/ 2>/dev/null || echo "unknown")
fi

if [ -f "$wasm_file" ]; then
  wasm_mtime=$(stat -c %Y "$wasm_file" 2>/dev/null || stat -f %m "$wasm_file" 2>/dev/null || echo 0)
fi

if [ "$is_missing" = true ]; then
  is_stale=true
elif [ "$latest_crates_commit_time" -gt "$wasm_mtime" ]; then
  is_stale=true
fi

# Format timestamps for display
format_time() {
  local ts="$1"
  if [ "$ts" -eq 0 ]; then
    echo "none"
  elif date -d "@$ts" "+%Y-%m-%d %H:%M:%S" >/dev/null 2>&1; then
    date -d "@$ts" "+%Y-%m-%d %H:%M:%S"
  elif date -r "$ts" "+%Y-%m-%d %H:%M:%S" >/dev/null 2>&1; then
    date -r "$ts" "+%Y-%m-%d %H:%M:%S"
  else
    echo "$ts"
  fi
}

wasm_time_str=$(format_time "$wasm_mtime")
crates_time_str=$(format_time "$latest_crates_commit_time")

if [ "$mode_force" = false ] && [ "$is_stale" = false ]; then
  if [ "$mode_check" = true ]; then
    echo "✓ $target_dir is up to date (built: $wasm_time_str, engine: $crates_time_str)."
  fi
  exit 0
fi

# Here we need a rebuild or check failure
if [ "$mode_check" = true ]; then
  if [ "$is_missing" = true ]; then
    echo "error: $wasm_file is missing." >&2
  else
    echo "warning: $wasm_file is OUTDATED!" >&2
    echo "         WASM build:    $wasm_time_str" >&2
    echo "         Latest engine: $crates_time_str [$latest_crates_commit_hash]" >&2
    echo "         Engine changes in crates/ have not been compiled into WASM." >&2
  fi
  echo "         Run: scripts/ensure-web-pkg.sh [--dev]" >&2
  exit 1
fi

# In prompt mode, ask before proceeding
if [ "$mode_prompt" = true ]; then
  echo "WASM artifact ($target_dir) is outdated:"
  echo "  Built:  $wasm_time_str"
  echo "  Engine: $crates_time_str [$latest_crates_commit_hash]"
  read -r -p "Recompile now? [y/N] " response
  case "$response" in
    [yY][eE][sS]|[yY]) ;;
    *)
      echo "Aborted."
      exit 0
      ;;
  esac
fi

build_args=()
if [ "$mode_dev" = true ]; then
  build_args+=(--dev)
fi
if [ "$mode_slim" = true ]; then
  build_args+=(--slim)
fi

echo "Building WASM ($target_dir)..."

# If current rustc already has the wasm32 target, run directly
if rustc --print sysroot >/dev/null 2>&1 && \
   ls "$(rustc --print sysroot)/lib/rustlib/wasm32-unknown-unknown" >/dev/null 2>&1; then
  bash scripts/build-web.sh "${build_args[@]}"
else
  # Re-enter with clean environment so outer Nix shells do not shadow wasm32 std
  clean_path="$HOME/.cargo/bin:/nix/var/nix/profiles/default/bin:$HOME/.nix-profile/bin:/usr/bin:/bin"
  echo "Entering nix develop .#web-build..."
  env -i HOME="$HOME" USER="${USER:-$(whoami)}" PATH="$clean_path" \
    nix develop .#web-build --command bash scripts/build-web.sh "${build_args[@]}"
fi

echo "✓ WASM build ready in $target_dir."
