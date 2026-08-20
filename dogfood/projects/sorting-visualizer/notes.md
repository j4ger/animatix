# Sorting Visualizer Dogfood Notes

Status: rebuilt 2026-08-20 with build-time algorithm precomputation, then
2026-08-20 refactored into a timeline function `fn bubble_sort(bars, values)`.
The sort loop runs at build time inside the function: `let` shadowing carries
the array state (`list_swap`), leaf expression-indexed swap targets
(`swap bars.bar[key - j], bars.bar[key - j - 1]`) resolve against the loop
variables, and `[step: ...]` modifiers advance the build-time clock so each
swap lands on its own keyframe. No hand-unrolled keyframes remain. The
function body expands into a scoped block, so its local `let` bindings never
leak into the scene.

## What worked

- `pub component Bars(values: List<Num>, colors: List<Color>, ...)` generates
  real namespaced `bars.bar__N` tracks from source-level `bars.bar[i]`.
- The same component is instantiated in two scenes with different data/colors.
- `tokens.*` paths work as `List<Color>` values.
- Variable-index action targets: `swap bars.bar[key - j], bars.bar[key - j - 1]`
  resolves `key - j` against the build environment.
- `[step: 1200ms]` on the outer key loop and `[step: 300ms]` on the inner
  compare loop sequence the swaps without overlap (the swap overlap guard
  allows back-to-back swaps at exact boundaries).
- A `Callout` retargets to the current key bar via `key_note.target = bars.bar[key]`.
- Multi-scene `play` transitions work across the project.

## Language notes (build-time precomputation)

- Array state must be `let`-shadowed inside the loop; `list_swap` is the
  mutation primitive and the shadowed binding is visible to later statements.
- Loop-bound comparisons use a fixed iteration range (`for key in {1,2,3}`);
  there is no `while`, so data-dependent termination uses an `if` guard
  (the DNF demo guards with `if i <= two`).
- Action target indices are leaf-only (`bars.bar[key]`); indexing a non-leaf
  path segment is rejected by the parser.

## Verification

```bash
cargo run --bin animatix -- check dogfood/projects/sorting-visualizer/entry.amx
cargo run --bin animatix -- lint dogfood/projects/sorting-visualizer/entry.amx
cargo run --bin animatix -- image dogfood/projects/sorting-visualizer/entry.amx --time 0.3 --output /tmp/sorting-title.png
cargo run --bin animatix -- image dogfood/projects/sorting-visualizer/entry.amx --time 3.0 --output /tmp/sorting-steps.png
cargo run --bin animatix -- image dogfood/projects/sorting-visualizer/entry.amx --time 8.5 --output /tmp/sorting-result.png
```
