#!/usr/bin/env bash
# Extract keyframe timestamps from an .amx scene and render frames to images.
#
# Part of the web review pipeline (see docs/history.md, "The instrument redesign"):
# Renders at each `#N.Ns` keyframe timestamp + 0.45s (settled beat),
# plus the initial (0s) and final resting frames.
#
# Usage:
#   scripts/render-keyframes.sh <scene.amx> [out-dir] [--force]
#
set -euo pipefail

usage() { echo "Usage: $0 <scene.amx> [out-dir] [--force]" >&2; }
scene=""
out_dir=""
force=0
if [ $# -lt 1 ]; then usage; exit 2; fi
scene="$1"; shift
if [ $# -gt 0 ]; then
  case "$1" in
    --force) force=1; shift ;;
    *) out_dir="$1"; shift ;;
  esac
fi
if [ $# -gt 0 ]; then
  case "$1" in
    --force) force=1; shift ;;
    *) usage; exit 2 ;;
  esac
fi
[ $# -eq 0 ] || { usage; exit 2; }
out_dir="${out_dir:-/tmp/frames/$(basename "$scene" .amx)}"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

# This script removes frame_*.png from its output directory. Refuse values that
# could mean "the repository", "my home directory" or a system directory; the
# explicit --force covers a custom location outside the repo/TMPDIR, and never
# overrides those hard refusals.
case "$out_dir" in
  ""|.|./|..|../|/) echo "error: refusing output directory '$out_dir'" >&2; exit 2 ;;
esac
case "/$out_dir/" in
  */../*) echo "error: refusing output directory with a '..' component: $out_dir" >&2; exit 2 ;;
esac
out_abs="$(realpath -m -- "$out_dir")"
root_abs="$(realpath -m -- "$root")"
home_abs="$(realpath -m -- "${HOME:-/nonexistent}")"
tmp_base="$(realpath -m -- "${TMPDIR:-/tmp}")"

for sys_dir in / /bin /boot /dev /etc /home /lib /lib32 /lib64 /media /mnt /opt /proc /root /run /sbin /srv /sys /usr /var /tmp; do
  if [ "$out_abs" = "$sys_dir" ]; then
    echo "error: refusing to use system directory '$out_abs' as an output directory" >&2
    exit 2
  fi
done
if [ "$out_abs" = "$root_abs" ]; then
  echo "error: refusing to use the repository root '$out_abs' as an output directory" >&2
  exit 2
fi
case "$root_abs" in
  "$out_abs"/*)
    echo "error: refusing '$out_abs' — it is an ancestor of the repository ($root_abs)" >&2
    exit 2
    ;;
esac
if [ "$out_abs" = "$home_abs" ]; then
  echo "error: refusing to use the home directory '$out_abs' as an output directory" >&2
  exit 2
fi
if [ "$force" -eq 0 ]; then
  case "$out_abs" in
    "$root_abs"/*|"$tmp_base"/*) : ;;
    *)
      echo "error: refusing '$out_abs' — output must be inside the repository or $tmp_base," >&2
      echo "       or pass --force to acknowledge a custom directory." >&2
      exit 2
      ;;
  esac
fi

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

# Render into a fresh temporary directory first so only complete frames reach
# the output directory; the trap removes the staging area on interruption.
stage="$(mktemp -d "${TMPDIR:-/tmp}/animatix-keyframes.XXXXXX")"
trap 'rm -rf -- "${stage:-}"' EXIT

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
  out_file=$(printf "%s/frame_%03d_t%06.2fs.png" "$stage" "$idx" "$t")
  "$bin" image "$scene" --time "$t" -o "$out_file" >/dev/null 2>&1 || {
    echo "warning: failed to render frame at t=$t" >&2
  }
  idx=$((idx + 1))
done

mkdir -p "$out_dir"
# Only this script's output pattern is replaced; unrelated PNGs are left alone.
find "$out_dir" -maxdepth 1 -type f -name 'frame_*.png' -delete
shopt -s nullglob
rendered=("$stage"/frame_*.png)
if [ "${#rendered[@]}" -gt 0 ]; then
  mv -- "${rendered[@]}" "$out_dir"/
fi
count=$(find "$out_dir" -maxdepth 1 -name 'frame_*.png' | wc -l | tr -d ' ')
echo "✓ Rendered $count frame(s) in $out_dir."
