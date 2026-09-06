# Probe: timed plot-param assignment renders a wrong curve after completion

Status: **resolved 2026-09-06** — not a param-track bug at all. The adaptive
samplers' subdivision floor was 3 levels (8 samples); high-frequency curves
(e.g. `sin(5x)` over `(-10, 10)` ≈ 16 periods) fit a single chord within
tolerance and rendered as a straight line. `sample_recursive_cartesian`,
`_polar`, and `_parametric` now take a `min_depth` derived from the plot's
`resolution` (build-time previews use full depth). Pixel-verified: the repro
and a plain `sin(5x)` both render the full wave. Original writeup kept below
with the corrected diagnosis inline.

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
- **Corrected diagnosis (2026-09-06):** rendering a plain
  `func: (x) => sin(5 * x)` with no param machinery reproduces the sloped
  line — the param-track path was innocent. The samplers only guaranteed 8
  samples (`depth < 3` floor); for sin(5x) the 9 sample points are
  monotonically decreasing and nearly collinear, so the deviation test passes
  immediately. The param-track keyframes were working; the resampled wave was
  then under-sampled into a line.

## Impact

Any curve whose frequency is high relative to the domain (waves, aliasing
demos, parametric Lissajous figures) silently under-samples.

## Recommendation (applied)

Honor `resolution` as a minimum sample count: `min_depth =
ceil(log2(resolution))` (96 → 128 samples) in all three recursive samplers;
adaptive subdivision continues beyond the floor while the chord deviation
exceeds `tolerance`. Analyzer alignment landed 2026-09-07: declared plot
params no longer emit `unknown-property` infos (declaration and assignment
sites resolve against the func closure's referenced names).
