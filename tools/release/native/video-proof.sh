#!/usr/bin/env bash
# Build and verify a native video archive on its target platform.
set -euo pipefail

case "${RUNNER_OS:-$(uname -s)}" in
  macOS|Darwin)
    platform=macos
    case "$(uname -m)" in
      arm64) triplet=arm64-osx ;;
      x86_64) triplet=x64-osx ;;
      *) echo 'Unsupported macOS architecture' >&2; exit 2 ;;
    esac
    bootstrap=bootstrap-vcpkg.sh
    ext=''
    ;;
  Windows|MINGW*|MSYS*|CYGWIN*)
    platform=windows
    triplet=x64-windows
    bootstrap=bootstrap-vcpkg.bat
    ext=.exe
    ;;
  *) echo 'Run the video proof on native macOS or Windows' >&2; exit 2 ;;
esac
if [ "${1:-$platform}" != "$platform" ] && [ "${1:-}" != --ffmpeg-only ]; then
  echo "Cannot build $1 on $platform" >&2
  exit 2
fi

# vcpkg 2026.07.29 selects FFmpeg 8.1.2#3, including checksummed sources and
# patches for FFmpeg and its codecs. Update the pin and rerun both proofs
# together, preserving the FFmpeg 8 ABI expected by rsmpeg.
vcpkg_rev=9e593bb18ea69cc5095e012465dcd675a822ed0d
mkdir -p target/native-video-proof
output="$(cd target/native-video-proof && pwd)"
mkdir -p "$output/evidence"
export ANIMATIX_PHASE_TIMINGS_DIR=target/native-video-proof/evidence/phases
phase() {
  local name="$1" kind="$2"
  shift 2
  python3 tools/ci/phase-timing.py --gate native-video-proof --phase "$name" --kind "$kind" -- "$@"
}

proof_stage="${ANIMATIX_PROOF_STAGE:-}"
if [ "${1:-}" != --ffmpeg-only ] && [ -n "$proof_stage" ] && [ -f "$proof_stage/build-inputs.txt" ]; then
  cp "$proof_stage/build-inputs.txt" "$output/evidence/"
  phase package-acceptance execution "$BASH" tools/release/dist-package.sh "$platform" "$proof_stage" --video
  exit
fi

printf 'vcpkg_commit=%s\ntriplet=%s\n' "$vcpkg_rev" "$triplet" > "$output/evidence/build-inputs.txt"
printf 'cargo_dist_lto=%s\ncargo_dist_codegen_units=%s\n' \
  "${CARGO_PROFILE_DIST_LTO:-fat}" "${CARGO_PROFILE_DIST_CODEGEN_UNITS:-1}" >> "$output/evidence/build-inputs.txt"
dependency_root="$output"
if [ -n "${RUNNER_TEMP:-}" ]; then
  dependency_root="$RUNNER_TEMP"
  if [ -n "$ext" ]; then dependency_root="$(cygpath -u "$dependency_root")"; fi
fi
vcpkg="$dependency_root/animatix-vcpkg-$vcpkg_rev"
if [ ! -x "$vcpkg/vcpkg$ext" ]; then
  mkdir -p "$vcpkg"
  git -C "$vcpkg" init -q
  git -C "$vcpkg" fetch --depth 1 https://github.com/microsoft/vcpkg.git "$vcpkg_rev"
  git -C "$vcpkg" checkout --detach FETCH_HEAD
  (cd "$vcpkg" && "./$bootstrap" -disableMetrics)
fi
[ "$(git -C "$vcpkg" rev-parse HEAD)" = "$vcpkg_rev" ]

vcpkg_options=(install --classic)
ffmpeg_cache="${ANIMATIX_FFMPEG_CACHE:-}"
if [ -n "$ffmpeg_cache" ]; then
  if [ -n "$ext" ]; then ffmpeg_cache="$(cygpath -m "$ffmpeg_cache")"; fi
  mkdir -p "$ffmpeg_cache/binaries" "$ffmpeg_cache/sources"
  export VCPKG_BINARY_SOURCES="clear;files,$ffmpeg_cache/binaries,readwrite"
fi
if [ "${ANIMATIX_REQUIRE_FFMPEG_CACHE:-false}" = true ]; then
  vcpkg_options+=(--only-binarycaching)
fi
if [ -n "$ext" ]; then
  # The video binaries link the release import libraries. The GUI save test
  # uses its default features.
  overlay="$output/triplets"
  mkdir -p "$overlay"
  cp "$vcpkg/triplets/$triplet.cmake" "$overlay/$triplet.cmake"
  printf '\nset(VCPKG_BUILD_TYPE release)\nset(VCPKG_CRT_LINKAGE static)\n' >> "$overlay/$triplet.cmake"
  vcpkg_options+=("--overlay-triplets=$(cygpath -m "$overlay")")
  printf 'vcpkg_build_type=release\nvcpkg_crt_linkage=static\n' >> "$output/evidence/build-inputs.txt"
fi
# The vcpkg port disables autodetection, and FFmpeg's PNG decoder needs zlib.
ffmpeg_features=avcodec,avdevice,avfilter,avformat,ffmpeg,ffprobe,opus,swresample,swscale,vpx,x264,zlib
phase ffmpeg-build execution "$vcpkg/vcpkg$ext" "${vcpkg_options[@]}" \
  "ffmpeg[$ffmpeg_features]:$triplet"
installed="$vcpkg/installed/$triplet"
unset FFMPEG_BINDING_PATH FFMPEG_DLL_PATH FFMPEG_PKG_CONFIG_PATH FFMPEG_LIBS_DIR
if [ -z "$ext" ]; then
  export FFMPEG_INCLUDE_DIR="$installed/include"
  export FFMPEG_PKG_CONFIG_PATH="$installed/lib/pkgconfig"
  export FFMPEG_LINK_MODE=static
else
  # Rust's build script needs Windows paths even though Actions invokes bash.
  export FFMPEG_INCLUDE_DIR="$(cygpath -m "$installed/include")"
  export FFMPEG_LIBS_DIR="$(cygpath -m "$installed/lib")"
  export FFMPEG_LINK_MODE=dynamic
fi
stage="$(mktemp -d "$output/package.XXXXXXXX")"
for tool in ffmpeg ffprobe; do
  cp "$installed/tools/ffmpeg/$tool$ext" "$stage/"
done
if [ -z "$ext" ]; then
  file "$stage/ffmpeg" "$stage/ffprobe"
fi
if [ -n "$ext" ]; then
  cp "$installed/bin/"*.dll "$stage/"
  export RUST_LOG="${RUST_LOG:-warn,wgpu_hal::dx12::instance=debug}"
fi
mkdir -p "$stage/licenses" "$stage/media-source"
for license in "$installed/share/"*/copyright; do
  port="$(basename "$(dirname "$license")")"
  cp "$license" "$stage/licenses/$port.txt"
done
git -C "$vcpkg" archive --format=tar.gz HEAD > "$stage/media-source/vcpkg-$vcpkg_rev.tar.gz"
# Binary restores can leave downloads empty. Use the same hashes as the pinned ports.
mkdir -p "$vcpkg/downloads"
while read -r source sha512 url; do
  case "$source" in ''|\#*) continue ;; esac
  archive="$vcpkg/downloads/$source"
  if [ -n "$ffmpeg_cache" ] && [ -f "$ffmpeg_cache/sources/$source" ]; then
    archive="$ffmpeg_cache/sources/$source"
  fi
  if [ ! -f "$archive" ] || [ "$("$vcpkg/vcpkg$ext" hash "$archive" sha512)" != "$sha512" ]; then
    (cd "$vcpkg/downloads" && "$vcpkg/vcpkg$ext" x-download "$source" "--sha512=$sha512" "--url=$url") </dev/null
    archive="$vcpkg/downloads/$source"
  fi
  cp "$archive" "$stage/media-source/"
  if [ -n "$ffmpeg_cache" ] && [ "$archive" != "$ffmpeg_cache/sources/$source" ]; then
    cp "$archive" "$ffmpeg_cache/sources/"
  fi
done < tools/release/native/vcpkg-sources.lock
if [ -n "$ext" ]; then
  cp "$overlay/$triplet.cmake" "$stage/media-source/"
fi
cat > "$stage/media-source/REBUILD.txt" <<EOF
Animatix commit $(git rev-parse HEAD)
The companion Linux source archive for this release contains the Animatix
source and vendored Cargo dependencies for the same commit.

Extract the vcpkg archive, place the codec archives in its downloads directory,
and bootstrap vcpkg. On Windows, use the accompanying overlay triplet.
Run vcpkg install --classic 'ffmpeg[$ffmpeg_features]:$triplet'. The vcpkg tree
contains the pins, patches, and build instructions for the packaged libraries.
Run the dist-$platform gate from the matching Animatix source for the application.
EOF

"$stage/ffmpeg$ext" -version >> "$output/evidence/build-inputs.txt"
if ! "$stage/ffmpeg$ext" -hide_banner -decoders | awk '$2 == "png" { found=1 } END { exit !found }'; then
  echo 'The staged FFmpeg must decode PNG for the preview check.' >&2
  exit 1
fi
if [ "${1:-}" = --ffmpeg-only ]; then
  exit 0
fi

build=(build --locked --profile dist -p animatix-cli -p animatix-gui --features video)
cargo_output="${CARGO_TARGET_DIR:-target}"
if [ -n "$ext" ]; then
  # Target only the shipped binaries so host build scripts keep their normal CRT.
  rust_target=x86_64-pc-windows-msvc
  export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C target-feature=+crt-static"
  build+=(--target "$rust_target")
  cargo_output="$cargo_output/$rust_target"
  printf 'rust_target=%s\nrust_crt_linkage=static\n' "$rust_target" >> "$output/evidence/build-inputs.txt"
fi
phase rust-dist-build cargo cargo "${build[@]}"
cp "$cargo_output/dist/animatix$ext" "$cargo_output/dist/animatix-gui$ext" "$stage/"
cp "$output/evidence/build-inputs.txt" "$stage/"
if [ -n "$proof_stage" ]; then
  # The independent proof can retry the archive check with identical build inputs.
  cp -R "$stage" "$proof_stage"
fi
# Native Python on Windows can otherwise resolve bash to the WSL launcher.
phase package-acceptance execution "$BASH" tools/release/dist-package.sh "$platform" "$stage" --video
