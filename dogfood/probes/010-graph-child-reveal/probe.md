# Probe: container fade-in never reveals Graph-hosted PlotCurve children

Status: open — fix planned (roadmap "Planned: Dogfood-Driven Fix Pass", Stage A).

## Intent

The canonical "reveal a chart" pattern: declare a `Graph` with a hosted
`PlotCurve` before the first keyframe and fade the graph in. This is exactly
what the shipped example `examples/data/07_plots.amx` does.

## Minimal Repro

```animatix
g: Graph, x_domain: (-pi, pi), y_domain: (-1.8, 1.8), size: (400, 300), anchor: scene.center {
  c: PlotCurve, kind: "cartesian", func: (x) => sin(x), color: accent.primary, stroke_width: 4
}
#0.3s
fade-in g [300ms]
```

Expected: after 0.6s the curve is visible. Observed: axes render, the curve is
invisible forever, and **no diagnostic fires** (no `never-revealed`, no
warning).

## Expected DSL

Same source; the entrance action on the container should reveal the subtree,
matching the accepted group-target `fade-in cards` behavior for layout
containers (dogfood run/003).

## Current Workaround

Fade the curve itself (`fade-in c`), or give the child an explicit
`opacity:` declaration — which *bypasses* hidden-by-default entirely, so
`opacity: 0` + per-child opacity assignments is the only way to sequence
multiple hosted curves. Two inconsistent "start invisible" semantics.

## Diagnostics / Behavior

- `animatix check`: clean (no diagnostics at all).
- `animatix image --time 1.5`: axes visible, curve absent (pixel-verified).
- `fade-in c` instead: curve renders (so the lift machinery works when
  targeted at the leaf).
- `examples/data/07_plots.amx` at t=3.0/6.0: the headline sine curve is
  missing; the top-level VectorField/Heatmap reveal fine.

## Impact

Every "declare graph + curves up front, fade in the chart" author lands here.
Silent: the build is green, the export just lacks the curve.

## Recommendation

Cascade `lift_hidden_by_default` into Graph children when a container
entrance action runs (parity with layout-container group fade-in), and extend
the `never-revealed` diagnostic to Graph children so a future regression is
loud. Tracked as Stage A of the roadmap fix pass (2026-09-06).
