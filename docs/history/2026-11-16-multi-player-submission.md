# Browser multi-player presentation A/B — 2026-11-16

This is the successful browser follow-up to the unverified attempts recorded in
the [render and presentation submission note](2026-11-16-presentation-submit.md).
It measures the one-render-plus-present submission change on real WebGPU for
fixed-scale, manually driven players. It does not measure `requestAnimationFrame`
cadence or displayed FPS.

## Result

The source-path count reduction is exact: for every measured player frame, the
control submits Vello's render and the presentation blit separately (two
`GPUQueue.submit` calls), while the merged SDK submits them together (one call).
All requested draws occurred with zero deduplication. The browser A/B did not
show a consistent latency change: every queue-depth-one policy check is
**inconclusive** under the existing provisional 5% relative and 0.167 ms
absolute thresholds. No browser FPS or general speedup claim follows from these
three process pairs.

The English text/vector fixture ran with 1, 3, and 6 active players. Six player
elements were loaded in a visible 3×2 grid for every page; the rich-text/PNG
fixture was an additional one-active-player case. Each element used a 960×540
canvas and fixed 0.85 render scale, producing a 1088×612 scene raster from the
1280×720 fixtures. Warm and final readbacks were nonempty in all 24 measurements.
Every paired final per-player readback hash matched between control and merged
SDKs; all 24 rows have zero validation issues.

The browser harness calls `debug_readback_rgba(0)`, which re-renders fixed
timeline time zero into the offscreen target. It does not sample the live canvas
or the just-presented frame. Equal hashes are an offscreen scene-content
sanity-check, not direct browser presented-pixel parity; the native
presentation-target pixel tests provide that target-level check.

The engine made one `GPUAdapter.requestDevice()` call as the six players loaded;
the request count stayed at one after players two through six were ready. This
is consistent with the players sharing the page's engine device. The harness
records request counts, not JavaScript `GPUDevice` object identity.

## Browser, adapter, and SDK identity

- Chromium 146.0.7680.164 ran headless with ANGLE/Vulkan. CDP reported
  `ANGLE (AMD, Vulkan 1.4.335 (AMD Radeon 890M Graphics (RADV STRIX1)
  (0x0000150E)), radv-26.0.3)`. The engine-reported adapter was
  `vendor=amd`, `architecture=rdna-2`. `VK_ICD_FILENAMES` contained only the
  pinned Mesa 26.0.3 Radeon ICD, excluding lavapipe. The elevated browser
  processes could see `/dev/dri/renderD128` and `/dev/dri/renderD129`.
- CDP reported Vulkan and WebGPU enabled in five of the six process snapshots.
  The first control process, queried before page initialization, reported
  `webgpu=enabled_readback` and `gpu_compositing=disabled_software`; its engine
  still requested the AMD adapter, rendered all frames, and passed the same
  draw/readback checks. The raw SystemInfo objects are retained per process.
  This asymmetry independently limits attribution of end-to-end presentation
  timing; it does not turn the engine's AMD adapter into a software adapter.
- The preserved control SDK is
  `116fbfb0fc1c38e679323b94f32b9ac4939eec029b5104a0d883219a7e32e545`
  (manifest SHA-256 `0c8d62b5f5e455901122b7b9245bb763256d89a13fde154ece84402e990a0f7d`).
  The preserved merged SDK is
  `4900538d480625b046b471be337e83525144086c36af2a110217a1e25d4dceac`
  (manifest SHA-256 `37bdad4b90313671b00a9c4ed0ea58cfe8d9be31b6281701e0f44c8786e69f52`).
  All 31 listed files in each package passed hash, size, and gzip verification.
- These SDKs are the previously preserved pair from source commit
  `cc1adfcfe164b2db1f40d5f5426531b03f835370`; the comparison deliberately does
  not claim current-input SDK verification. Their recorded change is only the
  surface wrapper argument: `Some(surface)` for merged versus `None` for the
  existing separate-render/blit control. The current `frame.rs` SHA-256
  `b7ca86338edc9110e403d7059c402a7594677ad649e177ff222fc78adcee32b9` matches
  the restored final source hash in the SDK provenance. The fixtures contain no
  Filter/Glass/Blur/MotionBlur effect, so this historical SDK pair measures the
  plain presentation path and the rich-text/PNG path only.

The compared schedule was three independent browser process pairs in
control/merged, merged/control, control/merged order. Raster scale, fixture
content, browser version, ICD, and runtime SDK artifacts stayed fixed within
each pair. The source and SDK provenance are in
`target/perf/gpu-round3-2026-11-16/browser-ab/provenance.json`.

## Measurement method

For each active player count, the page warmed each active renderer and drained
the queue before measurement. CPU encoding samples comprise three repetitions
of 90 advancing page ticks; each tick calls `render_frame` on every active
player and does not wait for GPU completion. The batch sample encodes 90 page
ticks then calls `debug_gpu_drain()` once; its per-tick value includes CPU work
and queue completion. Queue-depth-one samples call `render_frame` and then
`debug_gpu_drain()` for each player, one at a time. Their per-player p95 is the
closest match to the existing queue-depth-one screen. The page-tick p95 for
three or six players sums multiple sequential render-and-drain operations; it
is reported as diagnostic work and is not an actual browser page cadence.

CPU and drain times include JavaScript/WASM and browser queue-notification
overhead. `performance.now()` values are quantized at this workload's scale.
The batch and drain measurements are not GPU timestamps. No sample called
`requestAnimationFrame`, and no displayed-frame counter was used.

### English text and vectors

Each pair cell shows `control / merged` milliseconds. Arm medians and ranges are
across the three independent process-pair results. Paired deltas are merged
minus control; their intervals are two-sided 95% Student-t intervals over the
three paired differences. Table values are rounded to 0.001 ms; raw per-frame
samples remain in the result JSON.

#### One active player

| Metric | Pair 1 C/M | Pair 2 C/M | Pair 3 C/M | Control median [range] | Merged median [range] | Paired Δ median [range]; mean Δ 95% CI |
|---|---:|---:|---:|---:|---:|---:|
| CPU page tick p50 | 0.300 / 0.300 | 0.300 / 0.300 | 0.300 / 0.300 | 0.300 [0.300–0.300] | 0.300 [0.300–0.300] | 0.000 [0.000–0.000]; [0.000, 0.000] |
| CPU page tick p95 | 0.600 / 0.600 | 0.600 / 0.600 | 0.700 / 0.600 | 0.600 [0.600–0.700] | 0.600 [0.600–0.600] | 0.000 [-0.100–0.000]; [-0.177, 0.110] |
| Drained batch / tick | 0.538 / 0.481 | 0.522 / 0.504 | 0.558 / 0.386 | 0.538 [0.522–0.558] | 0.481 [0.386–0.504] | -0.057 [-0.172–-0.018]; [-0.282, 0.117] |
| Queue-one player p95 | 3.700 / 4.200 | 4.100 / 4.200 | 4.200 / 3.700 | 4.100 [3.700–4.200] | 4.200 [3.700–4.200] | 0.100 [-0.500–0.500]; [-1.217, 1.284] |
| Queue-one page tick p95 | 3.700 / 4.200 | 4.100 / 4.200 | 4.300 / 3.700 | 4.100 [3.700–4.300] | 4.200 [3.700–4.200] | 0.100 [-0.600–0.500]; [-1.383, 1.383] |

#### Three active players

| Metric | Pair 1 C/M | Pair 2 C/M | Pair 3 C/M | Control median [range] | Merged median [range] | Paired Δ median [range]; mean Δ 95% CI |
|---|---:|---:|---:|---:|---:|---:|
| CPU page tick p50 | 0.700 / 0.700 | 0.700 / 0.600 | 0.700 / 0.700 | 0.700 [0.700–0.700] | 0.700 [0.600–0.700] | 0.000 [-0.100–0.000]; [-0.177, 0.110] |
| CPU page tick p95 | 1.400 / 1.100 | 1.300 / 1.100 | 1.400 / 1.300 | 1.400 [1.300–1.400] | 1.100 [1.100–1.300] | -0.200 [-0.300–-0.100]; [-0.448, 0.048] |
| Drained batch / tick | 1.507 / 0.902 | 1.114 / 1.047 | 1.058 / 1.067 | 1.114 [1.058–1.507] | 1.047 [0.902–1.067] | -0.068 [-0.604–0.009]; [-1.051, 0.609] |
| Queue-one player p95 | 4.200 / 4.200 | 4.000 / 3.800 | 4.600 / 3.700 | 4.200 [4.000–4.600] | 3.800 [3.700–4.200] | -0.200 [-0.900–0.000]; [-1.541, 0.807] |
| Queue-one page tick p95 | 11.600 / 12.700 | 11.300 / 11.000 | 12.800 / 10.700 | 11.600 [11.300–12.800] | 11.000 [10.700–12.700] | -0.300 [-2.100–1.100]; [-4.419, 3.552] |

#### Six active players

| Metric | Pair 1 C/M | Pair 2 C/M | Pair 3 C/M | Control median [range] | Merged median [range] | Paired Δ median [range]; mean Δ 95% CI |
|---|---:|---:|---:|---:|---:|---:|
| CPU page tick p50 | 1.300 / 1.200 | 1.300 / 1.200 | 1.300 / 1.200 | 1.300 [1.300–1.300] | 1.200 [1.200–1.200] | -0.100 [-0.100–-0.100]; [-0.100, -0.100] |
| CPU page tick p95 | 3.000 / 2.000 | 2.100 / 1.900 | 2.800 / 1.900 | 2.800 [2.100–3.000] | 1.900 [1.900–2.000] | -0.900 [-1.000–-0.200]; [-1.783, 0.383] |
| Drained batch / tick | 2.210 / 2.236 | 2.203 / 1.863 | 2.956 / 1.861 | 2.210 [2.203–2.956] | 1.863 [1.861–2.236] | -0.340 [-1.094–0.026]; [-1.889, 0.949] |
| Queue-one player p95 | 3.900 / 4.900 | 4.600 / 4.300 | 3.800 / 3.800 | 3.900 [3.800–4.600] | 4.300 [3.800–4.900] | 0.000 [-0.300–1.000]; [-1.458, 1.924] |
| Queue-one page tick p95 | 20.700 / 43.200 | 41.400 / 49.500 | 20.700 / 20.700 | 20.700 [20.700–41.400] | 43.200 [20.700–49.500] | 8.100 [0.000–22.500]; [-18.112, 38.512] |

### Rich text and PNG image, one active player

`rich-media.amx` contains Equation and Code text plus a PNG image; it does not
contain SVG. All six player elements were still loaded, with one active during
the measured phases.

| Metric | Pair 1 C/M | Pair 2 C/M | Pair 3 C/M | Control median [range] | Merged median [range] | Paired Δ median [range]; mean Δ 95% CI |
|---|---:|---:|---:|---:|---:|---:|
| CPU page tick p50 | 0.200 / 0.200 | 0.200 / 0.300 | 0.200 / 0.200 | 0.200 [0.200–0.200] | 0.200 [0.200–0.300] | 0.000 [0.000–0.100]; [-0.110, 0.177] |
| CPU page tick p95 | 0.300 / 0.300 | 0.300 / 0.400 | 0.300 / 0.400 | 0.300 [0.300–0.300] | 0.400 [0.300–0.400] | 0.100 [0.000–0.100]; [-0.077, 0.210] |
| Drained batch / tick | 0.422 / 0.432 | 0.432 / 0.440 | 0.464 / 0.436 | 0.432 [0.422–0.464] | 0.436 [0.432–0.440] | 0.008 [-0.029–0.010]; [-0.058, 0.051] |
| Queue-one player p95 | 5.200 / 4.100 | 7.200 / 7.400 | 4.100 / 4.100 | 5.200 [4.100–7.200] | 4.100 [4.100–7.400] | 0.000 [-1.100–0.200]; [-2.039, 1.439] |
| Queue-one page tick p95 | 5.200 / 4.100 | 7.200 / 7.400 | 4.100 / 4.100 | 5.200 [4.100–7.200] | 4.100 [4.100–7.400] | 0.000 [-1.100–0.200]; [-2.039, 1.439] |

The established queue-depth-one screen in
[`web-runtime-perf.mjs`](https://github.com/j4ger/animatix/blob/a8e268f9c0cc4491c36024140722b95fb41d1547/scripts/web-runtime-perf.mjs) compares per-player
render-and-drain p95 values, not multi-player page-tick sums. Its 5% relative /
0.167 ms screen is inconclusive in every group:

| Fixture / active | Merged/control ratio [95% CI] | Merged−control ms [95% CI] | Status |
|---|---:|---:|---|
| English / 1 | 1.008 [0.734, 1.384] | 0.033 [-1.217, 1.284] | Inconclusive |
| English / 3 | 0.914 [0.689, 1.213] | -0.367 [-1.541, 0.807] | Inconclusive |
| English / 6 | 1.055 [0.718, 1.551] | 0.233 [-1.458, 1.924] | Inconclusive |
| Rich text + PNG / 1 | 0.932 [0.649, 1.339] | -0.300 [-2.039, 1.439] | Inconclusive |

The six-player page-tick p95 includes six sequential queue drains. Its larger
values show the cost of this serial queue-depth-one probe, not six-player
`requestAnimationFrame` cadence. CPU p50 values also fall at the timer's
0.1 ms granularity; treat sub-0.1 ms differences as unresolved.

## Failed attempts and retained evidence

The initial round3 browser A/B attempts failed adapter discovery; they are
documented in the preceding [submission note](2026-11-16-presentation-submit.md).
Round4 then isolated the environment issue. The initial no-render Chromium
probe ran in the default sandbox, where host GPU device nodes were not visible;
its adapter failure was not evidence that the machine lacked a GPU. Elevated
Chromium 146 and 156 probes with the current
NVIDIA driver still returned no WebGPU adapter; Chromium 156 also logged
`Failed to send GpuControl.CreateCommandBuffer`. The cause remains unknown.
An earlier preflight also stopped at an obsolete SDK initializer, and the next
Mesa preflight exposed every Mesa ICD (including lavapipe) plus the system
OpenGL driver path; that setup found no adapter. These were setup failures,
not measured browser runs.
An elevated Chromium 146 probe with the pinned Mesa 26.0.3 Radeon ICD only
successfully initialized the SDK on the Radeon 890M and was used for the
measurements.

The first full attempt on that hardware reached all six browser processes but
ended before frame measurements because the temporary page queried the SDK
engine singleton before the first player initialized it. That run contains 12
page failures and zero measurements; the temporary runner's exit status was
corrected so collected failures now return nonzero. After moving the lookup
after player readiness, the retry completed all 24 rows with no validation
issues. All adapter attempts and both full-run directories remain under
`target/perf/gpu-round4-2026-11-16/browser/`.

## Evidence files

The successful run's ignored local artifacts are:

- `target/perf/gpu-round4-2026-11-16/browser/multi-player-ab/amd-146-3pair-retry01-20261116-0957/results.json` — raw per-player and per-phase samples, adapter/SystemInfo data, SDK manifests, schedule, and validation results.
- The adjacent `events.ndjson` and six `chromium-pair-*.stderr.log` files — sample progress and browser logs.
- `target/perf/gpu-round4-2026-11-16/browser/multi-player-ab/amd-146-3pair-retry01-20261116-0957/paired-summary.json` — pair values, medians/ranges, paired confidence intervals, hash checks, submit counts, and queue policy results.
- `target/perf/gpu-round4-2026-11-16/browser/analyze-multi.mjs` — read-only summary generator.
- `target/perf/gpu-round4-2026-11-16/browser/attempt-01-old-initializer/` and `attempt-02-pinned-mesa-no-adapter/` — retained preflight setup failures.
- `target/perf/gpu-round4-2026-11-16/browser/attempt-05-pinned-mesa-amd-chrome146/` — successful no-render adapter preflight.
- `target/perf/gpu-round4-2026-11-16/browser/attempt-03-current-nvidia-no-adapter/` and `attempt-04-current-store-nvidia-chrome156/` — elevated current-driver probes that still failed adapter initialization.
- `target/perf/gpu-round4-2026-11-16/browser/multi-player-ab/amd-146-3pair-20261116-0951/` — retained initialization-order failure; it contains no frame timing.

These browser measurements complement the mixed native measurements in the
[submission implementation record](2026-11-16-presentation-submit.md). The
verified outcome is the per-frame submission reduction and equal fixed-scene
readbacks. Latency remains unresolved at this sample size and on this single
adapter; browser playback cadence remains unmeasured.

## Supplement: one drain per page tick

The original 24-row tables above retain the original three measured phases and their original three process pairs. This supplement is a separate run; it does not replace those rows or pool them into a six-pair estimate. It added only a `page_depth_one` phase to the same 1/3/6 English and one-player rich-text+PNG matrix. Each measured tick rendered every active player first, then called `debug_gpu_drain()` once. The phase ran for 30 ticks at each active count. It measures CPU encoding plus one queue-completion wait, not frame presentation or `requestAnimationFrame` cadence.

The page now pauses at a ready/start handshake after all six players load. Before releasing timing, the Node driver samples CDP `SystemInfo.getInfo` and stores the full post-initialization object for each process and fixture. All 12 post-ready snapshots (six process arms × two fixtures) reported the same hardware/backend state: `gpu_compositing=disabled_software`, `webgpu=enabled_readback`, `vulkan=enabled_on`, and ANGLE Vulkan on the AMD Radeon 890M / RADV 26.0.3. The engine also requested only AMD hardware adapters. The before-page snapshots varied across arms (`gpu_compositing` was enabled in three and disabled in three), which is why they are retained separately; after player initialization, the measured arms had matching reported compositor states. These results describe the WebGPU queue path on the Radeon, with Chromium's HTML compositor in software mode.

| Process pair / order | Control before page → after ready | Merged before page → after ready |
|---|---|---|
| 1 / AB | `disabled_software` / `enabled_readback` → `disabled_software` / `enabled_readback` | `enabled` / `enabled` → `disabled_software` / `enabled_readback` |
| 2 / BA | `enabled` / `enabled` → `disabled_software` / `enabled_readback` | `disabled_software` / `enabled_readback` → `disabled_software` / `enabled_readback` |
| 3 / AB | `enabled` / `enabled` → `disabled_software` / `enabled_readback` | `enabled` / `enabled` → `disabled_software` / `enabled_readback` |

Each cell lists `gpu_compositing` / `webgpu`; both fixture snapshots in each process reported the same after-ready values.

Each cell below is a process-pair p50 or p95 in milliseconds, written control/merged in AB, BA, AB pair order. Arm medians and ranges are across the three pair-level percentiles. Paired delta is merged minus control; its 95% CI is a two-sided Student-t interval over the three independent pair deltas, not over the individual ticks. Raw 30-tick samples are in the results JSON.

### Page-tick p50

| Fixture / active players | Pair 1, 2, 3 C/M | Control median [range] | Merged median [range] | Paired Δ median [range] | Mean Δ 95% CI |
|---|---|---:|---:|---:|---:|
| English / 1 | 3.200/3.400, 3.200/3.300, 3.300/3.400 | 3.200 [3.200–3.300] | 3.400 [3.300–3.400] | 0.100 [0.100–0.200] | [-0.010, 0.277] |
| English / 3 | 5.200/4.000, 4.400/4.300, 4.400/4.200 | 4.400 [4.400–5.200] | 4.200 [4.000–4.300] | -0.200 [-1.200–-0.100] | [-2.011, 1.011] |
| English / 6 | 7.100/5.600, 5.800/5.800, 6.100/6.200 | 6.100 [5.800–7.100] | 5.800 [5.600–6.200] | 0.000 [-1.500–0.100] | [-2.693, 1.760] |
| Rich text + PNG / 1 | 3.300/3.300, 3.300/3.200, 4.100/3.400 | 3.300 [3.300–4.100] | 3.300 [3.200–3.400] | -0.100 [-0.700–0.000] | [-1.207, 0.674] |

### Page-tick p95

| Fixture / active players | Pair 1, 2, 3 C/M | Control median [range] | Merged median [range] | Paired Δ median [range] | Mean Δ 95% CI |
|---|---|---:|---:|---:|---:|
| English / 1 | 3.500/3.700, 3.800/3.600, 7.100/3.800 | 3.800 [3.500–7.100] | 3.700 [3.600–3.800] | -0.200 [-3.300–0.200] | [-5.859, 3.659] |
| English / 3 | 6.100/4.800, 5.300/5.400, 6.800/4.900 | 6.100 [5.300–6.800] | 4.900 [4.800–5.400] | -1.300 [-1.900–0.100] | [-3.583, 1.516] |
| English / 6 | 8.400/6.800, 6.400/10.800, 11.100/7.400 | 8.400 [6.400–11.100] | 7.400 [6.800–10.800] | -1.600 [-3.700–4.400] | [-10.743, 10.143] |
| Rich text + PNG / 1 | 3.900/3.700, 3.400/3.800, 5.000/3.600 | 3.900 [3.400–5.000] | 3.700 [3.600–3.800] | -0.200 [-1.400–0.400] | [-2.677, 1.877] |

Every p50 and p95 interval crosses zero; applying the existing provisional 5% relative / 0.167 ms screen to these page-tick percentiles also yields inconclusive results for all four groups. The 0.1 ms timer granularity and process-to-process spread remain visible, especially for the six-player p95. This phase closes the protocol gap but does not establish a consistent latency improvement.

All 24 supplemental rows passed validation. Each page-depth phase recorded 30 draws per active player, zero deduplication, and exactly two submits per draw for control versus one for merged (for six players, 360 versus 180 submits). All 33 paired final image hashes matched between arms, and warm/final readbacks were nonempty. As above, `debug_readback_rgba(0)` rerenders fixed time zero to the offscreen target: this hash match is a scene-content sanity-check and is not presented-canvas pixel parity.

The supplemental run used the same historical paired SDKs and Chromium/AMD setup recorded above. It verified and recorded the current production `web/runtime` independently as build `ca8ed41997507c2251d57ffe35abb0620babd97243d12c687751d02fd4f75cc9` (31/31 manifest files; manifest SHA-256 `3aa9ac3e660b2b105d922d75f96d182f4a2b1f0b5a288aea5a082df3f521921d`); it did not switch or rebuild that runtime. The current production Web frame source still matches the historical pair's preserved source hash. The historical comparison remains the plain presentation and rich-text/PNG path, with no Filter, Glass, Blur, or MotionBlur effect.

Supplemental evidence is under `target/perf/gpu-round4-2026-11-16/browser/page-depth-one/`:

- `multi-player-ab/amd-146-page-depth-one-3pair-20261116-1038/results.json` — all raw phase samples, full pre-page and post-ready SystemInfo snapshots, SDK provenance, and validations.
- The adjacent `events.ndjson` and six Chromium stderr logs — progress, submit counts, and browser output.
- `multi-player-ab/amd-146-page-depth-one-3pair-20261116-1038/paired-page-depth-summary.json` — per-pair tick percentiles, confidence intervals, compositor state, and count validation.
- `multi-player-page.html`, `multi-run.mjs`, and `analyze-page-depth.mjs` — the supplemental protocol and read-only analyzer. `original-protocol/` holds byte-identical copies of the prior HTML and runner; `protocol-manifest.json` hashes these copies and the prior successful run.
