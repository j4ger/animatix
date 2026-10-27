#!/usr/bin/env bash
# Ensure the animatix-web WASM packages in web/pkg (and web/pkg-slim) are built
# and match the current tree.
#
# Usage:
#   scripts/ensure-web-pkg.sh [--check] [--dev] [--slim] [--force] [--prompt]
#
# Flags:
#   --check   Check freshness only; exit 0 if up to date, 1 if missing or stale.
#   --dev     Expect/build the debug profile (much faster, no opt, no brotli).
#   --slim    Build only the slim profile (web/pkg-slim).
#   --force   Rebuild unconditionally even if the stamp looks current.
#   --prompt  If stale, ask before rebuilding (interactive use).
#
# Freshness is the `.build` stamp scripts/build-web.sh writes after a successful
# swap: HEAD, profile, variant and wasm-bindgen version, plus the stamp's own
# mtime against the build inputs. That catches uncommitted edits to crates/ and
# Cargo.* (which a `git log` timestamp cannot see) and refuses a debug bundle
# where a release one is expected.
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

expected_profile=release
[ "$mode_dev" = true ] && expected_profile=dev

current_head=unknown
if command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  current_head="$(git rev-parse HEAD 2>/dev/null || echo unknown)"
fi

stamp_value() {
  sed -n "s/^$2=//p" "$1/.build" 2>/dev/null | head -1
}

expected_variant_for() {
  case "$1" in
    *pkg-slim) echo slim ;;
    *) echo full ;;
  esac
}

# Echo why a package must be rebuilt; empty means it is current.
stale_reason() {
  local dir="$1" variant="$2" reason
  if [ ! -f "$dir/animatix_web_bg.wasm" ]; then echo "missing"; return; fi
  if [ ! -f "$dir/.build" ]; then echo "no .build stamp (legacy package)"; return; fi
  local head; head="$(stamp_value "$dir" head)"
  if [ "$head" != "$current_head" ]; then
    echo "built from ${head:-unknown}, HEAD is $current_head"; return
  fi
  local profile; profile="$(stamp_value "$dir" profile)"
  if [ "$profile" != "$expected_profile" ]; then
    echo "profile ${profile:-unknown}, expected $expected_profile"; return
  fi
  local built_variant; built_variant="$(stamp_value "$dir" variant)"
  if [ "$built_variant" != "$variant" ]; then
    echo "variant ${built_variant:-unknown}, expected $variant"; return
  fi
  # Any build input newer than the stamp — an uncommitted edit, a checkout, a
  # Cargo.lock change — invalidates it. This is the check a `git log` timestamp
  # could not make.
  local newer
  newer="$(find crates Cargo.toml Cargo.lock -type f -newer "$dir/.build" -print -quit 2>/dev/null || true)"
  if [ -n "$newer" ]; then echo "$newer is newer than the build"; return; fi
  if command -v wasm-bindgen >/dev/null 2>&1; then
    local have built_wbg
    have="$(wasm-bindgen --version | awk '{print $NF}')"
    built_wbg="$(stamp_value "$dir" wasm_bindgen)"
    if [ -n "$built_wbg" ] && [ "$have" != "$built_wbg" ]; then
      echo "wasm-bindgen $have, built with $built_wbg"; return
    fi
  fi
  if [ "$expected_profile" = release ] && [ -f "$dir/.dev" ]; then
    echo ".dev marker but a release package was expected"; return
  fi
  if [ "$expected_profile" = dev ] && [ ! -f "$dir/.dev" ]; then
    echo "no .dev marker but a dev package was expected"; return
  fi
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
  reason=""
  if [ "$mode_force" != true ]; then
    reason="$(stale_reason "$t" "$(expected_variant_for "$t")")"
  fi
  if [ "$mode_force" = true ] || [ -n "$reason" ]; then
    needs_build+=("$t")
    [ -n "$reason" ] && echo "stale: $t — $reason" >&2
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
