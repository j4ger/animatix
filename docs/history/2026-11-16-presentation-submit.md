# Render and presentation blit submission

This follows the [GPU interaction audit](2026-11-15-gpu-interactions.md) and
[backend-admission change](2026-11-16-filter-backend-admission.md).
Bloom's related correctness fix is recorded [separately](2026-11-16-bloom-alpha.md).

## Submission boundary

The Web player previously submitted Vello's render and then submitted the
canvas-format fullscreen blit. The new path appends that blit to Vello's current
encoder before its existing queue submission. The eligible render/blit pair
therefore uses one explicit `Queue::submit` instead of two. Surface acquisition
and `queue.present(frame)` remain with the Web caller; this does not batch
multiple frames or players.

The vendored Vello hook is synchronous. It encodes the complete recording,
resolves any Vello profiler queries, invokes the suffix, submits, and only then
returns transient resources to the pool. It does not return a deferred encoder.
Recording errors return before the suffix. The ordinary Vello API delegates
with a no-op callback. The atlas-retention repair remains independent; both
removal conditions and boundary tests are documented in
[`crates/vendor/vello/README.animatix.md`](../../crates/vendor/vello/README.animatix.md).

`RendererCore::render_vello_scene_and_present` selects a format-matched blit
pipeline. A private, lazily initialized 48-byte alpha=1/no-mask uniform removes
presentation parameter uploads and staging-buffer creation after warmup.
It has only `UNIFORM` usage and is never changed. It has a distinct binding-cache
key from general blits, within the existing 64-entry bound. General external
encoders retain their per-draw staging copies: a pending externally submitted
blit can change the mutable uniform, but cannot change the presentation uniform.

## Web admission and fallback

The merged path applies to an active single timeline with no Filter/Glass
capabilities, including Mask clipping and a composition outside a transition.
Scenes with future or currently identity Filter/Glass effects still take the
existing path. Filter composites and Glass backdrops drain in their existing
order before presentation; active transitions retain their compositor and FLIP
overlay ordering. Readback continues through the no-surface entry point.

Raster rounding, nominal scale re-encoding, presentation-scene recycling,
surface format selection, error handling, and dedup recording retain their
previous behavior. An acquired surface is presented and dedup is recorded only
after the chosen rendering path succeeds.

## Evidence

Raw artifacts are in `target/perf/gpu-round3-2026-11-16/`.

- `presentation-hardware-final.log` and `presentation-software-final.log` pass
  full-image separate-versus-merged comparisons on Radeon 890M and llvmpipe,
  including RGBA/BGRA formats, resized targets, and a reused image Blob across
  image-free renders. Independent target colours remain distinct.
- `immutable-uniform.log` verifies that a recorded mutable-alpha draw on the
  same source cannot alter the subsequent full-opacity presentation binding.
- `vello-callback-error-native.log` passes the private missing-buffer recording
  regression: the recording returns an error without running its suffix.
- The native `web_present_driver` uses the same pre-evaluated real scenes in
  both modes, fixed raster scale, rotated mode order, untimed warmup, and
  separate CPU encoding/submission and completion-wait measurements. Its
  submission counts describe source call sites, not a production queue counter.
  Native timing is not browser WebGPU timing, swapchain latency, or FPS.
- A first exploratory staging-buffer variant found one byte differing by one
  code value in a complex scene. The driver now records repeated separate and
  merged controls, all pairwise differences, coordinates, and full raw frames
  before reporting parity failure. The initial failure is retained in
  `present-staging-run.log`; it is not silently classified as renderer noise.

The final native run (`present-control.json`) contains three alternating paired
rounds of 90 frames per arm for each of six fixture/size combinations, after
12 untimed warmup frames. The AMD Radeon 890M results are mixed: merged CPU
encoding/submission means are lower in three cases and higher in three;
completed-wall means are lower in two and higher in four. This does not
establish a general latency improvement. The verified reduction is the explicit
render/blit submission count and steady-state presentation parameter traffic.

The real-scene sequence does **not** pass strict byte equality. Four cross-mode
comparisons over five frames contain eight byte differences in total, at four
unique frame/coordinate/channel positions, each differing by one code value.
Every observed coordinate and value pair also occurs between repeated renders
of the same mode. `present-control-audit.json` independently checks all raw
frame ranges, comparison statistics, and exact same-mode coverage against
`present-control.pixels.bin`; it reports no uncovered cross-mode difference.
This bounds the observed difference without identifying its cause or converting
the driver's strict parity failure into a pass. The synthetic hardware and
software regressions above remain exactly equal.

## Browser performance boundary

An adjacent browser A/B is prepared under `browser-ab/`: the final SDK is
`4900538d480625b046b471be337e83525144086c36af2a110217a1e25d4dceac`,
and the control SDK is
`116fbfb0fc1c38e679323b94f32b9ac4939eec029b5104a0d883219a7e32e545`.
The control changes only the surface wrapper's `Some(surface)` argument to
`None`, selecting the existing separate render/blit path. Compiler, bindgen,
optimizer, splitter, compressor, and Node versions match. The final source,
embed bundle, and runtime symlink are restored and verified before launching
the browser; `provenance.json` and `control-source.diff` record the boundary.

Neither browser attempt reaches timing. The first fails adapter discovery with
the default NVIDIA userspace; the second uses the previously successful
595.104.02 process-local ICD/libraries and baseline headless flags, but ANGLE
still reports Vulkan initialization error −3 and no WebGPU adapter. Current
Chromium is 156.0.8078.79; the earlier baseline used 146.0.7680.164. These facts
do not establish the cause of the failure. Both failed attempts and the driver
are retained; no successful process pair, frame-time result, or browser submit
counter result is claimed. Browser performance remains **unverified**.

The prepared driver pins a 960×540 surface and scale 0.85 (1088×612 raster),
checks SDK identity, actual draws, adapter identity, nonempty pixels, and
per-phase queue-submit deltas. Once a working browser environment is available,
run the English text/vector and rich/media fixtures in three process pairs,
rotating A/B, B/A, A/B. Retain every sample and apply the existing provisional
5% / 0.167 ms queue-depth-one p95 screen. This is a remaining measurement task,
not a displayed-playback FPS claim.

## Validation

The final combined worktree passes the named `fmt`, `structure`, `check`,
`lint`, `test`, `doc-links`, `doc`, and `render-smoke` gates in the native dev
shell. The workspace test report records 2,366 active tests; render-smoke
renders all 65 selected examples. The separate private Vello error test and
both hardware/software presentation probes also pass.
After adding the non-default `keep` assertion, all eight Bloom tests pass again
(`bloom-keep-final.log`), followed by `fmt`, `structure`, and `doc-links`
(`final-small-gates.log`).

`wasm-check` passes in the `web-build` shell. `web-build` and `web-smoke` pass
in the `web-ci` shell, including all eight browser smoke pages. The final base
Wasm gzip is 1,743,419 bytes, below the existing 1,750,000-byte budget. Logs are
`native-gates-verified.log`, `wasm-check-verified.log`, and
`web-sdk-smoke-final.log`. After the native driver's module-path correction and
the extra `keep` assertion, the final SDK is rebuilt with current-input
verification (`browser-ab/final-sdk-build.log`); those edits do not change the
Web renderer. No budget or load threshold was relaxed.

`scripts/perf-bench.sh compare quick` passes against the fresh pre-change
`cc1adfcf` baseline: 72/72 rows (100% coverage), zero material regressions,
zero inconclusive rows, and zero policy/baseline failures. The idle preflight
records load 0.27 on 24 CPUs against the unchanged limit of 12.00. The output
is `cpu-compare.log`, with estimates under `target/perf/gpu-round3-after/`
and the saved baseline under `target/perf/gpu-round3-baseline/`. This screen
covers engine/analyzer CPU workloads; the native render/presentation timings
above cover the changed GPU interaction separately.

## Follow-up

The [multi-player hardware measurements](2026-11-16-multi-player-submission.md)
record successful Radeon WebGPU runs, exact per-player submission counts,
page-depth-one completion probes, and the remaining browser timing limits.
The failed attempts in this record remain unverified measurements.
