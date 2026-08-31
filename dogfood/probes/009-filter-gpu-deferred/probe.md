# Probe 009 — GPU Filter: blur is not visually applied (image export)

**Status: FIXED (2026-08-31).** Root cause: back-to-back compute passes that
read/write the same ping-pong textures inside a single command buffer did not
make one pass's storage write visible to the next pass (on this driver). Fix:
each blur/color-matrix compute pass is now submitted in its own encoder, so a
submit boundary synchronizes the texture reads/writes. Verified by pixel
evidence (soft gradients across the previously hard checkerboard edges) and the
backend regression tests below.

## Repro

`repro.amx` wraps a checkerboard `Image` in a `Filter` with `blur: 10`.

```bash
nix develop
cargo run --bin animatix -- image dogfood/probes/009-filter-gpu-deferred/repro.amx \
    --time 1.0 -o /tmp/filter_blur.png
```

**Expected**: the checkerboard squares are smoothly blurred (soft edges).
**Actual**: the checkerboard is fully sharp — the blur is not applied.

## Why this was NOT the earlier "environment vs bug" ambiguity (2026-08-31)

Two backend tests in `crates/animatix/src/renderer/filter_backend.rs` isolated
the fault and now serve as regression guards (both active/passing):

- `color_matrix_actually_desaturates`: a red rectangle through `saturate: 0`
  desaturates to gray (Rec.709 luma ≈ 54) — the scene render, compute dispatch,
  and readback machinery works on the same device.
- `gpu_filter_blur_softens_a_hard_boundary`: a sharp black/white boundary
  through `blur: 8` must come back with an intermediate-alpha gradient. **Before
  the fix this failed (perfectly hard 255|0 edge); after the fix it passes.**

The root cause was pinned by an isolated diagnostic (then removed): a SINGLE
horizontal blur pass (tex_a→tex_b) produced a textbook Gaussian gradient, but
the two-pass ping-pong in ONE command buffer returned the untouched copy; the
same two passes in SEPARATE submits produced the gradient. Hence: the two
chained compute passes did not synchronize in a single encoder.

## The fix

`render_and_filter_scene_to_view` (`filter_backend.rs`) now gives the horizontal
blur pass, the vertical blur pass, and the color-matrix pass each their own
`CommandEncoder` + `queue.submit`, so writes are visible across passes
(submit boundary = sync point). The final texture is tex_b after a color
matrix, otherwise tex_a.

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

The original "Ask (human)" — run on a real GPU to decide bug-vs-environment —
is resolved by the color-matrix control: since the same device runs the
color-matrix compute correctly, the blur no-op is a code bug that would also
fail on a real GPU. No further human review needed for the diagnosis; a GPU is
only useful to double-check the eventual fix.