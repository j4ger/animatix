# Filter backend admission and translucent DropShadow

Follow-up to [the GPU interaction audit](2026-11-15-gpu-interactions.md).
The measurements started on 2026-11-15 and validation continued on 2026-11-16.

## Avoiding unused backend initialization

Native offscreen rendering, GUI preview, and Web playback previously created a
`GpuFilterBackend` even for scenes that only used ordinary Vello drawing or
Mask clipping. Its constructor creates another Vello renderer, fullscreen and
line pipelines, four canvas-sized RGBA8 textures, and a readback buffer. At
1280×720, those five raw pixel allocations describe about 17.58 MiB per backend,
excluding renderer resources and later scratch allocations. This is an
allocation-size calculation, not a measurement of resident VRAM.

`Timeline::needs_filter_backend()` now queries live actor capabilities and
memoizes the result in `EvalCaches`. A writable timeline access invalidates the
memo through the existing mutation funnel; cloning resets disposable caches.
The same query gates frame-cache replay and presentation backend admission, so
a post-build capability change cannot leave those decisions inconsistent.

Plain and Mask-only scenes pass no backend to evaluation or rasterization.
Filter and Glass scopes retain backend initialization even when the sampled
chain is currently disabled or at identity: the scope can activate later.
Transition inputs decide independently. Unused backends are released, while
compatible backend slots are moved between direct and transition paths to
avoid needless reconstruction when the active side changes. Resize still
invalidates backend targets.

### Measurement contract

`crates/animatix-render/examples/backend_init.rs` runs five 720p scenarios:
plain, Mask-only, a Blur that is identity at frame zero and active later,
active Blur, and active Glass. Each process runs three fresh-renderer trials
per scenario in rotated order, eight warmup frames, and thirty measured frames.
The actual renderer device supplies the recorded adapter identity.

The benchmark reports renderer construction separately from the first
completed frame. Frame timings use the blocking offscreen API and include
CPU work, GPU execution, and readback. They are native host wall times, not GPU
timestamps, browser frame rates, or browser startup measurements.

Raw A/B artifacts are under `target/perf/gpu-round2-2026-11-15/`.
The baseline admission implementation is from `246fc540`; both binaries use
the same fixtures and timing algorithm. The baseline driver's subsequently
removed unused result field and the final lint-only median parity expression
do not change those timings.

### Final native first-frame measurements

Five alternating A/B and B/A process pairs ran on AMD Radeon 890M, Vulkan,
RADV/Mesa 26.0.3. Each entry below is the median across five process results;
each process result is itself the median of three fresh-renderer trials.
`init-summary.json` retains each paired change, raw-file path, actual adapter
identity, and both binary hashes. `init-build-final.log` records the final
native build; `final-source.patch` and `backend_init.rs` retain its source.

| Scene | Before frame 0 (ms) | After frame 0 (ms) | Change |
|---|---:|---:|---:|
| Plain | 32.69 | 16.22 | −50.38% |
| Mask only | 35.18 | 16.38 | −53.44% |
| Future Blur, identity at frame 0 | 33.44 | 33.46 | +0.05% |
| Active Filter / Blur | 39.70 | 37.90 | −4.54% |
| Active Glass / Blur | 39.23 | 38.65 | −1.47% |

Plain and Mask first-frame times improve in all five pairs: respectively
44–54% and 50–61% lower. The future-Blur control retains backend initialization
at frame zero. Active-effect controls vary in both directions; this experiment
does not establish a benefit for them. These timings exclude the separately
reported `OffscreenRenderer::new()` interval: the old path's **filter backend**
constructor is inside frame zero, while the main renderer constructor is not.
They therefore do not describe whole-application startup time.

Steady frame medians and renderer-construction controls vary between pairs and
between preliminary and final batches. No steady-state or renderer-construction
speedup is claimed. The preliminary batch is retained in `init-preliminary/`.

## Correct straight-alpha DropShadow composition

Vello's fine shader stores separated (straight) alpha. Effect textures and the
readback image/composite paths use the same representation. DropShadow had
instead treated the source as premultiplied and written premultiplied shadow
colour back into a straight-alpha texture. A half-covered blue shadow therefore
stored `[0, 0, 128, 128]`, causing its blue contribution to be multiplied by
alpha again at presentation.

The shader now multiplies both layers by their alpha, applies source-over in
premultiplied space, and divides by the resulting alpha before storage. Zero
alpha stores transparent black. The existing single-sample path for zero
softness remains intact.

The new GPU tests fail against the original shader at concrete pixels:

| Case | Original output | Expected straight RGBA |
|---|---|---|
| Half-alpha blue shadow only | `[0, 0, 128, 128]` | `[0, 0, 255, 128]` |
| Half-alpha red source over blue shadow | `[255, 0, 64, 192]` | approximately `[170, 0, 85, 192]` |
| Soft blue penumbra | `[0, 0, 16, 16]` | `[0, 0, 255, 16]` |

Coverage also includes a translucent shadow colour over an opaque silhouette,
opaque output compatibility, and transparent input. The old-implementation red
run is `shadow-old-red.log`; the corrected run is `shadow-green.log` in the
artifact directory.

An end-to-end CLI image probe also compares the shadow-only pixel with a
plain blue shape at the same alpha over the same background. At source alpha
0.5 the original shadow was `(5, 8, 75, 255)` versus the reference
`(5, 8, 139, 255)`. After correction it is `(5, 8, 138, 255)`, within one
quantization level of that reference; opaque blue remains exact. Corrected
images and JSON are in `alpha-after/`; the original probe is in
`target/perf/gpu-followup-2026-11-15/`.

## Verification

- Workspace `check`, `fmt`, `structure`, and `lint` pass. Full workspace tests
  pass all 2,357 active cases with exact inventory coverage, plus doctests.
  Logs: `native-gates.log`, `native-gates-final.log`, and
  `target/ci/workspace-tests/20261115T161510Z-86664-811957ab5b/summary.json`.
- `wasm-check` passes in `nix develop .#web-build` (default and minimal web
  configurations plus the minimal-engine lint); see `wasm-check.log`.
- New GPU assertions cover backend absence/presence, plain/effect scene
  switches, independent transition sides, pixels, and resize. Engine tests
  cover mutation, custom capability overrides, cloning, nested scopes, and
  identity effects that activate later. GUI transition and Glass resize tests
  pass. These are native GPU checks; browser startup timing remains unmeasured.
- CPU `scripts/perf-bench.sh compare quick` uses the original 72-row baseline
  and unchanged thresholds. The first run (`cpu-compare.log`) reports one
  material regression in `code_compile__highlighted_palette` (+0.257 ms) and
  three inconclusive rows. That benchmark calls the unchanged text compiler
  directly, without constructing a Timeline or dispatching effects; this does
  not prove the cause of its result. The same-baseline repeat
  (`cpu-compare-repeat.log`) also exits 1: one material regression in that row
  (+0.222 ms), four inconclusive rows, and no coverage/policy failures. Neither
  whole-suite CPU screen is reported as passing.
- Following the measurement policy's adjacent-control protocol, the unchanged
  `code_compile/highlighted_palette` workload also ran from independently
  built `246fc540` and final-code binaries in five alternating A/B and B/A
  pairs, with the same 1-second warmup / 2-second measurement parameters.
  Paired deltas are +0.247, −0.006, −0.0005, +0.021, and +0.028 ms; the median
  is +0.021 ms (+0.98%), below its 0.1 ms action budget. The first pair still
  reproduces the larger deviation, so it remains in the record. This does not
  reproduce a consistent version-linked regression or establish a CPU speedup;
  it does not erase the two failed screens. Raw estimates, binary hashes, and
  paired results are in `text-control-summary.json`. The baseline build
  temporarily used the old source, then restored the final source bytes;
  `restored-source.patch` matches `final-source.patch` exactly.

## Remaining directions

- Audit Bloom and spatial filtering at translucent edges against the documented
  straight-alpha contract. Bloom still has a premultiplied-input assumption;
  this change corrects DropShadow only.
- Measure backend admission in a real WebGPU browser with fixed raster scale.
  Native first-frame measurements cannot establish browser startup savings.
- Investigate sharing Vello's final submission with the presentation blit.
  Pool reuse and image-resource lifetimes must be resolved before deferring
  submissions across independent players or frames.
- Deeper export pipelining needs explicit readback-slot ownership and bounded
  backpressure. Increasing the number of outstanding frame handles without
  that ownership contract can reuse a buffer still in flight.
