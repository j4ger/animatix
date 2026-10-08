#!/usr/bin/env bash
# Extract keyframe timestamps from an .amx scene and render frames to images.
#
# Follows the review pipeline described in docs/handoff_web_redesign.md:
# Renders at each `#N.Ns` keyframe timestamp + 0.45s (settled beat),
# plus the initial (0s) and final resting frames.
#
# Usage:
#   scripts/render-keyframes.sh <scene.amx> [out-dir]
#
set -euo pipefail

scene="${1:?Usage: $0 <scene.amx> [out-dir]}"
out_dir="${2:-/tmp/frames/$(basename "$scene" .amx)}"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

# Find or build animatix CLI binary
bin=""
if [ -x "target/release/animatix" ]; then
  bin="target/release/animatix"
elif [ -x "target/debug/animatix" ]; then
  bin="target/debug/animatix"
else
  echo "Building animatix CLI..."
  cargo build -p animatix-cli
  bin="target/debug/animatix"
fi

mkdir -p "$out_dir"
rm -f "$out_dir"/*.png

# 1. Extract total duration from timeline command or file header
duration=""
if timeline_out=$("$bin" timeline "$scene" 2>/dev/null); then
  duration=$(echo "$timeline_out" | grep -oE "total:\s*[0-9.]+s" | grep -oE "[0-9.]+" || true)
fi
if [ -z "$duration" ]; then
  duration=$(grep -oE "duration:\s*[0-9.]+" "$scene" | head -1 | grep -oE "[0-9.]+" || echo "0")
fi

# 2. Extract keyframe timestamps `#N.Ns` or `#Ns` from source
stamps=()
stamps+=(0.0)

while IFS= read -r stamp; do
  if [ -n "$stamp" ]; then
    stamps+=("$stamp")
    # Add +0.45s post-arrival beat if within duration
    settled=$(awk -v t="$stamp" 'BEGIN { printf "%.2f", t + 0.45 }')
    if (( $(awk -v s="$settled" -v d="${duration:-999}" 'BEGIN { print (s < d) }') )); then
      stamps+=("$settled")
    fi
  fi
done < <(grep -oE '#[0-9]+(\.[0-9]+)?s' "$scene" | tr -d '#s' | sort -n -u)

# Add end/rest frame if duration > 0
if [ -n "$duration" ] && (( $(awk -v d="$duration" 'BEGIN { print (d > 0) }') )); then
  stamps+=("$duration")
fi

# Deduplicate and sort timestamps
sorted_stamps=($(printf "%s\n" "${stamps[@]}" | sort -n -u))

echo "Rendering ${#sorted_stamps[@]} frame(s) for $scene into $out_dir/..."
idx=0
for t in "${sorted_stamps[@]}"; do
  out_file=$(printf "%s/frame_%03d_t%06.2fs.png" "$out_dir" "$idx" "$t")
  "$bin" image "$scene" --time "$t" -o "$out_file" >/dev/null 2>&1 || {
    echo "warning: failed to render frame at t=$t" >&2
  }
  idx=$((idx + 1))
done

count=$(find "$out_dir" -maxdepth 1 -name "frame_*.png" | wc -l)
echo "✓ Rendered $count frame(s) in $out_dir."
