#!/usr/bin/env bash
# Build and verify an unpublished native video package on its target platform.
set -euo pipefail

case "${RUNNER_OS:-$(uname -s)}" in
  macOS|Darwin)
    case "$(uname -m)" in
      arm64) triplet=arm64-osx ;;
      x86_64) triplet=x64-osx ;;
      *) echo 'Unsupported macOS architecture' >&2; exit 2 ;;
    esac
    bootstrap=bootstrap-vcpkg.sh
    ext=''
    ;;
  Windows|MINGW*|MSYS*|CYGWIN*)
    triplet=x64-windows
    bootstrap=bootstrap-vcpkg.bat
    ext=.exe
    ;;
  *) echo 'Run the video proof on native macOS or Windows' >&2; exit 2 ;;
esac

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
vcpkg="$output/vcpkg-$vcpkg_rev"
if [ ! -x "$vcpkg/vcpkg$ext" ]; then
  mkdir -p "$vcpkg"
  git -C "$vcpkg" init -q
  git -C "$vcpkg" fetch --depth 1 https://github.com/microsoft/vcpkg.git "$vcpkg_rev"
  git -C "$vcpkg" checkout --detach FETCH_HEAD
  (cd "$vcpkg" && "./$bootstrap" -disableMetrics)
fi
[ "$(git -C "$vcpkg" rev-parse HEAD)" = "$vcpkg_rev" ]

phase ffmpeg-build execution "$vcpkg/vcpkg$ext" install --classic \
  "ffmpeg[avcodec,avdevice,avfilter,avformat,ffmpeg,ffprobe,opus,swresample,swscale,vpx,x264]:$triplet"
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
phase gui-test mixed cargo test --locked -p animatix-gui --lib save_and_reopen_preserve_quoted_text_and_typst_payloads
phase rust-dist-build cargo cargo build --locked --profile dist -p animatix-cli -p animatix-gui --features video

stage="$(mktemp -d "$output/package.XXXXXXXX")"
cp "target/dist/animatix$ext" "target/dist/animatix-gui$ext" "$stage/"
for tool in ffmpeg ffprobe; do
  cp "$installed/tools/ffmpeg/$tool$ext" "$stage/"
done
if [ -n "$ext" ]; then
  cp "$installed/bin/"*.dll "$stage/"
fi
cp LICENSE "$stage/"
mkdir -p "$stage/licenses"
for license in "$installed/share/"*/copyright; do
  port="$(basename "$(dirname "$license")")"
  cp "$license" "$stage/licenses/$port.txt"
done

"$stage/ffmpeg$ext" -version >> "$output/evidence/build-inputs.txt"
phase narrated-export execution python3 tools/release/native/test-first-run.py "$stage" "$output/evidence"
