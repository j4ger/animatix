# Probe 009 — GPU Filter: blur is not visually applied (image export)

**Status: FIXED (2026-08-31); attribution revisited 2026-11-15.** The original
fix separated the compute-pass submissions and restored blur. However, the
old `dispatch_blur` also wrote horizontal/vertical direction parameters into
the same uniform buffer before submitting both passes. Splitting submissions
changed parameter snapshots as well as command boundaries, so the experiment
did not isolate storage-write visibility from parameter overwriting.

The current backend gives every instance/pass independent uniform slots.
Separate-versus-combined full-image tests now match on hardware AMD RADV and
software llvmpipe, including multiple passes, changing parameters, repeated
effects, ROI changes, and pending-composite harvests. Normal execution uses one
encoder/submission for the seed, effect passes and harvest. Stage dumps retain
separate submissions so intermediate readbacks see the intended image. See
[the review and measurements](../../../docs/history/2026-11-15-gpu-interactions.md).

The removed diagnostic's original source is unavailable, so the direction
overwrite is a confirmed historical confounding factor, not proof of the
precise cause of every observation made in August. The remaining sections
preserve that original investigation; their storage-visibility attribution
does not define the current synchronization contract.

## Repro

`repro.amx` wraps a checkerboard `Image` in a `Filter` with `blur: 10`.

```bash
nix develop
cargo run --bin animatix -- image dogfood/probes/009-filter-gpu-deferred/repro.amx \
    --time 1.0 -o /tmp/filter_blur.png
```

**Expected**: the checkerboard squares are smoothly blurred (soft edges).
**Original actual output**: the checkerboard was fully sharp — the blur was not applied.

## Why this was NOT the earlier "environment vs bug" ambiguity (2026-08-31)

Two backend tests isolated the visible fault. They now live in
`crates/animatix-render/src/filter_backend/tests.rs` and remain regression guards:

- `color_matrix_actually_desaturates`: a red rectangle through `saturate: 0`
  desaturates to gray (Rec.709 luma ≈ 54) — the scene render, compute dispatch,
  and readback machinery works on the same device.
- `gpu_filter_blur_softens_a_hard_boundary`: a sharp black/white boundary
  through `blur: 8` must come back with an intermediate-alpha gradient. **Before
  the fix this failed (perfectly hard 255|0 edge); after the fix it passes.**

The investigation recorded a removed diagnostic: a single horizontal blur pass
(tex_a→tex_b) produced a Gaussian gradient, while two-pass ping-pong in one
command buffer returned an apparently untouched copy; separate submits
produced the gradient. This was originally attributed to storage visibility.
Because submission boundaries also changed the shared direction-uniform
snapshots, those observations did not distinguish synchronization from
parameter overwriting.

## Original workaround (2026-08-31)

The original `render_and_filter_scene_to_view` gave the horizontal blur,
vertical blur, and color-matrix passes separate encoders and submissions.
That restored the expected pixels and ensured each direction-uniform upload
was submitted before the next overwrite. It did not independently prove
that each texture dependency required a submission boundary. The historical
final texture was tex_b after a color matrix, otherwise tex_a.

The old smoke tests only asserted `is_ok()` + size, which is why this never
surfaced; the two content-level tests above are the regression guard.

## Verification

```bash
nix develop
cargo run --bin animatix -- image dogfood/probes/009-filter-gpu-deferred/repro.amx \
    --time 1.0 -o /tmp/filter_blur_fixed.png   # checkerboard edges are now soft
```

Pixel scan across the scene's middle row after the fix: 88 soft-gradient
transitions and 0 hard steps (previously the checkerboard was fully sharp).

## Resolution notes

The color-matrix control established that the compute path ran on the test
device; it did not establish which of the two changing variables caused the
blur failure. The later full-image policy comparison on hardware and software
Vulkan provides the current regression evidence. The synchronization contract
is documented in `docs/effects.md`; the original attribution above is retained
as investigation history, not as a required per-pass submit rule.
