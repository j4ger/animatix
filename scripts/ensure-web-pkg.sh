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

latest_crates_commit_time=0
if command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  latest_crates_commit_time=$(git log -1 --format=%ct crates/ 2>/dev/null || echo 0)
fi

# Check freshness for a package directory
is_stale_or_missing() {
  local dir="$1"
  local wasm="$dir/animatix_web_bg.wasm"
  if [ ! -f "$wasm" ]; then return 0; fi
  local mtime
  mtime=$(stat -c %Y "$wasm" 2>/dev/null || stat -f %m "$wasm" 2>/dev/null || echo 0)
  if [ "$latest_crates_commit_time" -gt "$mtime" ]; then return 0; fi
  return 1
}

run_build() {
  local extra_args=("$@")
  local args=()
  if [ "$mode_dev" = true ]; then
    args+=(--dev)
  fi
  args+=("${extra_args[@]}")

  if rustc --print sysroot >/dev/null 2>&1 && \
     ls "$(rustc --print sysroot)/lib/rustlib/wasm32-unknown-unknown" >/dev/null 2>&1 && \
     [ -z "${LD_LIBRARY_PATH:-}" ]; then
    bash scripts/build-web.sh "${args[@]}"
  else
    local nix_bin_dir
    nix_bin_dir="$(dirname "$(command -v nix 2>/dev/null || echo /run/current-system/sw/bin)")"
    local clean_path="$HOME/.cargo/bin:$nix_bin_dir:/run/current-system/sw/bin:/etc/profiles/per-user/${USER:-$(whoami)}/bin:/nix/var/nix/profiles/default/bin:$HOME/.nix-profile/bin:/usr/bin:/bin"
    echo "Entering nix develop .#web-build..."
    env -i HOME="$HOME" USER="${USER:-$(whoami)}" PATH="$clean_path" \
      nix develop .#web-build --command bash scripts/build-web.sh "${args[@]}"
  fi
}

targets=()
if [ "$mode_slim" = true ]; then
  targets+=("web/pkg-slim")
else
  targets+=("web/pkg-slim" "web/pkg")
fi

needs_build=()
for t in "${targets[@]}"; do
  if [ "$mode_force" = true ] || is_stale_or_missing "$t"; then
    needs_build+=("$t")
  fi
done

if [ ${#needs_build[@]} -eq 0 ]; then
  if [ "$mode_check" = true ]; then
    echo "✓ WASM packages are up to date."
  fi
  exit 0
fi

if [ "$mode_check" = true ]; then
  echo "error: WASM package(s) missing or outdated: ${needs_build[*]}" >&2
  echo "       Run: scripts/ensure-web-pkg.sh [--dev]" >&2
  exit 1
fi

if [ "$mode_prompt" = true ]; then
  echo "WASM packages need rebuilding: ${needs_build[*]}"
  read -r -p "Recompile now? [y/N] " response
  case "$response" in
    [yY][eE][sS]|[yY]) ;;
    *)
      echo "Aborted."
      exit 0
      ;;
  esac
fi

for t in "${needs_build[@]}"; do
  echo "Building WASM ($t)..."
  if [ "$t" = "web/pkg-slim" ]; then
    run_build --slim
  else
    run_build
  fi
  echo "✓ WASM build ready in $t."
done
