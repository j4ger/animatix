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
apt-get install -y --no-install-recommends libvulkan1 mesa-vulkan-drivers xvfb xauth x11-utils python3
rm -rf /var/lib/apt/lists/*

mkdir -p /tmp/xdg-runtime
chmod 0700 /tmp/xdg-runtime
export XDG_RUNTIME_DIR=/tmp/xdg-runtime
export WGPU_BACKEND=vulkan
icd="$(find /usr/share/vulkan/icd.d -maxdepth 1 -name 'lvp_icd*.json' -print -quit)"
[ -n "$icd" ] || { echo "error: host test image has no lavapipe ICD" >&2; exit 1; }
export VK_ICD_FILENAMES="$icd"

# Confirm the application links the bundled FFmpeg libraries and uses no Nix path.
[ ! -x /usr/bin/ffmpeg ] || { echo 'error: clean runtime image unexpectedly has FFmpeg' >&2; exit 1; }
[ ! -d /nix ] || { echo 'error: clean runtime image unexpectedly has Nix' >&2; exit 1; }
[ ! -x /usr/bin/rustc ] || { echo 'error: clean runtime image unexpectedly has Rust' >&2; exit 1; }
(cd /package && sha256sum --check --status SHA256SUMS)
test -s /package/share/licenses/animatix/LICENSE || {
  echo 'error: Animatix license is missing from the package' >&2
  exit 1
}
test -s /package/share/applications/io.github.j4ger.animatix.desktop
test -s /package/share/icons/hicolor/scalable/apps/io.github.j4ger.animatix.svg
test -s /package/share/icons/hicolor/256x256/apps/io.github.j4ger.animatix.png
test -s /package/share/icons/hicolor/512x512/apps/io.github.j4ger.animatix.png
test -x /package/install-desktop.py
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
sound: Audio, source: "tone.wav", transcript: "tone.srt" [delay: 200ms]
subs: Captions, source: "sound", opacity: 1.0, at: (160, 145), font_size: 28, color: "#FFFFFF", caption_mode: "line", backdrop: false
#0s
square: Rect, size: (100, 100), color: "#e74c3c", at: (160, 90)
fade_in(target: square, duration: 200ms)
EOF
cat > tone.srt <<'EOF'
1
00:00:00,000 --> 00:00:00,600
CAPTION
EOF

# Give the launchers only dirname. A subprocess lookup cannot see ffmpeg,
# even though the test can use the bundled inspection tools by absolute path.
mkdir -p export-path
ln -sf "$(command -v dirname)" export-path/dirname
export_path="$PWD/export-path"

PATH="$export_path" /package/bin/animatix video scene.amx --duration 1 --fps 5 --width 320 --height 180 \
  --threads 1 --codec libx264 --output out.mp4
PATH="$export_path" /package/bin/animatix video scene.amx --duration 1 --fps 5 --width 320 --height 180 \
  --threads 1 --codec vp9 --output out.webm
PATH="$export_path" /package/bin/animatix video scene.amx --duration 1 --fps 5 --width 320 --height 180 \
  --threads 1 --codec libx264 --output out.mov
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
assert_stream out.mov v h264
assert_stream out.mov a aac
mov_brand=$(/package/bin/ffprobe -v error -show_entries format_tags=major_brand \
  -of default=noprint_wrappers=1:nokey=1 out.mov)
[ "$mov_brand" = 'qt  ' ] || { echo "error: out.mov has major brand '$mov_brand'" >&2; exit 1; }
assert_stream out.gif v gif
for f in out.mp4 out.webm out.mov out.gif; do
  [ -s "$f" ] || { echo "error: empty output: $f" >&2; exit 1; }
done

assert_video_frames() {
  local file="$1" found
  found=$(/package/bin/ffprobe -v error -select_streams v:0 -count_frames \
    -show_entries stream=nb_read_frames -of default=noprint_wrappers=1:nokey=1 "$file")
  [ "$found" = 5 ] || {
    echo "error: $file decoded $found video frames; expected 5" >&2
    exit 1
  }
  /package/bin/ffmpeg -v error -xerror -i "$file" -map 0:v:0 -an -f null -
}

assert_audio() {
  local file="$1" name="$2" frame_samples="$3" raw="$testdir/$2.pcm" decode_log="$testdir/$2-decode.log" playback_duration
  playback_duration=$(/package/bin/ffprobe -v error -show_entries format=duration \
    -of default=noprint_wrappers=1:nokey=1 "$file")
  awk -v duration="$playback_duration" 'BEGIN { exit !(duration >= 0.98 && duration <= 1.02) }' || {
    echo "error: $file playback duration is ${playback_duration}s; expected 1.0s ±0.02s" >&2
    exit 1
  }
  # Keep codec diagnostics (including the known Opus packet-header warning) as
  # evidence while requiring the decoder to finish successfully.
  /package/bin/ffmpeg -v error -xerror -i "$file" -map 0:a:0 -vn -ac 2 -ar 44100 \
    -f s16le -y "$raw" 2>"$decode_log"
  python3 - "$raw" "$name" "$playback_duration" "$frame_samples" <<'PY'
import array
import math
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
samples = array.array("h")
samples.frombytes(path.read_bytes())
if sys.byteorder != "little":
    samples.byteswap()
if not samples:
    raise SystemExit(f"{sys.argv[2]}: decoder produced no PCM samples")
rate = 44100
frames = len(samples) // 2
duration = frames / rate
sample_budget = int(sys.argv[4])
sample_error = abs(frames - rate)
if sample_error > sample_budget:
    raise SystemExit(f"{sys.argv[2]}: decoded {frames} samples per channel ({duration:.6f}s), "
                     f"{sample_error} samples from target; codec frame budget is {sample_budget}")
def rms(signal):
    return math.sqrt(sum(sample * sample for sample in signal) / len(signal))

for channel in (0, 1):
    early = samples[channel:round(0.15 * rate) * 2:2]
    if rms(early) > 100:
        raise SystemExit(f"{sys.argv[2]}: channel {channel} was audible before the 200 ms delay")
    playback = samples[round(0.3 * rate) * 2 + channel:round(0.9 * rate) * 2:2]
    if rms(playback) < 500:
        raise SystemExit(f"{sys.argv[2]}: channel {channel} is too quiet after the delay")
    crossings = sum((left < 0) != (right < 0) for left, right in zip(playback, playback[1:]))
    frequency = crossings * rate / (2 * (len(playback) - 1))
    if not 425 <= frequency <= 455:
        raise SystemExit(f"{sys.argv[2]}: decoded tone is {frequency:.1f}Hz, expected 440Hz")
print(f"{sys.argv[2]} duration {duration:.6f}s, 440 Hz after 200 ms on both channels")
PY
}

assert_video_frames out.mp4
assert_video_frames out.webm
assert_video_frames out.mov
assert_video_frames out.gif
# The locked AAC encoder emits 1024-sample frames at 44.1 kHz (23.22 ms); Opus
# emits 960 samples at 48 kHz, or 882 after the 44.1 kHz decode resample (20 ms).
# Compare container playback duration separately, then allow at most one final
# codec frame of decoder padding in raw PCM.
assert_audio out.mp4 mp4-audio 1024
assert_audio out.webm webm-audio 882
assert_audio out.mov mov-audio 1024

# Decode the caption region away from the square. The caption should appear on
# the 200 ms frame with the tone and disappear at the end of its window.
/package/bin/ffmpeg -v error -i out.mp4 -map 0:v:0 \
  -vf 'crop=240:40:40:128' -pix_fmt gray -f rawvideo -y caption.raw
python3 - <<'PY'
from pathlib import Path

data = Path("caption.raw").read_bytes()
frame_size = 240 * 40
ink = [sum(pixel > 180 for pixel in data[i:i + frame_size])
       for i in range(0, len(data), frame_size)]
if len(ink) != 5 or ink[0] or min(ink[1:4]) < 30 or ink[4]:
    raise SystemExit(f"caption ink by frame did not follow the 200–800 ms cue: {ink}")
print(f"caption ink per video frame: {ink}")
PY

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
