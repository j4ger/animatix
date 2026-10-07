#!/usr/bin/env bash
# The wasm-bindgen version pin, in exactly one place.
#
# `crates/animatix-web/Cargo.toml` pins `wasm-bindgen = "=X.Y.Z"` because
# `wasm-bindgen-futures` releases in lockstep with it: a `wasm-bindgen` CLI that
# does not match the crate version compiles for several minutes and then fails at
# glue generation. That made the version a fact three callers needed (build-web.sh,
# the CI provisioning in ci.sh, and the Pages workflow), and each copy drifted.
# This file is the fourth, and now only, source: everything else calls `wbg_pin`.
#
# Usage:
#   . scripts/wasm-tools.sh
#   wbg_pin                 # -> the pinned version string
#   check_wbg               # -> ok/error exit, naming what to install
#   install_wbg [dir]       # -> download the pinned CLI into dir (default /usr/local/bin)

# Echo the pinned wasm-bindgen version, read from the crate that pins it.
wbg_pin() {
  local root manifest
  root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  manifest="$root/crates/animatix-web/Cargo.toml"
  sed -nE 's/^wasm-bindgen = "=([0-9.]+)".*/\1/p' "$manifest" | head -1
}

# Fail with the exact remedy unless the CLI on PATH matches the pin.
check_wbg() {
  local want have
  want="$(wbg_pin)"
  if ! command -v wasm-bindgen >/dev/null 2>&1; then
    echo "error: wasm-bindgen not on PATH; animatix-web pins =$want." >&2
    echo "       scripts/ci.sh setup wasm-bindgen" >&2
    echo "       (or: cargo install --version $want --locked wasm-bindgen-cli)" >&2
    return 1
  fi
  have="$(wasm-bindgen --version | awk '{print $NF}')"
  if [ -n "$want" ] && [ "$have" != "$want" ]; then
    echo "error: wasm-bindgen $have is on PATH but animatix-web pins =$want." >&2
    echo "       wasm-bindgen-futures releases in lockstep with it, so a mismatch" >&2
    echo "       fails at glue generation. Install the pinned CLI:" >&2
    echo "       scripts/ci.sh setup wasm-bindgen" >&2
    echo "       (or: cargo install --version $want --locked wasm-bindgen-cli)" >&2
    return 1
  fi
}

# Fetch the pinned release binary. `target` defaults to the musl static build,
# which runs on any Linux regardless of the runner's glibc.
install_wbg() {
  local dir="${1:-/usr/local/bin}" want tmp
  want="$(wbg_pin)"
  if [ -z "$want" ]; then
    echo "error: could not read the wasm-bindgen pin from crates/animatix-web/Cargo.toml" >&2
    return 1
  fi
  tmp="$(mktemp -d)"
  curl -sL "https://github.com/rustwasm/wasm-bindgen/releases/download/${want}/wasm-bindgen-${want}-x86_64-unknown-linux-musl.tar.gz" \
    | tar -xz -C "$tmp"
  if [ -w "$dir" ]; then
    install -m 0755 "$tmp/wasm-bindgen-${want}-x86_64-unknown-linux-musl/wasm-bindgen" "$dir/wasm-bindgen"
  else
    sudo install -m 0755 "$tmp/wasm-bindgen-${want}-x86_64-unknown-linux-musl/wasm-bindgen" "$dir/wasm-bindgen"
  fi
  rm -rf "$tmp"
  "$dir/wasm-bindgen" --version
}
