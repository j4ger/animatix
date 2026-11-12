#!/usr/bin/env bash
# Validate an unpacked Linux package on clean Ubuntu runtime images.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
# shellcheck source=packaging/linux/sources.lock
. packaging/linux/sources.lock

pkg="${1:-target/dist-linux-package}"
case "$pkg" in
  /*) ;;
  *) pkg="$root/$pkg" ;;
esac
[ -d "$pkg" ] || { echo "error: package directory not found: $pkg" >&2; exit 2; }
evidence="${2:-target/perf/distribution-refactor/native/runtime}"
case "$evidence" in /*) ;; *) evidence="$root/$evidence" ;; esac
mkdir -p "$evidence"
command -v docker >/dev/null 2>&1 || { echo "error: Docker is required" >&2; exit 1; }

run_validation() {
  local image="$1" label="$2"
  echo "Testing Linux package on $label ($image)"
  mkdir -p "$evidence/$label"
  docker run --rm --platform linux/amd64 \
    --env EVIDENCE_DIR=/evidence \
    --volume "$evidence/$label:/evidence" \
    --volume "$pkg:/package:ro" \
    --volume "$root:/workspace:ro" \
    --workdir /tmp \
    "$image" \
    bash /workspace/packaging/linux/test-runtime.sh
}

run_validation "$UBUNTU_IMAGE" Ubuntu-22.04
run_validation "$UBUNTU_24_IMAGE" Ubuntu-24.04
