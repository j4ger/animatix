# Web performance baseline — 2026-11-15

This is a historical diagnostic baseline for `ed3764e4`, not a regression verdict or a
product SLO. The strongest measured opportunities are startup work for rich
content and the per-player submission/presentation cost of multiple animations.
The ordinary frame-evaluation CPU path is small in the three measured fixtures.
Actual page playback cadence remains unverified in this run.

> Archived on 2026-11-17. Source paths, payload sizes and budgets below describe
> the measured revision, not current main. The current page harness is
> `web/tools/test/web-page-perf.mjs`; the current primary gzip budget is 1.85 MB.
> Re-measure after later engine, layout and presentation changes before making
> current performance claims. Follow-up acceptance is tracked in the
> [roadmap](../roadmap.md#web-playback-performance-active).

## Identity and evidence

- Source: `ed3764e4b6add34d55938007aaa091fabddbbb3e`; no tracked source edits
  preceded the SDK build.
- Release SDK: `b6975ca588ec5560e8c9a4838fe94685d754446d1c3cff7193378ba615d611d7`.
  Rebuilt with `nix develop .#web-build -c scripts/build-web.sh`; current-input
  and commit verification passed before measurements.
- Chromium 146.0.7680.164, NVIDIA GeForce RTX 4060 Laptop GPU, native Vulkan,
  **headless hardware WebGPU**, 960×540 canvas, production build quality,
  1280×720 scene, fixed scale 0.85 / 1088×612 raster.
- Local loopback HTTP, no CPU/network throttling. These are local startup
  measurements, not WAN or mobile estimates. The diagnostic server serves raw
  JavaScript/JSON and the SDK's explicitly requested gzip Wasm/font files.
- Three fresh browser processes per fixture; each followed by one cache-enabled
  reload in the same process. A warm reload creates a new JS realm and GPU
  context; it is not a warm `applySource()` or an additional player in the same
  page. Browser/driver/OS caches can survive a process restart.
- Every engine-created device was verified as NVIDIA. Fixed-raster checks,
  actual-draw counters, zero deduplication assertions, and nonempty pixel
  readbacks passed for the measured fixtures.

Local evidence (gitignored):

| File under `target/perf/web-audit-2026-11-15/` | Purpose |
|---|---|
| `results.json` | 18 raw startup/frame runs; scale and 1/3/6-player sweeps; failed page attempts |
| `run.mjs` | Diagnostic driver reusing `page('fresh')` from `scripts/web-runtime-perf.mjs` |
| `run-headless.log` | Baseline progress and actual-page adapter errors |
| `detail-results.json`, `detail.log` | Separate single-repetition phase/compilation/resource-waterfall probes |
| `detail-eager-results.json`, `detail-eager.log` | Unsuccessful eager-loading control; no performance conclusion |

The original `web/runtime` was a different, dirty-source build from `31e78a9f`.
Its size/performance was not treated as current-source evidence. The retained
2026-11-13 legacy/current A/B status file also says **unverified** after browser
context timeouts; it supplies no successful comparative performance verdict.

## Startup baseline

Values are medians of three runs, in milliseconds. `ready` starts before the
embed module import and ends when the element becomes ready, sampled at 50 ms
intervals. It does not establish first on-screen GPU presentation time.
`load` measures `_startLoad()` itself. Prepare/build are nested stage details,
so the columns must not be added together.

| Fixture | Cold ready | Cold load | Warm reload ready | Warm reload load | Cold prepare | Cold synchronous build |
|---|---:|---:|---:|---:|---:|---:|
| English text + vectors | 808.0 | 359.5 | 267.6 | 247.8 | 51.1 | 41.5 |
| CJK text + vectors | 784.0 | 382.5 | 339.0 | 323.6 | 75.6 | 68.8 |
| Equation + Code + image | 1414.5 | 989.5 | 920.4 | 907.5 | 371.3 | 320.9 |

Cold ready ranges: English 724–1730 ms, CJK 783–793 ms, rich/media 1279–1483 ms.
The first English sample was slow in both the baseline and the separate detail
probe; retain it. Three samples cannot establish a tail-latency distribution.

There is an important attribution boundary: approximately 330–435 ms of the
ordinary cold ready samples elapsed **outside `_startLoad()`**; the first
English sample had about 1370 ms outside it. The resource waterfall puts the
manifest request after that delay. Lazy admission / visibility and browser
scheduling are included in `ready`; this is not all Wasm compilation time.
The eager control failed adapter discovery, so this run does not prove how much
of that delay production could remove.

For the rich/media fixture, warm preparation still took a median **370.9 ms**
and warm synchronous build **272.8 ms**. Caching downloads alone leaves a
substantial startup and main-thread stall problem on reload.

The separate phase probe (one repetition, diagnostic only) measured:

- Shared GPU engine initialization: 115–154 ms across the six cold/warm runs.
- Base `compileStreaming`: 40–62 ms; rich capability compilation: about
  155–157 ms. These asynchronous spans include waiting and are not CPU profiles.
- Rich/media `ensureCapabilities`: 233–239 ms, including dependencies,
  download/decompression/verification/compilation/instantiation.
- Rich cold ordering: capabilities started around 479 ms, image fetch around
  721 ms, fonts around 729 ms, GPU initialization around 785 ms. This confirms
  the serialized dependencies visible in the loader source.

## Frame and multiple-player baseline

Values below are medians across the three cold runs. CPU p95 is the median of
the last CPU sample group's p95 in each run. Each batch contains 90 advancing
frames followed by `debug_gpu_drain()`. Queue-depth-one p95 is computed within
each 30-frame sequence, with a drain after each frame, then medianed across runs.

| Fixture | CPU p50 | CPU p95 | Batch completion / frame | Queue-depth-one p95 | Wasm linear capacity |
|---|---:|---:|---:|---:|---:|
| English | 0.3 ms | 0.4 ms | 1.74 ms | 4.1 ms | 20.38 MiB |
| CJK | 0.3 ms | 0.5 ms | 1.89 ms | 5.0 ms | 23.75 MiB |
| Rich/media | 0.3 ms | 0.6 ms | 1.69 ms | 4.1 ms | 32.56 MiB |

Wasm capacity is not resident memory, peak memory, JS heap or GPU memory.
Batch completion includes CPU work, WebGPU/Dawn/driver overhead and queue
completion notification. It is **not pure GPU execution time**. Subtracting CPU
time does not isolate a GPU timestamp. Queue-depth-one also includes Promise
and scheduling overhead. Neither quantity is displayed playback FPS. This
distinction follows the separate content/device/queue timelines in the
[WebGPU specification](https://gpuweb.github.io/gpuweb/#dom-gpuqueue-onsubmittedworkdone).

The fixed-order multiple-player sweep uses the English fixture and one shared
device, 60 frames per player per batch, three repetitions per count:

| Simultaneous players | Median batch wall / page tick | Median CPU / page tick |
|---|---:|---:|
| 1 | 2.75 ms | 0.29 ms |
| 3 | 7.49 ms | 0.76 ms |
| 6 | 14.87 ms | 1.52 ms |

All requested draws occurred. Six players leave little of a 16.67 ms reference
60 Hz budget for the rest of the page under this synthetic sustained workload.
This does **not** prove that a real six-figure page drops frames: visibility,
held-frame deduplication, concurrency parking and adaptive cadence change the
actual draw schedule. Those mechanisms were deliberately excluded here.

Scale sweeps (three repetitions in a fixed order, medians):

| Scale / raster | English ms/frame | CJK ms/frame | Rich/media ms/frame |
|---|---:|---:|---:|
| 1.30 / 1664×936 | 2.01 | 2.22 | 2.33 |
| 0.85 / 1088×612 | 2.05 | 1.92 | 2.34 |
| 0.50 / 640×360 | 2.15 | 1.80 | 2.20 |
| 0.25 / 320×180 | 2.55 | 1.69 | 1.67 |

A 27× area reduction does not deliver a proportionate speedup. Some samples
are slower at a smaller raster; fixed ordering, clock state and browser noise
limit attribution. CJK/rich scenes do show some benefit. The supported conclusion
is to prioritize frame/submission count over further blanket resolution cuts
on this GPU, not that pixel cost or static caching can never matter elsewhere.

## Payload baseline

Current SDK artifact sizes, decimal MB. These are file sizes, not all of them
startup transfers:

| Module | Raw | Gzip |
|---|---:|---:|
| Base Wasm | 5.000 MB | 1.734 MB |
| Shared capability dependency | 0.578 MB | 0.252 MB |
| Rich text capability | 18.261 MB | 7.829 MB |
| Media capability | 0.508 MB | 0.205 MB |
| Raster encoder capability | 0.052 MB | 0.024 MB |

The raster encoder was not loaded by these playback fixtures. Font requests
were 0.114 MB / one face for English, 0.842 MB / two faces for CJK, and **2.038 MB
/ all eight faces** for rich/media. The latter includes both static and variable
Open Sans faces and the CJK fallback, because rich/dynamic requirements select
conservatively. It is a reduction opportunity that needs coverage/weight/style
validation, not evidence that those faces can simply be deleted.

The JSON manifest is 217,786 bytes before HTTP compression. The complete SDK is
38.49 MB, including raw/compressed alternatives and notices; that is not a
first-load download. Base gzip is only 16,285 bytes below its existing 1.75 MB
budget. The rich fixture requests about 10.02 MB of compressed Wasm plus 2.04 MB
of compressed fonts before considering JS/manifest/image bytes. At a hypothetical
10 Mbit/s, those 12.06 MB alone require about 9.65 seconds of transfer; this is
bandwidth arithmetic, not a measured WAN startup time.

## Optimization order

1. **Close the measurement gaps before changing the renderer.**
   `scripts/web-page-perf.mjs` still requires the retired
   `web/pkg/animatix_web_bg.wasm`, reads the retired embed bundle path, and cannot
   establish the new SDK's identity correctly. Migrate it to manifest/current
   SDK verification and record first-drawn completion, load admission, ready,
   actual draw cadence, scale, pressure and queue metrics separately. Keep the
   three fixtures and add a dense plot/effects/transition case and weaker GPU.
   The diagnostic script here is local evidence, not a new maintained gate.
2. **Shorten the startup dependency chain, retaining cancellation/diagnostics.**
   `web/embed/src/player/load.js` awaits `loadRuntime()` before `fetchSource()`;
   `view.js::_prepareDocument` awaits capabilities, then assets, then default
   fonts, then overlay fonts; GPU initialization follows preparation. Start
   independent source/resource/font fetches earlier and overlap GPU initialization
   with preparation where appropriate. External transcript assets must still
   enrich font requirements before final font selection. The measured
   capability and engine stages are 233–239 ms and 115–154 ms respectively;
   their overlap is a plausible tens-to-low-hundreds-of-ms opportunity, not an
   established saving. Validate with paired startup measurements and long tasks.
3. **Reduce rich-content bytes and synchronous compilation work.**
   Rich text is the dominant capability payload. Investigate finer partitioning
   of Code highlighting / Typst math and safe static-versus-variable font
   selection, then profile the 273–321 ms synchronous rich build. Preserve
   glyph coverage and arbitrary markup behavior. Reuse existing font-context
   and syntax caches: `web/prepared.rs` already pools contexts across live
   players, and `host/prepare.rs` already caches prepared syntax. Reload numbers
   do not show that these same-page caches are absent. A Worker-based preparation
   path could reduce main-thread stalls but needs a separate architecture and
   transfer-cost experiment; it is not automatically a startup speedup.
4. **Reduce submissions/draws per page tick.**
   `web/frame.rs` already uses a shared context and clone-free presentation
   evaluation. Do not propose those as missing optimizations. Each player still
   rasterizes independently, blits and presents; investigate an encode/submit
   seam that can combine raster/blit commands or page submissions. Existing
   fullscreen blit code already has encoder-taking/batched primitives. Confirm
   command counts and GPU timestamps before an invasive change. Preserve
   existing held-frame deduplication, offscreen pausing and page cadence
   arbitration. Measure 1/3/6-player completion and real page cadence together.

For these fixtures, a hypothetical 50% reduction in ordinary per-frame CPU work
would recover only about 0.15 ms per player; it is a lower priority than the
measured rich startup stalls and multiple-player completion cost. This does not
cover dense geometry, animated text content, effect chains or transitions.

## Environment limits and follow-up

The host loaded NVIDIA kernel module 595.104.02 but pointed userspace at
595.112.07. Matching, already-installed 595.104.02 libraries and ICD were selected
**only in measurement-process environment variables**, without changing system
drivers. Hardware headless measurements succeeded. The headed path failed EGL
initialization and also reported a host Mesa/GLIBC ABI mismatch.

Actual tour and transformer page attempts failed adapter discovery in the
headless environment. They provide no valid rAF/FPS, LCP or INP measurements.
The eager-loading diagnostic also failed adapter discovery and supplies no
performance comparison. After the display/driver environment is consistent,
rerun natural-viewport and all-visible pages with the repaired current-SDK page
harness. Include CPU and GPU identity, refresh rate, DPR, actual draw counts,
adaptive scale/cadence and multiple process repetitions. Do not turn a successful
smoke check or the historical 131.5 rAF/s result into a current playback claim.

The SDK uses `compileStreaming`, but its HTTP cache reuse must not be assumed to
prove persistent optimized Wasm code-cache hits: the loader creates a decoded
`Response`, and the detail runs still spend time compiling. Verify cache events
in a trace before redesigning this path. See V8's
[Wasm architecture](https://chromium.googlesource.com/v8/v8/+/lkgr/docs/wasm/architecture.md)
for the streaming compilation/cache distinction.
