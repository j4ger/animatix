# Straight-alpha neighbourhood filtering

This follows the [Bloom composition repair](2026-11-16-bloom-alpha.md).
`Blur` and `MotionBlur` still interpolated and averaged straight RGB directly.
Transparent neighbouring texels could darken saturated colours or leak hidden
RGB into visible edges. Premultiplying after a bilinear sample does not repair
that sample: its transparent texels have already contaminated the colour.

Both operators now premultiply individual texels before interpolation and
accumulate premultiplied colour and coverage with the original weights. They
recover straight RGB from the filtered coverage before storing. Fully opaque
neighbourhoods retain the same colour average. Zero-alpha neighbourhoods store
transparent black.

## Sampling and compatibility

Blur keeps two separable Gaussian passes, 17 taps per pass, the existing
`max(1, radius / 8)` stride, sigma `radius / 3`, and clamp-to-edge boundaries.
The inactive axis is integral, so a tap needs one texel load at an exact integer
position, or two loads for fractional interpolation. This avoids four loads
for an inherently one-dimensional pass. Each pass stores straight RGBA for
the following operator.

MotionBlur keeps its angle convention, tap positions, 64-tap cap, and
`max(center.a, sampled_mean.a)` output coverage. Its colour divides by the
sampled mean alpha before applying that coverage rule. A diagonal fractional
sample needs four clamped texel loads; exact integer and axis-aligned positions
use one or two. There is no epsilon snapping or change to the sample positions.

The shader layouts, authored properties, pass counts, effect ABI, support
padding, and raster scaling are unchanged. Existing radius-below-0.5 Blur and
one-tap MotionBlur passthrough branches remain bit-exact copies. No GPU
submission or host upload is added.

## Pixel evidence

Raw RGBA seeds include saturated half-alpha red beside transparent texels with
hidden green/blue. They bypass Vello's normalization of invisible RGB and run
the actual built-in compute chain. Independent CPU references use clamped
bilinear footprints and normalized kernel weights, including intermediate
8-bit storage. Full-image comparisons permit at most two code values per
channel and reject every byte beyond that tolerance.

Eight separate tests cover dense and fractional Blur, axial and diagonal
MotionBlur, zero-alpha hidden colour for each operator, fully opaque colour
averages, and Blur followed by Bloom. The original shaders fail seven tests;
the opaque control passes. The corrected shaders pass all eight on both the
fallback software device and the explicit non-CPU hardware mode.

For MotionBlur length 8 at the boundary, half-texel sampling covers 3.5 red
texels out of eight: input alpha 128 yields outside alpha about 56, with straight
red 255. One texel inside, sampled mean alpha is about 72 but output alpha
remains the authored 128. Blur radius 10 has a 1.25-texel stride; the x=34 probe
includes a sample at x=31.5 across the colour/transparent boundary. This catches
the ordering error that integer-only taps would miss. The Blur→Bloom outside
probe has straight red 255 and coverage about 27.

Artifacts are under `target/perf/gpu-round4-2026-11-16/`: the definitive old
run is `spatial-red-split.log`; final runs are `spatial-green-software.log`
and `spatial-green-hardware.log`. The CLI before/after fixture and images use
unique actor labels and an explicit visible half-alpha source.
Visual review confirms brighter, undarkened blur edges. MotionBlur's retained
coverage floor keeps the original authored coverage at source pixels; paired
with corrected straight RGB, its source edge looks firmer than the darkened
old result. The x=31 regression locks that retained coverage rule.

## Cost and validation

The native `gpu_interaction` driver adds half-alpha dense Blur (radius 8),
wide Blur (44), and diagonal MotionBlur (length 24, angle 27). Default 64×64
regions retain one/eight-scope runs; optional 512×512 uses one scope to expose
kernel cost separately from small-region fixed overhead. Old/new binaries
share the driver and differ only in the two shaders. Each process warms ten
frames, measures 120, and separates encoding/submission, completion wait, and
their sum. Five alternating A/B and B/A process pairs retain every result.

The run uses AMD Radeon 890M, Vulkan/RADV Mesa 26.0.3. All 60 process records
report the same adapter and expected ROI. Completed-frame values below are
the medians of five process-level frame medians, in milliseconds:

| Preset | Region | Scopes | Before | After |
|---|---:|---:|---:|---:|
| Blur radius 8 | 64 | 1 | 0.830 | 0.807 |
| Blur radius 8 | 64 | 8 | 2.934 | 2.918 |
| Blur radius 8 | 512 | 1 | 0.822 | 0.808 |
| Blur radius 44 | 64 | 1 | 0.942 | 0.849 |
| Blur radius 44 | 64 | 8 | 2.916 | 2.944 |
| Blur radius 44 | 512 | 1 | 0.929 | 0.863 |
| MotionBlur 24 / 27° | 64 | 1 | 0.907 | 0.932 |
| MotionBlur 24 / 27° | 64 | 8 | 2.846 | 2.877 |
| MotionBlur 24 / 27° | 512 | 1 | 0.946 | 0.805 |

The nine median changes range from −14.9% to +2.8%, but matched process-pair
changes span −58.5% to +51.7%. Encoding/submission varies substantially, so
lower totals do not establish a speed improvement. At 512 pixels, completion
wait medians rise 2.3%, 6.3%, and 9.9% respectively; that CPU wait also includes
queue notification and is not a GPU timestamp. The run does not show a clear
material completed-frame regression, but cannot isolate the added sampling
cost or establish general hardware neutrality. The exact axis fast paths are
not separately timed. `spatial-summary.json` retains all phases, pairs, raw
paths, and binary hashes; `run-spatial-ab.py` retains the protocol. Idle load
is 5.62/24 before and 4.11/24 after, below the unchanged limit of 12.

## Validation

The named `fmt`, `structure`, `check`, and `lint` gates pass in
`native-precheck-final.log`. `test`, `doc-links`, `doc`, and `render-smoke` pass
in `native-validation-final.log`: 2,374 active tests and all 65 selected
examples. The separate eight-case hardware and software runs also pass.
`wasm-check.log` passes in the `web-build` shell; `web-validation-final.log`
passes `web-build` and all eight `web-smoke` pages in the `web-ci` shell.
The final base Wasm gzip is 1,744,965 bytes within the unchanged 1,750,000-byte
budget. `spatial-before.png` and `spatial-after.png` were visually inspected.

`scripts/perf-bench.sh compare quick` covers 72/72 saved rows with zero material
regressions and zero policy failures, but exits 2 for one inconclusive row:
`code_compile/highlighted`. Its delta interval is 55.8–108.4 µs, straddling the
unchanged 100 µs action budget; its relative interval also straddles 5%.
The full output and estimates remain in `cpu-compare.log` and
`target/perf/gpu-round4-after/`.

Three fresh-process repeats of that exact benchmark use two seconds of warmup,
eight seconds of measurement, and 100 samples each. Against the same saved
2.065 ms baseline, their means are 2.091, 2.110, and 2.104 ms. All three classify
`OK` under the unchanged policy; the largest upper delta bound is 56.4 µs,
below the 100 µs budget. Each repeat's raw samples, estimates, and classification
are retained in `highlighted-repeat-{1,2,3}/` and `highlighted-repeats.json`.
The original full comparison's inconclusive result is not replaced with a synthetic
passing row or a refreshed baseline.

This CPU screen reuses the recorded `cc1adfcf` baseline from the previous round;
it includes the intervening Bloom/presentation changes, rather than isolating
this shader patch. The GPU binary pairs above isolate the two spatial shaders.
The CPU preflight is 0.99/24 against the unchanged limit of 12, with no load
override. The focused repeats resolve the one borderline CPU workload; they
do not establish a causal CPU speed benefit from the shader change.
