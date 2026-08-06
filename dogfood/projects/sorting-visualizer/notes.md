# Sorting Visualizer Dogfood Notes

Status: second pass. The first-pass findings were fixed, the project now uses
the idiomatic forms, and check/lint/render pass.

## What worked

- `for v, i in {4, 1, 3, 2}` inside a `Row` generates real internal `bar__N`
  tracks while source continues to use `bar[i]`.
- `fade-in bar[0]`, `swap bar[0], bar[1]`, and `bar[i].scale` in `always`
  parse and build with indexed targets.
- A `Callout` can target a generated array actor (`target: bar[1]`) and can be
  retargeted by source-level array reference (`key_note.target = bar[2]`).
- `import "../../../examples/lib/tokens.amx" as tokens` gives namespaced token
  access across the dogfood project.
- Multi-scene `play` transitions work across the project.

## Fixed findings

- Indexed reactive targets no longer emit `ModifierCompilationError`; the
  bytecode VM now supports them natively.
- Callout retargeting now accepts bare actor labels, not just strings.
- The linter counts `Callout.target` references as label usage.
- Block-style `Callout { ... }` properties are now parsed into actor props
  instead of being silently dropped.

## Remaining design signal

- The algorithm itself is still hand-unrolled into keyframes. There is no
  runtime mutable array or branching timeline, so this is a recording of one
  insertion sort pass, not the algorithm running live.
- A reusable `Bars` component is now viable at runtime, and the analyzer
  recognizes component types, actions, and generated array labels. The
  remaining linter gap is false `unused-label` on component-internal template
  actors. See probe 004.

## Next dogfood candidates

- Make sorting visualizer use the reusable `Bars` component after the analyzer
  component-awareness gap is addressed.
- Probe `List<Color>` inference for named colors in lists.

## Verification

```bash
cargo run --bin animatix -- check dogfood/projects/sorting-visualizer/entry.amx
cargo run --bin animatix -- lint dogfood/projects/sorting-visualizer/entry.amx
cargo run --bin animatix -- image dogfood/projects/sorting-visualizer/entry.amx --time 0.3 --output /tmp/sorting-title.png
cargo run --bin animatix -- image dogfood/projects/sorting-visualizer/entry.amx --time 3.0 --output /tmp/sorting-steps.png
cargo run --bin animatix -- image dogfood/projects/sorting-visualizer/entry.amx --time 8.5 --output /tmp/sorting-result.png
```
