#!/usr/bin/env bash
# Opt-in build-host experiment. Never installs system software or deploys.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
mode="${1:-probe}"
[ "$#" -le 1 ] || { echo 'expected one stage: probe, nix, sdk, or site' >&2; exit 2; }
case "$mode" in probe|nix|sdk|site) ;; *) echo 'unknown experiment stage' >&2; exit 2 ;; esac

probe() {
  printf 'experiment=cloudflare-native-build stage=probe\n'
  printf 'uid=%s gid=%s\n' "$(id -u)" "$(id -g)"
  printf 'architecture=%s cpu_count=%s\n' "$(uname -m)" "$(getconf _NPROCESSORS_ONLN)"
  if [ -r /proc/meminfo ]; then
    awk '/^MemTotal:/ {print "host_memory_kib=" $2}' /proc/meminfo
  fi
  # Container limit, when exposed, is more useful than host memory.
  if [ -r /sys/fs/cgroup/memory.max ]; then
    value="$(cat /sys/fs/cgroup/memory.max)"
    case "$value" in max|*[!0-9]*) ;; *) printf 'cgroup_memory_bytes=%s\n' "$value" ;; esac
  fi
  df -Pk . | awk 'NR==2 {print "disk_total_kib=" $2 " disk_free_kib=" $4}'
  for path in . /nix /nix/store; do
    exists=no; writable=no
    [ ! -e "$path" ] || exists=yes
    [ ! -w "$path" ] || writable=yes
    printf 'path=%s exists=%s writable=%s\n' "$path" "$exists" "$writable"
  done
  for tool in nix bash git curl tar xz sha256sum timeout node npm rustc cargo sudo; do
    available=no
    if command -v "$tool" >/dev/null 2>&1; then available=yes; fi
    printf 'tool=%s available=%s\n' "$tool" "$available"
  done
  echo 'probe complete: no installation, privilege request, credentials, or deployment'
}
probe
[ "$mode" != probe ] || exit 0

# Do not silently bypass the Nix contract, install curl|sh, request sudo, or
# disable host isolation. A missing Nix needs a separately reviewed bootstrap.
command -v nix >/dev/null || { echo 'BLOCKED: Nix is not installed. Keep the probe log; do not run an unreviewed installer.' >&2; exit 3; }
command -v timeout >/dev/null || { echo 'BLOCKED: timeout is required for this bounded experiment' >&2; exit 3; }
[ -f flake.lock ] || { echo 'BLOCKED: missing pinned flake.lock' >&2; exit 3; }
if ! git_status="$(git status --porcelain)"; then
  echo 'BLOCKED: could not verify whether the checkout is clean' >&2
  exit 3
fi
[ -z "$git_status" ] || { echo 'BLOCKED: experiment requires a clean checkout' >&2; exit 3; }
lock_before="$(sha256sum flake.lock | cut -d' ' -f1)"
started=$SECONDS
trap 'printf "experiment_stage=%s elapsed_seconds=%s\n" "$mode" "$((SECONDS-started))"' EXIT
export ASTRO_TELEMETRY_DISABLED=1
export SITE_BASE=/
# Exactly flake.lock; no implicit lock update or flake-supplied configuration.
# Use the small existing Wasm shell, not the default GUI/FFmpeg shell.
nix_args=(--extra-experimental-features 'nix-command flakes' develop --no-write-lock-file .#web-build --command)
case "$mode" in
  nix)
    timeout --signal=TERM --kill-after=20s 300s nix "${nix_args[@]}" bash -eu -c '
      rustc --version; node --version; wasm-bindgen --version
      test -d "$(rustc --print sysroot)/lib/rustlib/wasm32-unknown-unknown"
      command -v wasm-opt >/dev/null; command -v zopfli >/dev/null
      echo "pinned web-build shell ready"
    '
    ;;
  sdk)
    timeout --signal=TERM --kill-after=20s 1050s nix "${nix_args[@]}" bash scripts/ci.sh gate web-build web-sdk-integrity
    ;;
  site)
    # Full per-build assembly, not reliance on files from an earlier CF build.
    timeout --signal=TERM --kill-after=20s 1050s nix "${nix_args[@]}" bash -eu -c '
      bash scripts/ci.sh gate web-build web-sdk-integrity site-artifact
      python3 scripts/cloudflare-site-check.py _site
    '
    ;;
esac
[ "$lock_before" = "$(sha256sum flake.lock | cut -d' ' -f1)" ] || { echo 'lockfile changed unexpectedly' >&2; exit 1; }
echo 'experiment succeeded; this is build evidence only, not full CI or deployment approval'
