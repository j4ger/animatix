# Bloom straight-alpha composition

Follow-up to the [DropShadow and backend-admission work](2026-11-16-filter-backend-admission.md).

Bloom treated both textures as premultiplied even though the effect ABI stores
straight RGBA. It added straight RGB directly, clamped it to output alpha,
then stored that value as straight RGB. Translucent saturated colours were
darkened; mixed chains also added the wrong amount of light.

The pass now converts the pre-chain original and running result to
premultiplied colour, applies `keep` and the gated added intensity, clamps that
sum to output coverage, and unpremultiplies before storage. Zero alpha stores
transparent black. The alpha accumulation and straight-luma threshold formulas
are unchanged. When both input texels are opaque, this reduces to the previous
add-and-clamp result. No property, uniform layout, or effect ABI version changes.

## Pixel evidence

Six new GPU regressions cover saturated partial-alpha content, partial glow,
transparent input, alpha saturation, thresholding, and the distinct original
and running inputs. Four fail with the original shader; all eight Bloom tests,
including the existing opaque controls, pass after correction.
The saturated-source test also checks `keep: 0.5`: straight red becomes about
153 while alpha stays about 160, so kept colour and accumulated coverage are
locked independently.

| Sample | Original shader RGBA | Expected straight RGBA |
|---|---|---|
| Red alpha 128, intensity 0.25 | `[160, 0, 0, 160]` | `[255, 0, 0, 160]` |
| Blue penumbra, transparent original | `[0, 0, 16, 16]` | `[0, 0, 255, 16]` |
| White alpha 64, threshold open | `[128, 128, 128, 128]` | `[255, 255, 255, 128]` |
| Red 64 → ColorGrade red 128 → Bloom | `[192, 0, 0, 255]` | approximately `[96, 0, 0, 255]` |

The corrected column gives the analytic expectations checked with rounding
tolerances; the green test log records assertions, rather than printing the
sample bytes. The penumbra uses the corrected DropShadow as its running input,
so it isolates Bloom from Blur's separate neighbourhood-filtering semantics.
Blur is not changed here.

The CLI comparison renders a plain colour at the expected post-Bloom alpha
beside a filtered colour over the same opaque theme background. Its centre
pixels are:

- Plain reference: `(4, 59, 169, 255)`.
- Original Bloom: `(4, 73, 109, 255)`.
- Corrected Bloom: `(4, 60, 169, 255)` — one green code value from the reference.

The fixture and before/after PNGs are preserved under
`target/perf/gpu-round3-2026-11-16/`. The original shader red run is
`bloom-old-red.log`; the corrected run is `bloom-green.log`.

## Cost measurement

The native `gpu_interaction` driver now accepts a `bloom` preset: one and eight
independent 64×64 regions on a 1280×720 target, half-alpha source colours,
intensity 0.75, keep 1, threshold 0. Five alternating A/B and B/A process pairs
compare binaries built before and after only the shader fix. Each run warms
ten frames and measures 120; render encoding/submission and explicit completion
wait are reported separately. Pipeline creation is outside the timed region.

On AMD Radeon 890M, Vulkan/RADV Mesa 26.0.3:

| Scope count | Before completed frame (ms) | After (ms) |
|---|---:|---:|
| 1 | 0.763 | 0.745 |
| 8 | 2.889 | 2.866 |

These are medians across process results, each using its per-frame median.
Encoding and wait medians change by at most 0.5%, while paired completed-time
changes range from −21.6% to +14.3% at one scope and −51.1% to +0.17% at eight.
The latter includes a slow before-run outlier. This is near-neutral evidence
on one GPU, not a general speedup claim or GPU timestamp measurement.
`bloom-summary.json` retains every pair, raw-file path, adapter identity, and
both binary hashes.

Full workspace, browser, and CPU-screen evidence for the combined worktree is
recorded with the [presentation submission change](2026-11-16-presentation-submit.md).
