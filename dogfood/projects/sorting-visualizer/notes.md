# Sorting Visualizer Dogfood Notes

Status: rebuilt 2026-08-20 (insertion sort as a timeline function), rewritten
2026-08-20 after two real bugs were found:

1. **The sort did not sort.** The fn swapped by value positions
   (`swap bars.bar[key - j]`) — but after the first swap the bars move, so
   slot indices no longer match array indices, and later swaps mis-targeted
   bars. Fixed by tracking `order` — the original bar index at each slot —
   and swapping `bars.bar[order[i]]`, comparing `values[order[i]]`.
2. **Scenes were cut short.** The composition inferred scene durations from
   keyframe tracks only, ignoring `play` times, so the Steps scene ended at
   3.0s (its last swap) instead of 4.0s. The engine now extends a scene's
   duration to its `play` statement's time.

Also fixed while rewriting:
- The Steps scene's `config` override dropped `dynamic_layout: true`, so the
  row never reordered and the swap animation was invisible.
- The fn was named `bubble_sort` while implementing insertion sort.

## What worked

- `pub component Bars(values, colors, ...)` generates namespaced `bars.bar__N`
  tracks from source-level `bars.bar[i]`.
- `[step: ...]` sequences the swaps without overlap; back-to-back swaps at
  exact boundaries are allowed by the overlap guard.
- A `Callout` retargets to the current key bar via
  `key_note.target = bars.bar[order[key]]`.
- Explicit `config { duration: N }` holds each scene.

## Language notes (build-time precomputation)

- Array state must be `let`-shadowed inside the loop; `list_swap` is the
  mutation primitive.
- Loop-bound comparisons use a fixed iteration range; data-dependent
  termination uses an `if` guard (see the DNF demo's `if i <= two`).
- Action target indices are leaf-only (`bars.bar[order[key]]`).

## Verification

```bash
cargo run --bin animatix -- check dogfood/projects/sorting-visualizer/entry.amx
cargo run --bin animatix -- image dogfood/projects/sorting-visualizer/entry.amx --time 5.6 --output /tmp/sorting-steps.png
cargo run --bin animatix -- image dogfood/projects/sorting-visualizer/entry.amx --time 8.0 --output /tmp/sorting-result.png
cargo test -p animatix --lib -- sorting_visualizer_steps_scene_sorts
```
