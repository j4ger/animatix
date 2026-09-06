# Probe: plot closure capturing an always-written let never re-samples

Status: **resolved 2026-09-06** — the dynamic gate now accounts for
captures written by `always` blocks (see Resolution).

## Intent

spec.md §14 "Runtime parameters" documents this exact pattern: sweep a plot
parameter from an `always` block so the curve animates. Math explainers need
this for frequency/amplitude/degree knobs.

## Minimal Repro

```animatix
#0s
let freq = 2
curve: PlotCurve, kind: "cartesian", func: (x) => sin(freq * x), ...
always {
  freq = 2 + 3 * sin(t * 0.5)
}
```

Expected: the curve's frequency sweeps over time. Observed: static — renders
at t=0.3 and t=5.0 are pixel-identical.

## Expected DSL

The spec §14 pattern as written should animate.

## Current Workaround

Inline the `t`-expression into the closure body:
`func: (x) => sin((2 + 3 * sin(t * 0.5)) * x)`. Works (frame-sampled), but
does not scale to complex knobs and is not what the spec teaches.

## Diagnostics / Behavior

`animatix check` is clean — nothing tells the author the capture is dead.
Root-cause direction: `ProceduralPlot::is_dynamic()`
(crates/animatix/src/timeline/plot.rs:1132) is
`func_body.references_ident("t") || !param_names.is_empty()`; a capture-only
plot is classified static, the cached build-time `vector_paths` are reused
every frame, and the frame-env shadowing path (scene_eval.rs:522+) is never
reached. The shadowing machinery itself works once sampling fires (verified:
an inline-`t` body animates).

Spelling matrix (pixel-verified 2026-09-06):

| Spelling | Animates? |
|---|---|
| `t` inside the closure body | yes |
| `let freq` + `always { freq = ... }` (this repro) | **no** |
| declared `freq: 2` + `always { freq = ... }` | no (same shape) |

## Impact

Anyone following the spec's reactive-plot recipe ships a static chart. Silent.

## Recommendation

Make the dynamic gate account for `extra_captures` that an `always` block in
the same scene writes (build-time AST scan, cf. `referenced_roots`), so the
existing per-frame shadowing kicks in. Tracked as Stage B of the roadmap fix
pass (2026-09-06). Regression: two render times of this repro must differ.

## Resolution

`Timeline::collect_frame_written_vars` scans the lowered `always` statements
at the end of the build (bare assignments + `let`s, walking if/for) into
`Timeline::frame_written_vars`; `ProceduralPlot::is_dynamic` now also fires
when `extra_captures` intersect that set. The per-frame shadowing machinery
needed no change. Pixel-verified: the repro's frequency now sweeps (t=0.3 vs
t=5.0 differ); regression test
`plot_capture_of_always_written_var_is_dynamic` pins the gate. The
inline-`t` spelling continues to work unchanged.
