# GPU interaction review — 2026-11-15

The first implementation pass removes the GUI preview's separate sampling
texture and fixes parameter aliasing that made deferred blit encoding unsafe.
It also isolates repeated effect instances and reuses effect bind groups. A
second pass tests the historical submission constraint independently and
combines each effect chain into one encoder/submission. Stage dumps retain the
separate reference path.

## Changes

- GUI preview rendering exposes an `Rgba8UnormSrgb` sampling view of the current
  `Rgba8Unorm` render/composite target. The old path copied the complete frame
  into a separate sRGB texture and submitted that copy every time the preview
  updated. The new path saves one full-frame copy, one submission, and one
  frame-sized texture. At 1280×720 the removed texture holds 3,686,400 bytes;
  at 3840×2160 it holds 33,177,600 bytes, before allocation overhead.
- Effect author uniforms use independent aligned slots for repeated instances
  of the same effect type. Previously `Blur(2) → Blur(8)` and
  `Blur(8) → Blur(8)` produced identical pixels because the second parameter
  upload overwrote the first before submission.
- Effect bind groups are cached by their wgpu resource handles, bounded to
  256 entries. Parameter changes still upload fresh values and select the
  appropriate dynamic offsets. Buffer growth and scratch eviction clear the
  cache. A stable two-pass Blur reuses its two bindings after warmup.
- Context uploads cover active passes instead of all 64 available slots:
  a two-pass Blur uploads 512 bytes instead of 16,384 bytes per scope.
- External-encoder blit APIs record parameter copies immediately before the
  corresponding draw passes. Several calls can therefore share one encoder
  without all draws consuming the final call's alpha/mask. More than 64 layers
  are processed in successive chunks, with their uploads in command order.
  These APIs allocate temporary staging buffers; the immediate-submit APIs
  retain their existing queue-write path. This fixes batching correctness;
  it is not a claim that staging allocation is free.
- GUI transitions now drain pending Filter/Glass layers into both source
  targets before blending them. Resizing drops all three filter backends so
  the incoming side cannot reuse old-size targets after the outgoing side
  updates the shared dimensions marker, and clears the compositor's cached
  input views without rebuilding its pipeline.

## Preview presentation experiment

The ignored GUI test `benchmark_legacy_copy_versus_direct_srgb_view` compares
the old copy-plus-sampling path with direct sRGB-view sampling in one process,
alternating AB/BA order for 120 pairs after warmup. Both paths draw the same
1280×720 pixel stream, with a queue wait after each sample. Three separate
invocations on the AMD Radeon 890M/RADV device produced:

| Run | Legacy copy + sample p50 | Direct view + sample p50 |
| --- | ---: | ---: |
| 1 | 0.972 ms | 0.681 ms |
| 2 | 0.972 ms | 0.679 ms |
| 3 | 0.979 ms | 0.683 ms |

The median of those medians falls from 0.972 to 0.681 ms (29.9%). This measures
the isolated preview presentation path in the native test profile, including
CPU submission and completion waiting. It does not measure complete GUI frame
time or establish a 29.9% playback FPS improvement. Raw logs include the
adapter identity and live beside the filter experiment's local evidence.

Reproduce with:

```sh
nix develop -c cargo test -p animatix-gui --lib \
  benchmark_legacy_copy_versus_direct_srgb_view -- \
  --ignored --nocapture --test-threads=1
```

## First filter experiment: parameter isolation and resource reuse

The reusable native driver is
`crates/animatix-render/examples/gpu_interaction.rs`. It constructs Vello scenes
directly and measures one/eight independent 64×64 Blur scopes composited onto
a 1280×720 target. Each case has ten warmup frames and 120 measured frames.
CPU encoding includes queue submissions; completion includes the explicit
queue wait. Neither is a GPU timestamp or displayed playback FPS.

Original and modified source were compiled into separate release executables
before measurement. Three adjacent pairs ran in AB, BA, AB order on the same
device: AMD Radeon 890M Graphics, Vulkan/RADV, Mesa 26.0.3 from the pinned Nix
shell. Values below are medians of the three per-run medians.

| Blur scopes | Metric | Before | After | Change |
| --- | --- | ---: | ---: | ---: |
| 1 | Encode + submit CPU | 0.636 ms | 0.659 ms | +3.66% |
| 1 | Frame completion | 0.845 ms | 0.856 ms | +1.33% |
| 8 | Encode + submit CPU | 3.266 ms | 3.405 ms | +4.27% |
| 8 | Frame completion | 3.486 ms | 3.605 ms | +3.43% |

This experiment does **not** establish a filter-throughput improvement. Pair
direction changes and the first modified one-scope run was substantially
slower than the other two. Keep the parameter correctness fix and describe
binding/upload reductions as resource-count changes; do not turn those counts
into a claimed FPS gain. The driver intentionally leaves effect pass submits
unchanged, so it does not test submission batching.

Local raw summaries, adapter identities, build logs, source hashes and order
metadata live under `target/perf/gpu-interaction-2026-11-15/` (gitignored).
The original source is commit `ed3764e4b6add34d55938007aaa091fabddbbb3e`;
the working tree already had documentation changes before this pass.

## Second filter experiment: combine the chain submission

The original pre-fix code at `24e4778f^` wrote `BlurParams.direction` to the
same buffer at offset zero for horizontal and vertical dispatches, submitting
only after both writes. Commit `24e4778f` split the submissions, changing both
uniform snapshots and submission boundaries. The removed diagnostic's source
is unavailable, so this establishes a confounded experiment, not the precise
root cause of every historical observation.

With independent uniform slots, a new pixel matrix compares separate and
combined execution on the same device. It covers a two-dimensional checker/cross
pattern, Blur, Blur+ColorGrade+Bloom, repeated effects, changing parameters,
different ROI sizes, and a harvested pending composite sampled into a readable
target. All RGBA bytes match on both:

- AMD Radeon 890M, Vulkan/RADV, Mesa 26.0.3;
- llvmpipe, LLVM 21.1.8, Mesa 26.0.3, Vulkan software device.

Normal execution now combines seed copies, independent compute passes and
harvest into one encoder/submission. A pending two-pass Blur chain needs one
chain submission instead of four; its Vello sub-scene render still submits
separately. The eight-scope driver therefore drops from 42 to 18 submissions
per frame, including the main render and final composite batch. These counts
follow the call paths; the timing experiment below measures their effect.

Both release executables use the new uniform slots and binding caches; the
comparison isolates the submission policy. Five adjacent pairs alternate
AB/BA order, with ten warmup and 120 measured frames per case. Host load at
measurement start was 0.89 on 24 available CPUs. Values are medians of five
per-run medians on the same AMD device:

| Blur scopes | Metric | Separate submits | Combined submit | Change |
| --- | --- | ---: | ---: | ---: |
| 1 | Encode + submit CPU | 0.668 ms | 0.734 ms | +9.84% |
| 1 | Frame completion | 0.884 ms | 0.922 ms | +4.38% |
| 8 | Encode + submit CPU | 3.222 ms | 2.653 ms | −17.65% |
| 8 | Frame completion | 3.438 ms | 2.864 ms | −16.70% |

All five eight-scope pairs improve: separate completion medians range from
3.353–3.533 ms, combined from 2.846–2.924 ms. The one-scope samples overlap
substantially and change direction across pairs (separate 0.789–1.132 ms,
combined 0.728–1.092 ms); they do not establish a one-scope benefit. Retain that
limitation when reporting the many-scope gain. This is a synthetic scene
measurement, not whole-page FPS or a cross-device performance guarantee.

Raw summaries, pair order, adapter identities, build logs and pixel-test logs
live under `target/perf/gpu-submit-2026-11-15/`. Executable SHA-256 identities:

```text
separate 4cfa90cb5d41430dac673d68be61c6c612d8958adaae41106547baeb8eeb1393
combined 4557994b170eb25612604830041435427929c10920d90944fad84adb359c1a1a
```

`ANIMATIX_DUMP_STAGES` selects the separate path for intermediate synchronous
snapshots. The test override retains both policies for future driver checks.
The hardware test is explicitly ignored in the automatic suite so software-only
CI remains supported; it was run manually with `ANIMATIX_REQUIRE_GPU=1` here.

## Validation

- Native gates `fmt`, `structure`, `check`, `lint`, `test`, and `doc-links`
  pass. The final workspace run executes all 2,349 active cases with exact
  inventory coverage, followed by successful doctests. Its raw summary is
  under `target/ci/workspace-tests/20261115T145731Z-17895-0e369ce49f/`.
- `wasm-check` passes in the project's `nix develop .#web-build` shell,
  checking default/minimal web features and the minimal engine's Clippy run.
- The separate/combined policy matrix passes on hardware and software Vulkan.
  Pixel regressions for repeated uniforms, encoded blit parameters and Glass
  transitions were also run against restored original implementations: they
  fail with the expected wrong pixels and pass after the fixes.
- The quick CPU guard compares 72/72 baseline cases with no material
  regressions, inconclusive rows or policy failures. This covers the shared
  evaluation/analyzer lens; GPU submission throughput is measured separately
  by the experiments above.
- Three-frame storyboards at 1/3/6 seconds were rendered and visually reviewed
  for `examples/animation/30_effects_catalog.amx` and
  `examples/animation/38_glass_panel.amx`. Images and logs live beside the
  submission experiment's local evidence. Both preserve the rendered cast;
  the Glass reference shows the expected changing backdrop blur.

Final gate logs are in `target/perf/gpu-submit-2026-11-15/`. Browser presentation
cadence, other hardware backends and complete GUI FPS are not measured by this
native validation pass.

## Remaining architectural work

- Vello's public `render_to_texture` submits internally. A narrow encoder-tail
  callback could combine rasterization with a final blit/readback copy, but
  filter composites and Glass backdrop reads must be scheduled before that
  tail. Preserve Vello's temporary-resource lifetime through submission.
- Batch independent players within one page tick rather than accumulating
  future display frames. Distinct targets and parameter storage are required;
  deferred encoding alone does not provide either.
- Replace inline filtered-image CPU readback with a GPU image bridge where
  practical. Vello texture registrations belong to the consuming renderer;
  nesting, texture reuse, alpha interpretation and dirty marking need explicit
  coverage before replacing the current path.
- Export already submits frame N+1 before waiting for frame N, using two
  readback buffers and submission-index waits. Deeper in-flight queues and
  replacing static per-worker chunks need measured workload evidence; neither
  was changed by this pass.

Further batching experiments need independent shader-parameter storage and
pixel comparisons, including the consuming renderer's texture lifetimes.
