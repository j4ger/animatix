#!/usr/bin/env bash
# Container-side implementation for scripts/dist-linux.sh.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
# shellcheck source=packaging/linux/sources.lock
. packaging/linux/sources.lock

out_rel="${1:-target/dist-linux-package}"
case "$out_rel" in
  ""|.|./|/|target|target/|dist|dist/)
    echo "error: refusing unsafe output directory '$out_rel'" >&2
    exit 2
    ;;
esac
case "$out_rel" in
  /*|../*|*/../*|*/..)
    echo "error: output must be a safe path within the repository" >&2
    exit 2
    ;;
esac

case "$out_rel" in
  target/*|dist/*) ;;
  *) echo "error: Linux package output must be below target/ or dist/" >&2; exit 2 ;;
esac

for tool in cargo rustc curl sha256sum pkg-config clang-14 patchelf readelf ldd flock; do
  command -v "$tool" >/dev/null 2>&1 || { echo "error: $tool is missing in the build image" >&2; exit 1; }
done
[ "$(rustc --version | awk '{print $2}')" = "$RUST_VERSION" ] || {
  echo "error: expected Rust $RUST_VERSION, found $(rustc --version)" >&2
  exit 1
}
[ "$(uname -m)" = x86_64 ] || { echo "error: native Linux package requires x86_64" >&2; exit 1; }
[ -z "${FFMPEG_BINDING_PATH:-}" ] || {
  echo "error: FFMPEG_BINDING_PATH must be unset so bindings are generated from the locked FFmpeg headers" >&2
  exit 1
}

work="$root/target/linux-dist-work"
src="$work/src"
project="$work/project"
downloads="$root/target/linux-dist-downloads"
image_key="${ANIMATIX_BUILD_IMAGE_ID:-unrecorded}"
image_key="${image_key#sha256:}"
target="$root/target/linux-dist-ubuntu2204"
out="$root/$out_rel"
exec 9>"$root/target/linux-dist-build.lock"
flock -n 9 || { echo 'error: another Linux distribution build is active' >&2; exit 1; }
rm -rf "$work" "$target"
mkdir -p "$src" "$downloads" "$(dirname "$out")"

# Snapshot only allowlisted project/build inputs, including tracked and dirty
# untracked source. This isolated copy is both what we build and what we ship.
python3 packaging/linux/source-package.py snapshot --root "$root" --destination "$project" > "$work/snapshot-result.json"
source_commit="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["source_commit"])' "$work/snapshot-result.json")"
source_dirty="$(python3 -c 'import json,sys; print(str(json.load(open(sys.argv[1]))["source_dirty"]).lower())' "$work/snapshot-result.json")"
source_tree_sha256="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["source_tree_sha256"])' "$work/snapshot-result.json")"
# Use the lock values from the copied source snapshot for every download/build.
. "$project/packaging/linux/sources.lock"
media_key="$(sha256sum "$project/packaging/linux/sources.lock" "$project/packaging/linux/Dockerfile" "$project/scripts/dist-linux-container.sh" | sha256sum | cut -d' ' -f1)"
prefix="$root/target/linux-dist-media/$media_key-${image_key:-unrecorded}/prefix"
mkdir -p "$prefix"
python3 packaging/linux/source-package.py vendor "$project"

fetch() {
  local archive="$1" url="$2" expected="$3"
  local path="$downloads/$archive"
  if [ -f "$project/third-party-sources/$archive" ]; then
    cp "$project/third-party-sources/$archive" "$path"
    printf '%s  %s\n' "$expected" "$path" | sha256sum --check --status || {
      echo "error: SHA-256 mismatch in corresponding source archive: $archive" >&2
      exit 1
    }
  else
    if [ ! -f "$path" ] || ! printf '%s  %s\n' "$expected" "$path" | sha256sum --check --status; then
      curl --fail --silent --show-error --location --retry 5 --retry-all-errors --connect-timeout 20 --max-time 600 "$url" --output "$path.tmp"
      printf '%s  %s\n' "$expected" "$path.tmp" | sha256sum --check --status || {
        echo "error: SHA-256 mismatch for $archive" >&2
        exit 1
      }
      mv "$path.tmp" "$path"
    fi
  fi
  printf '%s  %s\n' "$expected" "$path" | sha256sum --check --status || {
    echo "error: SHA-256 mismatch for $archive" >&2
    exit 1
  }
  cp "$path" "$src/$archive"
}

fetch "$FFMPEG_ARCHIVE" "$FFMPEG_URL" "$FFMPEG_SHA256"
fetch "$X264_ARCHIVE" "$X264_URL" "$X264_SHA256"
fetch "$LIBVPX_ARCHIVE" "$LIBVPX_URL" "$LIBVPX_SHA256"
fetch "$OPUS_ARCHIVE" "$OPUS_URL" "$OPUS_SHA256"

# Keep codec implementation libraries static inside libavcodec. The package
# then carries the FFmpeg shared ABI that rusty_ffmpeg links, while avoiding
# separate runtime dependencies on x264/libvpx/Opus.
mkdir -p "$src/x264" "$src/libvpx"
tar -xzf "$src/$X264_ARCHIVE" --strip-components=1 -C "$src/x264"
tar -xzf "$src/$LIBVPX_ARCHIVE" --strip-components=1 -C "$src/libvpx"
tar -xzf "$src/$OPUS_ARCHIVE" -C "$src"

jobs="${ANIMATIX_BUILD_JOBS:-8}"
export CFLAGS="${CFLAGS:-} -fPIC"
export CXXFLAGS="${CXXFLAGS:-} -fPIC"

tar -xJf "$src/$FFMPEG_ARCHIVE" -C "$src"
if [ ! -f "$prefix/.complete" ]; then
(
  cd "$src/x264"
  ./configure --prefix=/usr --enable-static --disable-shared --enable-pic --disable-cli
  make -j"$jobs"
  make DESTDIR="$prefix" install
)
(
  cd "$src/libvpx"
  ./configure --prefix=/usr --target=x86_64-linux-gcc --enable-vp8 --enable-vp9 \
    --enable-static --disable-shared --enable-pic --disable-examples --disable-tools \
    --disable-docs --disable-unit-tests
  make -j"$jobs"
  make DESTDIR="$prefix" install
)
(
  cd "$src/opus-1.5.2"
  ./configure --prefix=/usr --enable-static --disable-shared --with-pic
  make -j"$jobs"
  make DESTDIR="$prefix" install
)

(
  cd "$src/ffmpeg-$FFMPEG_VERSION"
  export PKG_CONFIG_LIBDIR="$prefix/usr/lib/pkgconfig:$prefix/usr/lib64/pkgconfig"
  export LIBRARY_PATH="$prefix/usr/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"
  export C_INCLUDE_PATH="$prefix/usr/include/opus:$prefix/usr/include${C_INCLUDE_PATH:+:$C_INCLUDE_PATH}"
  ./configure --prefix=/usr --enable-gpl --enable-libx264 --enable-libvpx \
    --enable-libopus --enable-shared --disable-static --enable-pic --disable-doc \
    --disable-ffplay --pkg-config-flags=--static
  make -j"$jobs"
  make DESTDIR="$prefix" install
  make distclean
)
touch "$prefix/.complete"
fi

# The dedicated target directory guarantees rusty_ffmpeg's build script cannot
# reuse bindings from the Nix/host FFmpeg installation. Both variables are
# explicit and rusty_ffmpeg checks them as rebuild inputs; dynamic mode is
# selected because the distribution carries FFmpeg's shared libraries.
export FFMPEG_INCLUDE_DIR="$prefix/usr/include"
export FFMPEG_LIBS_DIR="$prefix/usr/lib"
export FFMPEG_LINK_MODE=dynamic
unset FFMPEG_PKG_CONFIG_PATH FFMPEG_BINDING_PATH
export LIBCLANG_PATH=/usr/lib/llvm-14/lib
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$root=/usr/src/animatix"
export CARGO_NET_OFFLINE=true
before_build_tree_sha256="$(python3 "$project/packaging/linux/source-package.py" digest "$project")"
[ "$before_build_tree_sha256" = "$source_tree_sha256" ] || {
  echo 'error: copied Linux source snapshot identity does not match the build record' >&2
  exit 1
}
cargo build --offline --locked --jobs "$jobs" --profile dist --target-dir "$target" \
  --manifest-path "$project/Cargo.toml" \
  -p animatix-cli -p animatix-gui --features video
if [ "$(python3 "$project/packaging/linux/source-package.py" digest "$project")" != "$before_build_tree_sha256" ]; then
  echo 'error: source tree changed during the Linux distribution build' >&2
  exit 1
fi

stage="$work/package"
mkdir -p "$stage/bin" "$stage/libexec" "$stage/lib" \
  "$stage/share/licenses/ffmpeg" "$stage/share/licenses/x264" \
  "$stage/share/licenses/libvpx" "$stage/share/licenses/opus" \
  "$stage/share/licenses/animatix" "$stage/share/sources"
install -m 0755 "$target/dist/animatix" "$stage/libexec/animatix"
install -m 0755 "$target/dist/animatix-gui" "$stage/libexec/animatix-gui"
# Desktop identity matches the GUI viewport's app_id / X11 WM_CLASS.
install -Dm 0644 "$project/packaging/linux/io.github.j4ger.animatix.desktop" "$stage/share/applications/io.github.j4ger.animatix.desktop"
install -Dm 0644 "$project/logo.svg" "$stage/share/icons/hicolor/scalable/apps/io.github.j4ger.animatix.svg"
install -Dm 0644 "$project/crates/animatix-gui/assets/icon.png" "$stage/share/icons/hicolor/256x256/apps/io.github.j4ger.animatix.png"
install -Dm 0644 "$project/crates/animatix-gui/assets/icon-512.png" "$stage/share/icons/hicolor/512x512/apps/io.github.j4ger.animatix.png"
install -m 0755 "$project/packaging/linux/install-desktop.py" "$stage/install-desktop.py"
install -m 0755 "$prefix/usr/bin/ffmpeg" "$stage/bin/ffmpeg"
install -m 0755 "$prefix/usr/bin/ffprobe" "$stage/bin/ffprobe"
cp -a "$prefix"/usr/lib/libav*.so* "$prefix"/usr/lib/libsw*.so* "$stage/lib/"

record_system_license() {
  local path="$1" owner package
  owner="$(dpkg-query -S "$path" 2>/dev/null || dpkg-query -S "$(readlink -f "$path")" 2>/dev/null || true)"
  [ -n "$owner" ] || return 0
  package="${owner%%:*}"
  if [ -f "/usr/share/doc/$package/copyright" ]; then
    mkdir -p "$stage/share/licenses/system/$package"
    cp "/usr/share/doc/$package/copyright" "$stage/share/licenses/system/$package/copyright"
    dpkg-query -W -f='${binary:Package}\t${Version}\t${source:Package}\t${source:Version}\n' "$package" \
      > "$stage/share/licenses/system/$package/source-package.txt"
  fi
}

# Desktop backends use dlopen, so treat their client libraries as explicit
# runtime roots. GPU loader/ICD/vendor libraries remain host-provided.
for soname in libX11.so.6 libX11-xcb.so.1 libXcursor.so.1 libXi.so.6 \
  libXrandr.so.2 libXinerama.so.1 libXfixes.so.3 libXrender.so.1 libXss.so.1 \
  libxkbcommon.so.0 libxkbcommon-x11.so.0 libwayland-client.so.0 \
  libwayland-cursor.so.0 libwayland-egl.so.1; do
  path="$(ldconfig -p | awk -v soname="$soname" '$1 == soname && !found { print $NF; found=1 }')"
  [ -f "$path" ] || { echo "error: missing dlopen runtime root: $soname" >&2; exit 1; }
  cp -L "$path" "$stage/lib/$soname"
  record_system_license "$path"
done

cat > "$stage/bin/animatix" <<'EOF'
#!/bin/sh
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PATH="$here:$PATH"
export PATH
exec "$here/../libexec/animatix" "$@"
EOF
cat > "$stage/bin/animatix-gui" <<'EOF'
#!/bin/sh
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PATH="$here:$PATH"
export PATH
exec "$here/../libexec/animatix-gui" "$@"
EOF
chmod 0755 "$stage/bin/animatix" "$stage/bin/animatix-gui"

# Include corresponding sources with the package: FFmpeg is configured with
# --enable-gpl and statically incorporates x264/libvpx/Opus codec code.
install -m 0644 "$src/$FFMPEG_ARCHIVE" "$stage/share/sources/"
install -m 0644 "$src/$X264_ARCHIVE" "$stage/share/sources/"
install -m 0644 "$src/$LIBVPX_ARCHIVE" "$stage/share/sources/"
install -m 0644 "$src/$OPUS_ARCHIVE" "$stage/share/sources/"
install -m 0644 "$src/ffmpeg-$FFMPEG_VERSION/COPYING.GPLv2" "$stage/share/licenses/ffmpeg/"
install -m 0644 "$src/x264/COPYING" "$stage/share/licenses/x264/"
install -m 0644 "$src/libvpx/LICENSE" "$stage/share/licenses/libvpx/"
install -m 0644 "$src/libvpx/PATENTS" "$stage/share/licenses/libvpx/"
install -m 0644 "$src/opus-1.5.2/COPYING" "$stage/share/licenses/opus/"
install -m 0644 "$project/LICENSE" "$stage/share/licenses/animatix/LICENSE"
cp "$project/packaging/linux/README.md" "$stage/share/README-linux.txt"
cp "$project/packaging/linux/sources.lock" "$stage/share/sources.lock"

is_host_dependency() {
  case "$1" in
    libc.so.*|libm.so.*|libpthread.so.*|libdl.so.*|librt.so.*|libresolv.so.*|libutil.so.*|ld-linux*.so.*|linux-vdso.so.*|\
    libvulkan.so.*|libGL.so.*|libGLX*.so.*|libEGL.so.*|libGLES*.so.*|libOpenGL.so.*|libdrm*.so.*|libgbm.so.*|\
    libva*.so.*|libnvidia*.so.*|libcuda.so.*|libOpenCL.so.*) return 0 ;;
    *) return 1 ;;
  esac
}

# Copy the recursive ELF closure by SONAME. The only excluded libraries are
# glibc/kernel interfaces and the host's GPU/graphics driver stack.
queue="$work/elf-queue"
printf '%s\n' "$stage/libexec/animatix" "$stage/libexec/animatix-gui" "$stage/bin/ffmpeg" "$stage/bin/ffprobe" > "$queue"
find "$stage/lib" -type f -name '*.so*' -print >> "$queue"
index=1
while [ "$index" -le "$(wc -l < "$queue")" ]; do
  elf="$(sed -n "${index}p" "$queue")"
  index=$((index + 1))
  [ -f "$elf" ] || continue
  readelf -h "$elf" >/dev/null 2>&1 || continue
  while IFS=$'\t' read -r needed resolved; do
    [ -n "$needed" ] || continue
    if is_host_dependency "$needed"; then continue; fi
    [ -e "$resolved" ] || { echo "error: unresolved dependency $needed ($resolved)" >&2; exit 1; }
    dest="$stage/lib/$needed"
    if [ ! -e "$dest" ]; then
      cp -L "$resolved" "$dest"
      record_system_license "$resolved"
      printf '%s\n' "$dest" >> "$queue"
    fi
  done < <(LD_LIBRARY_PATH="$prefix/usr/lib:$stage/lib" ldd "$elf" | awk '
    /=> \/[^ ]+/ { name=$1; path=$3; print name "\t" path; next }
    /=> not found/ { print $1 "\tmissing"; next }
    /^\// { print $1 "\t" $1 }
  ')
done

# RUNPATH is relative to each ELF; do not leak build or host library paths.
for elf in "$stage/libexec/animatix" "$stage/libexec/animatix-gui" "$stage/bin/ffmpeg" "$stage/bin/ffprobe"; do
  patchelf --set-rpath '$ORIGIN/../lib' "$elf"
done
while IFS= read -r lib; do
  [ -f "$lib" ] && readelf -h "$lib" >/dev/null 2>&1 || continue
  patchelf --set-rpath '$ORIGIN' "$lib"
done < <(find "$stage/lib" -type f -name '*.so*' -print)

# Fail closed on missing libraries, absolute paths, build-store references, or
# accidental GPU driver/loader bundling. Host GPU drivers remain a runtime need.
for elf in "$stage/libexec/animatix" "$stage/libexec/animatix-gui" "$stage/bin/ffmpeg" "$stage/bin/ffprobe" "$stage"/lib/*.so*; do
  [ -f "$elf" ] && readelf -h "$elf" >/dev/null 2>&1 || continue
  if ldd "$elf" | grep -q 'not found'; then
    echo "error: unresolved dynamic dependency in $elf" >&2
    exit 1
  fi
  rpath="$(patchelf --print-rpath "$elf")"
  case "$rpath" in
    '$ORIGIN'|'$ORIGIN/../lib') ;;
    *)
    echo "error: non-relative RPATH/RUNPATH in $elf" >&2
    exit 1
    ;;
  esac
  if strings "$elf" | grep -F '/nix/store/' >/dev/null; then
    echo "error: Nix store reference found in $elf" >&2
    exit 1
  fi
  if strings "$elf" | grep -F "$root/" >/dev/null; then
    echo "error: build workspace path found in $elf" >&2
    exit 1
  fi
  python3 - "$elf" <<'PY'
import re
import subprocess
import sys

path = sys.argv[1]
header = subprocess.check_output(["readelf", "-h", path], text=True)
if "Advanced Micro Devices X86-64" not in header or "ELF64" not in header:
    raise SystemExit(f"error: wrong release architecture: {path}")
versions = subprocess.check_output(["readelf", "--version-info", path], text=True)
required = {tuple(map(int, value.split("."))) for value in re.findall(r"Name: GLIBC_([0-9.]+)", versions)}
if required and max(required) > (2, 35):
    raise SystemExit(f"error: {path} needs glibc {max(required)}, newer than Ubuntu 22.04")
PY
done
if find "$stage" -type f \( -name 'libvulkan*.so*' -o -name 'libGLX_nvidia*.so*' -o -name 'libnvidia*.so*' \) -print -quit | grep -q .; then
  echo "error: distribution must not bundle host GPU drivers or Vulkan loader" >&2
  exit 1
fi

for codec in libx264 libvpx-vp9 libopus aac; do
  "$stage/bin/ffmpeg" -hide_banner -encoders 2>/dev/null | grep -F "$codec" >/dev/null || {
    echo "error: FFmpeg package is missing required encoder $codec" >&2
    exit 1
  }
done

dpkg-query -W -f='${binary:Package}=${Version}\n' | sort > "$stage/share/build-apt-packages.txt"
cat > "$stage/share/BUILD-METADATA.txt" <<EOF
target=x86_64-unknown-linux-gnu
source_commit=$source_commit
source_dirty=$source_dirty
source_tree_sha256=$source_tree_sha256
sources_lock_sha256=$(sha256sum "$project/packaging/linux/sources.lock" | cut -d' ' -f1)
runtime_baseline=Ubuntu 22.04 x86_64 (glibc host-provided)
build_image_id=${ANIMATIX_BUILD_IMAGE_ID:-unknown}
gcc=$(gcc --version | head -1)
binutils=$(ld --version | head -1)
clang=$(clang-14 --version | head -1)
rust=$(rustc --version)
ffmpeg=$FFMPEG_VERSION
ffmpeg_sha256=$FFMPEG_SHA256
x264_revision=$X264_REV
x264_sha256=$X264_SHA256
libvpx=$LIBVPX_VERSION
libvpx_revision=$LIBVPX_REV
libvpx_sha256=$LIBVPX_SHA256
opus=$OPUS_VERSION
opus_sha256=$OPUS_SHA256
ffmpeg_link_mode=dynamic
binding_headers=FFmpeg $FFMPEG_VERSION source archive include/
ffmpeg_install_prefix=/usr (staged with DESTDIR)
EOF

version="$(sed -nE '/^\[workspace\.package\]/,/^\[/s/^version = "([^"]+)".*/\1/p' "$project/Cargo.toml" | head -1)"
source_archive="$root/${out_rel}.source.tar.gz"
python3 "$project/packaging/linux/source-package.py" package \
  --snapshot "$project" --media-dir "$src" --output "$source_archive" --version "$version"
source_archive_sha256="$(sha256sum "$source_archive" | cut -d' ' -f1)"
source_archive_bytes="$(stat -c '%s' "$source_archive")"
release_label="${ANIMATIX_RELEASE_LABEL:-v$version}"
release_source_name="animatix-${release_label}-x86_64-linux-gnu-video-source.tar.gz"
cat >> "$stage/share/BUILD-METADATA.txt" <<EOF
corresponding_source_archive=$release_source_name
corresponding_source_sha256=$source_archive_sha256
corresponding_source_size_bytes=$source_archive_bytes
EOF

(cd "$stage" && find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS)
"$stage/bin/animatix" --version
"$stage/bin/animatix-gui" --version
du -sh "$stage"
rm -rf "$out"
mv "$stage" "$out"
echo "Linux package tree ready: $out"
