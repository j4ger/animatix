# Probe: timed plot-param assignment renders a wrong curve after completion

Status: open — root-cause then fix (roadmap "Planned: Dogfood-Driven Fix Pass",
Stage B; do not assume it shares a root cause with probe 011).

## Intent

Animate a declared plot parameter to a new value — the plot-parameter analog
of an ordinary property assignment. `assignments/mod.rs:629-683` explicitly
implements keyframed `plot_param_tracks` for this.

## Minimal Repro

```animatix
curve: PlotCurve, kind: "cartesian", func: (x) => sin(freq * x), freq: 2,
  color: accent.primary, stroke_width: 3, at: (320, 180), size: (400, 300)

#0.5s
curve.freq = 5 [1s]
```

Expected at t=5.0 (assignment complete at 1.5s): `sin(5x)` — a dense wave
(≈16 periods over the default x domain). Observed: a nearly flat, gently
sloped line — neither sin(2x), nor sin(5x), nor a plausible blend of the two.

## Expected DSL

Same source; the parametrized curve should land on the target function.

## Current Workaround

None known. A timed `curve.func = (x) => sin(5 * x)` transition (function
transition instead of parameter assignment) is the reliable alternative.

## Diagnostics / Behavior

- `animatix check`: two `unknown-property: freq` infos from the analyzer
  (the documented declaration form is not in the common-property table), no
  build error.
- `animatix image --time 5.0`: near-flat sloped line (pixel-verified
  2026-09-06).
- Suspicion: `plot_param_tracks` seeding/`PropertyTrack::new(target_val)`
  plus the unconditional injection in scene_eval.rs:532-547 feed the sampler
  an unexpected value; needs temporary tracing on the sample path before any
  fix.

## Impact

Timed parameter animation is the documented escape hatch for animating plots;
as-is it silently renders garbage.

## Recommendation

Root-cause the injected value, fix the param-track sampling, add a pixel
regression at mid- (blend window) and post-transition times, and align the
analyzer's common-property handling for plot params. Tracked in the roadmap
fix pass (2026-09-06).
