#!/usr/bin/env bash
# Runs inside a clean Ubuntu runtime container. Only host graphics and
# inspection tools are installed; FFmpeg itself must come from /package.
set -euo pipefail

# Persist the actual clean-host outputs as release acceptance evidence.
if [ -n "${EVIDENCE_DIR:-}" ]; then
  mkdir -p "$EVIDENCE_DIR"
  exec > >(tee "$EVIDENCE_DIR/runtime.log") 2>&1
  cp /etc/os-release "$EVIDENCE_DIR/os-release"
  trap 'status=$?; if [ -d /tmp/animatix-linux-package-test ]; then cp -a /tmp/animatix-linux-package-test/. "$EVIDENCE_DIR/"; fi; printf "%s\n" "$status" > "$EVIDENCE_DIR/exit-status"' EXIT
fi

if [ ! -x /package/bin/animatix ] || [ ! -x /package/bin/animatix-gui ] || [ ! -x /package/bin/ffmpeg ] || [ ! -x /package/bin/ffprobe ]; then
  echo "error: incomplete Linux package tree" >&2
  exit 1
fi

apt-get update
apt-get install -y --no-install-recommends libvulkan1 mesa-vulkan-drivers xvfb xauth x11-utils
rm -rf /var/lib/apt/lists/*

mkdir -p /tmp/xdg-runtime
chmod 0700 /tmp/xdg-runtime
export XDG_RUNTIME_DIR=/tmp/xdg-runtime
export WGPU_BACKEND=vulkan
icd="$(find /usr/share/vulkan/icd.d -maxdepth 1 -name 'lvp_icd*.json' -print -quit)"
[ -n "$icd" ] || { echo "error: host test image has no lavapipe ICD" >&2; exit 1; }
export VK_ICD_FILENAMES="$icd"

# Confirm the executable resolves FFmpeg from the package and uses no Nix path.
[ ! -x /usr/bin/ffmpeg ] || { echo 'error: clean runtime image unexpectedly has FFmpeg' >&2; exit 1; }
[ ! -d /nix ] || { echo 'error: clean runtime image unexpectedly has Nix' >&2; exit 1; }
[ ! -x /usr/bin/rustc ] || { echo 'error: clean runtime image unexpectedly has Rust' >&2; exit 1; }
(cd /package && sha256sum --check --status SHA256SUMS)
test -s /package/share/licenses/animatix/LICENSE || {
  echo 'error: Animatix license is missing from the package' >&2
  exit 1
}
metadata=/package/share/BUILD-METADATA.txt
grep -Eq '^source_commit=[0-9a-f]{40}$' "$metadata"
grep -Eq '^source_dirty=(true|false)$' "$metadata"
grep -Eq '^source_tree_sha256=[0-9a-f]{64}$' "$metadata"
if grep -F '/workspace/' "$metadata" >/dev/null; then
  echo 'error: build metadata contains a workspace path' >&2
  exit 1
fi
"/package/bin/animatix" --version
"/package/bin/animatix-gui" --version
ldd /package/libexec/animatix | grep -E 'libavcodec[^ ]* => /package/.*/lib/libavcodec' >/dev/null
if grep -R -a -l '/nix/store/' /package/libexec /package/lib /package/bin/ffmpeg >/dev/null; then
  echo "error: Nix store reference found in release package" >&2
  exit 1
fi

testdir=/tmp/animatix-linux-package-test
mkdir -p "$testdir"
cd "$testdir"
# Every media operation uses the package; no system codec library can hide a
# missing dependency in the bundled closure.
/package/bin/ffmpeg -hide_banner -loglevel error -f lavfi \
  -i 'sine=frequency=440:duration=1.2' -ar 44100 -ac 2 -c:a pcm_s16le tone.wav
cat > scene.amx <<'EOF'
config { duration: 1.0, resolution: (320, 180) }
sound: Audio, source: "tone.wav", volume: 1.0
#0s
square: Rect, size: (100, 100), color: "#e74c3c", at: (160, 90)
fade-in square [200ms]
EOF

/package/bin/animatix video scene.amx --duration 1 --fps 5 --width 320 --height 180 \
  --threads 1 --codec libx264 --output out.mp4
/package/bin/animatix video scene.amx --duration 1 --fps 5 --width 320 --height 180 \
  --threads 1 --codec vp9 --output out.webm
/package/bin/animatix gif scene.amx --duration 1 --fps 5 --width 320 --height 180 \
  --threads 1 --output out.gif

assert_stream() {
  local file="$1" stream="$2" codec="$3" found
  found=$(/package/bin/ffprobe -v error -select_streams "$stream:0" \
    -show_entries stream=codec_name -of default=noprint_wrappers=1:nokey=1 "$file")
  [ "$found" = "$codec" ] || {
    echo "error: $file expected $stream/$codec, found '$found'" >&2
    exit 1
  }
}
assert_stream out.mp4 v h264
assert_stream out.mp4 a aac
assert_stream out.webm v vp9
assert_stream out.webm a opus
assert_stream out.gif v gif
for f in out.mp4 out.webm out.gif; do
  [ -s "$f" ] || { echo "error: empty output: $f" >&2; exit 1; }
done

# A version flag does not start eframe or create a preview. Verify a real GUI
# window under the clean host's virtual display, then close the test process.
xvfb-run -a sh -eu -c '
  /package/bin/animatix-gui scene.amx >gui.log 2>&1 &
  gui_pid=$!
  trap "kill $gui_pid 2>/dev/null || true" EXIT INT TERM
  found=false
  for attempt in $(seq 1 40); do
    kill -0 "$gui_pid" 2>/dev/null || { cat gui.log >&2; exit 1; }
    xwininfo -root -tree > gui-window-tree.txt
    if grep -E "Animatix|scene.amx" gui-window-tree.txt >/dev/null; then found=true; break; fi
    sleep 0.5
  done
  [ "$found" = true ] || { cat gui.log >&2; echo "GUI did not create a window" >&2; exit 1; }
  kill "$gui_pid"
  wait "$gui_pid" || true
'

dpkg-query -W -f='${binary:Package}=${Version}\n' | sort > runtime-apt-packages.txt
/package/bin/ffprobe -v error -show_streams -show_format -of json out.mp4 > mp4-probe.json
/package/bin/ffprobe -v error -show_streams -show_format -of json out.webm > webm-probe.json
/package/bin/ffprobe -v error -show_streams -show_format -of json out.gif > gif-probe.json
echo "Ubuntu $(. /etc/os-release && echo "$VERSION_ID"): native package rendered and encoded MP4/AAC, WebM/Opus, and GIF."
