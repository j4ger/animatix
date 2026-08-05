# Plan: Port LeetCodeAnimation topics to .amx + probe language gaps

Source: planner output. 3 algorithmic animations ported from ../LeetCodeAnimation/.

## Files
- `examples/leetcode_sort_colors.amx` — LeetCode 75 (Dutch national flag)
- `examples/leetcode_reverse_linked_list.amx` — LeetCode 206 (iterative reversal)
- `examples/leetcode_climbing_stairs.amx` — LeetCode 70 (DP recurrence)

## Probes (per file)
- Sort Colors: `swap` on Row children; for-loop gen with index arithmetic in `at`; pointer labels with no follow primitive.
- Reverse LL: animating Arrow `from`/`to` to flip link direction (highest risk); no actor-ref connectors; no list-of-actor-refs.
- Climbing Stairs: `draw-in` on Rect; `text` assignment re-render; repeated `from`/`to` re-animation; no looping timeline.

## Cross-cutting gap hypotheses (G1–G8)
- G1 No first-class list/array of actor references at runtime.
- G2 `swap`/`reorder` only on layout containers, not arbitrary actor sets.
- G3 No conditional/branching timeline; no runtime mutable state → algorithms hand-unrolled.
- G4 No runtime-indexed actor targeting (`bars[i].color` with runtime i).
- G5 No attach/follow/lookat; resolved child positions not readable.
- G6 Arrow endpoints are free Vec2, not actor refs (only Callout has target).
- G7 Text morphing (string→string tween) unsupported; assignment hard-swaps.
- G8 Heterogeneous tuples rejected in for-loops (2-tuple inferred as Vec2).

## Verification
Run `cargo run -p animatix -- ...` (or whatever the parse/render CLI is) on each file; collect diagnostics. Cross-check against hypotheses.
