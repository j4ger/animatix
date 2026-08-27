# Probe 009 — GPU Filter: blur is not visually applied (image export)

**Status: OPEN — needs human review on real GPU / GUI.** Could not be
definitively root-caused in the `nix develop` software-Vulkan (lavapipe)
environment; the code path looks correct but the blur is not visibly applied.

## Repro

`repro.amx` wraps a checkerboard `Image` in a `Filter` with `blur: 10`.

```bash
nix develop
cargo run --bin animatix -- image dogfood/probes/009-filter-gpu-deferred/repro.amx \
    --time 1.0 -o /tmp/filter_blur.png
```

**Expected**: the checkerboard squares are smoothly blurred (soft edges).
**Actual**: the checkerboard is fully sharp — the blur is not applied.

## Why this is in code

`GpuFilterBackend` (crates/animatix/src/renderer/filter_backend.rs) has a WGSL
compute blur shader and a ping-pong horizontal/vertical pass:

- `render_and_filter_scene_to_view` renders the sub-scene to a texture, then
  runs two `dispatch_blur` passes (horizontal, then vertical) with `radius =
  blur` (e.g. 10) — `filter_backend.rs:616-700`.
- `dispatch_blur` writes `BlurParams { radius, direction, tex_size }` to a
  uniform buffer and dispatches the compute pass — `filter_backend.rs:430-478`.
- The blur shader samples a Gaussian window of `ceil(radius)` px on each side
  (sigma = radius/3) — `filter_backend.rs:34-77`.
- The final (blurred) texture is either returned to the caller
  (`render_scene_to_image_gpu_filtered`) or kept as a **pending composite** and
  blitted onto the rendered scene afterward (`offscreen.rs:147-164`).

All of this looks correct by inspection, and `GpuFilterBackend::new` succeeds
here (compute is supported by lavapipe). Yet the output is unblurred.

## Answers needed / hypotheses

1. **Is this a real GPU bug or a software-Vulkan limitation?** The smoke tests
   (`filter_backend.rs:911-978`) only assert the backend returns a `SceneImage`
   of the right size (`result.is_ok()`); **they never assert the content is
   actually blurred**. So the blur pipeline has never been verified to produce a
   blurred image on any backend. Test on a real GPU adapter (or via the GUI
   preview) to see whether `blur` works there.
2. If blur **does not** work on real GPUs either, suspect the WGSL shader or the
   texture bindings (e.g., the `texture_storage_2d<rgba8unorm, write>` view
   vs. the sampled `texture_2d<f32>` view), or the blur radius being effectively
   tiny.
3. **Silent fallback**: in the Filter scene-eval branch there is a fallback that
   renders children unfiltered without surfacing a warning when the GPU filter
   backend is unavailable. Even if a real GPU works, the silent fallback on a
   machine without a usable filter backend is worth making louder (diagnostic).

## Ask (human)

- Run `repro.amx` on a real GPU adapter and/or in the GUI preview: does `blur`
  (and `brightness`/`contrast`/`sepia`) visibly apply?
- If yes (works on GPU, not on lavapipe): close this as environment-specific, or
  add a "filter backend unavailable" diagnostic to avoid silent fallback.
- If no (broken on GPU too): the blur shader/pipeline or its verification needs a
  fix; add a content-level regression test that renders a two-color scene through
  the filter and asserts the boundary is not a hard edge.
