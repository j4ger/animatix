#!/usr/bin/env bash
# The `.amx` scene walk, in one place.
#
# `check_examples.sh` (parse/build gate) and `render_smoke.sh` (non-blank-pixel
# gate) each wrote their own `find`, and they had to stay word-for-word equal —
# an example visible to one gate and not the other means one of them is green
# about a file the other never looked at. The interesting part is not the `find`,
# it is the multi-file-project rule that follows it.
#
# Usage:
#   . scripts/lib-scenes.sh
#   scene_files                 # newline-separated absolute paths
#   scene_files "$dir"          # walk somewhere other than examples/

# Every top-level scene in the repo, in sorted order.
scene_files() {
  local root="${1:-}" amx dir base
  if [ -z "$root" ]; then
    root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/examples"
  fi
  while read -r amx; do
    dir=$(dirname "$amx")
    base=$(basename "$amx")
    # Multi-file projects (e.g. gallery/brand_reel/) are driven through their
    # main.amx, which builds the whole composition. The scene fragments they
    # import reference persisted actors and cross-file context that a per-file
    # check cannot see, so checking one on its own only produces false failures.
    if [ -f "$dir/main.amx" ] && [ "$base" != "main.amx" ]; then
      continue
    fi
    printf '%s\n' "$amx"
  done < <(find "$root" -name '*.amx' -not -path '*/lib/*' -not -path '*/scenes/*' | sort)
}
