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
if [ "${1:-$platform}" != "$platform" ]; then
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
for source in ffmpeg-ffmpeg-n8.1.2.tar.gz \
  videolan-x264-31e19f92f00c7003fa115047ce50978bc98c3a0d.tar.gz \
  webmproject-libvpx-v1.16.0.tar.gz xiph-opus-v1.5.2.tar.gz madler-zlib-v1.3.2.tar.gz; do
  cp "$vcpkg/downloads/$source" "$stage/media-source/"
done
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

phase rust-dist-build cargo cargo build --locked --profile dist -p animatix-cli -p animatix-gui --features video
cargo_dist="${CARGO_TARGET_DIR:-target}/dist"
cp "$cargo_dist/animatix$ext" "$cargo_dist/animatix-gui$ext" "$stage/"
cp "$output/evidence/build-inputs.txt" "$stage/"
# Native Python on Windows can otherwise resolve bash to the WSL launcher.
phase package-acceptance execution "$BASH" tools/release/dist-package.sh "$platform" "$stage" --video
