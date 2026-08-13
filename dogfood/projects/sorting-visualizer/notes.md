# Sorting Visualizer Dogfood Notes

Status: third pass. The Steps and Result scenes now use a reusable `Bars`
component, check/lint/render pass, and component and hand-authored frames are
pixel-identical.

## What worked

- `pub component Bars(values: List<Num>, colors: List<Color>, ...)` generates
  real namespaced `bars.bar__N` tracks from source-level `bars.bar[i]`.
- The same component is instantiated in two scenes with different data/colors.
- `tokens.*` paths work as `List<Color>` values.
- `swap bars.bar[0], bars.bar[1]` resolves against component-generated tracks.
- A `Callout` can target `bars.bar[1]` and retarget to `bars.bar[2]`.
- `import "../../../examples/lib/tokens.amx" as tokens` gives namespaced token
  access across the dogfood project.
- Multi-scene `play` transitions work across the project.

## Fixed findings

- Component expansion did not recurse into `# SceneName` bodies, so component
  instances placed in the shared prelude were not expanded in multi-scene
  files. The expander now expands scene bodies.
- Callout actor-reference parsing only accepted bare `bar[2]`, not
  component-namespaced `bars.bar[2]`. Both declaration and assignment target
  parsing now support path-plus-index references and resolve them to
  `bars.bar__2`.
- Indexed reactive targets no longer emit `ModifierCompilationError`; the
  bytecode VM now supports them natively.
- The linter counts `Callout.target` references as label usage.
- Block-style `Callout { ... }` properties are now parsed into actor props
  instead of being silently dropped.
- Structural containers with children no longer trigger `unused-label`.

## Remaining design signal

- The algorithm itself is still hand-unrolled into keyframes. There is no
  runtime mutable array or branching timeline, so this is a recording of one
  insertion sort pass, not the algorithm running live.

## Next dogfood candidates

- Probe array/group `fade-in` targets as a focused A/B run.
- Explore reusable `Bars` actions for key highlighting and swaps.

## Verification

```bash
cargo run --bin animatix -- check dogfood/projects/sorting-visualizer/entry.amx
cargo run --bin animatix -- lint dogfood/projects/sorting-visualizer/entry.amx
cargo run --bin animatix -- image dogfood/projects/sorting-visualizer/entry.amx --time 0.3 --output /tmp/sorting-title.png
cargo run --bin animatix -- image dogfood/projects/sorting-visualizer/entry.amx --time 3.0 --output /tmp/sorting-steps.png
cargo run --bin animatix -- image dogfood/projects/sorting-visualizer/entry.amx --time 8.5 --output /tmp/sorting-result.png
```
