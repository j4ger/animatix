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
#   install_wbg [dir]       # -> checksum-verified download (default ~/.local/bin)
#
# Bumping the pin therefore touches three places: this file's `wbg_sha256`, the
# URL+hash of the fixed-output derivation in `flake.nix`, and the crate pin.

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
  if [ -z "$want" ]; then
    echo "error: could not read the wasm-bindgen pin from crates/animatix-web/Cargo.toml." >&2
    echo "       Expected a line: wasm-bindgen = \"=X.Y.Z\"" >&2
    return 1
  fi
  if ! command -v wasm-bindgen >/dev/null 2>&1; then
    echo "error: wasm-bindgen not on PATH; animatix-web pins =$want." >&2
    echo "       Nix:   nix develop .#web-build   (the shell carries the pinned CLI)" >&2
    echo "       Other: scripts/ci.sh setup wasm-bindgen" >&2
    return 1
  fi
  have="$(wasm-bindgen --version | awk '{print $NF}')"
  if [ "$have" != "$want" ]; then
    echo "error: wasm-bindgen $have is on PATH but animatix-web pins =$want." >&2
    echo "       wasm-bindgen-futures releases in lockstep with it, so a mismatch" >&2
    echo "       fails at glue generation. Use nix develop .#web-build, or:" >&2
    echo "       scripts/ci.sh setup wasm-bindgen" >&2
    return 1
  fi
}

# SHA-256 of the pinned release tarball. Only the Linux x86_64 musl build is
# published as a static binary; other platforms install the CLI themselves.
wbg_sha256() {
  case "$(wbg_pin)" in
    0.2.128) echo "b51f0208fdff83515a787bd8ab9ac5865ed84dabb66d0c709957bb59793c645f" ;;
    *) return 1 ;;
  esac
}

# Fetch and verify the pinned release binary into a user-writable directory.
# This is the non-Nix fallback: .#web-build now carries the CLI, so CI and Nix
# users never reach it. It never uses sudo and never trusts an unverified
# tarball. Only the x86_64 Linux musl build is published; other platforms are
# told to `cargo install` the exact version instead of receiving a binary that
# cannot run.
install_wbg() {
  local dir="${1:-$HOME/.local/bin}" want sha tmp url
  want="$(wbg_pin)"
  if [ -z "$want" ]; then
    echo "error: could not read the wasm-bindgen pin from crates/animatix-web/Cargo.toml" >&2
    return 1
  fi
  if [ "$(uname -s)" != "Linux" ] || [ "$(uname -m)" != "x86_64" ]; then
    echo "error: no pinned wasm-bindgen binary for $(uname -s)/$(uname -m)." >&2
    echo "       Install the CLI for =$want with:" >&2
    echo "       cargo install --version $want --locked wasm-bindgen-cli" >&2
    return 1
  fi
  sha="$(wbg_sha256)" || {
    echo "error: no pinned checksum for wasm-bindgen $want." >&2
    echo "       Add it to wbg_sha256 in scripts/wasm-tools.sh." >&2
    return 1
  }
  mkdir -p "$dir" || return 1
  [ -w "$dir" ] || {
    echo "error: $dir is not writable; pass a writable directory or set PATH." >&2
    return 1
  }
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' RETURN
  url="https://github.com/rustwasm/wasm-bindgen/releases/download/${want}/wasm-bindgen-${want}-x86_64-unknown-linux-musl.tar.gz"
  echo "downloading wasm-bindgen $want"
  curl -fsSL "$url" -o "$tmp/wbg.tar.gz" || {
    echo "error: could not download $url" >&2
    return 1
  }
  if command -v sha256sum >/dev/null 2>&1; then
    printf '%s  %s\n' "$sha" "$tmp/wbg.tar.gz" | sha256sum -c - >/dev/null || return 1
  else
    printf '%s  %s\n' "$sha" "$tmp/wbg.tar.gz" | shasum -a 256 -c - >/dev/null || return 1
  fi
  tar -xzf "$tmp/wbg.tar.gz" -C "$tmp"
  install -m 0755 "$tmp/wasm-bindgen-${want}-x86_64-unknown-linux-musl/wasm-bindgen" "$dir/wasm-bindgen"
  "$dir/wasm-bindgen" --version
  case ":$PATH:" in
    *":$dir:"*) ;;
    *) echo "note: add $dir to PATH to use it in this shell" >&2 ;;
  esac
}
