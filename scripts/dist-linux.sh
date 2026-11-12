#!/usr/bin/env bash
# Build the self-contained x86_64 Linux distribution in the pinned Ubuntu 22.04
# toolchain container. This is intentionally separate from the Nix development
# shell used by local builds and performance work.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
# shellcheck source=packaging/linux/sources.lock
. packaging/linux/sources.lock

out="${1:-target/dist-linux-package}"
case "$out" in
  ""|.|./|/|web|web/|target|target/)
    echo "error: refusing unsafe output directory '$out'" >&2
    exit 2
    ;;
esac
case "$out" in
  /*|../*|*/../*|*/..) echo "error: output must stay inside the repository" >&2; exit 2 ;;
esac

case "$out" in
  target/*|dist/*) ;;
  *) echo "error: Linux package output must be below target/ or dist/" >&2; exit 2 ;;
esac

command -v docker >/dev/null 2>&1 || {
  echo "error: Docker is required for the pinned Ubuntu 22.04 release build." >&2
  exit 1
}

image_tag="animatix-linux-dist:$(sha256sum packaging/linux/Dockerfile packaging/linux/sources.lock | sha256sum | cut -c1-16)"
if ! docker image inspect "$image_tag" >/dev/null 2>&1; then
  docker build --platform linux/amd64 \
    --tag "$image_tag" \
    --file packaging/linux/Dockerfile \
    packaging/linux
fi

# A shared checkout can receive packaging edits while the image builds. Never
# run a container assembled from an earlier Dockerfile with the new scripts.
current_tag="animatix-linux-dist:$(sha256sum packaging/linux/Dockerfile packaging/linux/sources.lock | sha256sum | cut -c1-16)"
if [ "$current_tag" != "$image_tag" ]; then
  exec bash "$root/scripts/dist-linux.sh" "$out"
fi

mkdir -p target
image_id="$(docker image inspect --format '{{.Id}}' "$image_tag")"
docker run --rm --platform linux/amd64 \
  --user "$(id -u):$(id -g)" \
  --env HOME=/tmp \
  --env CARGO_HOME=/workspace/target/dist-linux-cargo-home \
  --env RUSTUP_HOME=/usr/local/rustup \
  --env "RUSTUP_TOOLCHAIN=$RUST_VERSION" \
  --env "CARGO_NET_OFFLINE=${ANIMATIX_DIST_OFFLINE:-false}" \
  --env "ANIMATIX_SOURCE_COMMIT=${ANIMATIX_SOURCE_COMMIT:-}" \
  --env "ANIMATIX_SOURCE_DIRTY=${ANIMATIX_SOURCE_DIRTY:-}" \
  --env "ANIMATIX_SOURCE_TREE_SHA256=${ANIMATIX_SOURCE_TREE_SHA256:-}" \
  --env CARGO_NET_RETRY=5 \
  --env CARGO_HTTP_MULTIPLEXING=false \
  --env "ANIMATIX_BUILD_JOBS=${ANIMATIX_BUILD_JOBS:-8}" \
  --env "ANIMATIX_BUILD_IMAGE_ID=$image_id" \
  --volume "$root:/workspace" \
  --workdir /workspace \
  "$image_tag" \
  bash scripts/dist-linux-container.sh "$out"
