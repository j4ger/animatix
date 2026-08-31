# Probe 009 — GPU Filter: blur is not visually applied (image export)

**Status: CONFIRMED REAL BUG (2026-08-31) — root-caused to the blur compute
passes writing nothing; the surrounding machinery works.** No longer awaiting
human review: a backend-level test with a color-matrix control proves this is a
code bug, not a software-Vulkan limitation.

## Repro

`repro.amx` wraps a checkerboard `Image` in a `Filter` with `blur: 10`.

```bash
nix develop
cargo run --bin animatix -- image dogfood/probes/009-filter-gpu-deferred/repro.amx \
    --time 1.0 -o /tmp/filter_blur.png
```

**Expected**: the checkerboard squares are smoothly blurred (soft edges).
**Actual**: the checkerboard is fully sharp — the blur is not applied.

## Why this is NOT the earlier "environment vs bug" ambiguity (2026-08-31)

Two new backend tests in `crates/animatix/src/renderer/filter_backend.rs` isolate
the fault:

- `color_matrix_actually_desaturates` (active, passes): a red rectangle through
  `saturate: 0` with `blur: 0` **desaturates to gray** — so in the SAME
  `GpuFilterBackend`, on the SAME lavapipe device, the scene renders, the
  compute pipeline dispatches, and the readback works. The machinery is fine.
- `gpu_filter_blur_softens_a_hard_boundary` (**`#[ignore]`, known bug**): a
  sharp black/white boundary through `blur: 8` stays **perfectly hard**
  (255|0, no intermediate pixels). The blur passes effectively write nothing —
  the result is the unblurred copy.

Conclusion: the blur WGSL shader / horizontal+vertical ping-pong chain
(specifically the two chained `dispatch_blur` passes — single-pass color
matrix works) does not write its result. Suspects, in order:

1. Uniform / `BlurParams` layout read as `radius < 0.5` → the shader takes its
   copy-branch (produces an unblurred copy). The Rust `BlurParams` is
   `repr(C)` and matches the WGSL offsets, so this is listed for completeness.
2. Chained compute passes writing/reading the same ping-pong textures in one
   encoder (barrier / bind-group reuse issue) — most plausible since a
   single-pass control works.
3. A bug in the Gaussian loop (clamp/textureLoad bounds).

The smoke tests only asserted `is_ok()` + size, which is why this never
surfaced. Fix target: `dispatch_blur` / the ping-pong in
`render_and_filter_scene_to_view` (`filter_backend.rs` 430-700); after fixing,
enable the `#[ignore]` blur test as the regression guard.

## Resolution notes

The original "Ask (human)" — run on a real GPU to decide bug-vs-environment —
is resolved by the color-matrix control: since the same device runs the
color-matrix compute correctly, the blur no-op is a code bug that would also
fail on a real GPU. No further human review needed for the diagnosis; a GPU is
only useful to double-check the eventual fix.