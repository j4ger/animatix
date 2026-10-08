#!/usr/bin/env bash
# Stitch keyframe frames into a tiled contact sheet grid using ffmpeg.
#
# Follows the review pipeline described in docs/handoff_web_redesign.md:
# Generates a contact sheet per scene for visual inspection and audit.
#
# Usage:
#   scripts/contact-sheet.sh <frames-dir> [out.png] [cols]
#
# Examples:
#   scripts/contact-sheet.sh /tmp/frames/hero /tmp/sheets/hero.png
#   scripts/contact-sheet.sh /tmp/frames/hero /tmp/sheets/hero.png 6
set -euo pipefail

frames_dir="${1:?Usage: $0 <frames-dir> [out.png] [cols]}"
out_file="${2:-/tmp/sheets/$(basename "$frames_dir").png}"
cols="${3:-6}"

mkdir -p "$(dirname "$out_file")"

count=$(find "$frames_dir" -maxdepth 1 -name "frame_*.png" | wc -l)
if [ "$count" -eq 0 ]; then
  echo "error: no frame_*.png files found in $frames_dir" >&2
  exit 1
fi

rows=$(( (count + cols - 1) / cols ))
echo "Building ${cols}x${rows} contact sheet from $count frames in $frames_dir..."

ffmpeg -y -hide_banner -loglevel error \
  -pattern_type glob -i "$frames_dir/frame_*.png" \
  -vf "scale=320:-1,tile=${cols}x${rows}" \
  -update 1 \
  "$out_file"

echo "✓ Contact sheet saved: $out_file"
