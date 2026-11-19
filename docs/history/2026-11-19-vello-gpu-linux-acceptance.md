# Vello GPU Linux acceptance — 2026-11-19

This record covers the opt-in migration candidate at `06c35e24` and its
Compute baseline at `204b668a`. The release procedure and remaining decisions
are in the [migration handoff](../handoff_vello_gpu.md). The local evidence
directory is `target/ci/vello-gpu-acceptance/`.

## SDK and package checks

Both release SDKs came from clean checkouts using the same `.#web-build` flake,
Rust 1.99.1, wasm-bindgen 0.2.128, Binaryen 126 and Zopfli. The release identity
checks and cold raster PNG proof passed on both builds. The candidate's text
parity gate passed eight plain and two grouped cases.

| SDK | Source commit | Build ID |
| --- | --- | --- |
| Compute | `204b668a2f1ef4107957f421fa513ac05e3a4846` | `b23961c9524ed45e0053225d3a1e5844c1a1b005e6c0f30efca755db861103fb` |
| Vello GPU WebGPU + WebGL2 | `06c35e243616e6ceffc84f56dea6d0c693c34181` | `3bcc3c56e3284d663df03171ae328109b27f4867a45f9edf277ae97d18dc6b47` |

| Bytes | Compute | Vello GPU | Gate |
| --- | ---: | ---: | ---: |
| Base Wasm | 5,368,359 | 6,431,830 | 5,400,000 |
| Base Wasm gzip | 1,866,359 | 2,432,903 | 1,900,000 |
| All Wasm modules | 24,764,109 | 25,958,767 | 28,000,000 |
| Compressed font pack | 2,038,396 | 2,038,396 | 2,400,000 |
| SDK total | 38,292,998 | 40,185,627 | 42,000,000 |

The dual renderer adds 566,544 compressed base bytes (+30.4%) over Compute
and exceeds the current gzip limit by 532,903 bytes. The Compute SDK passes
the size gate. Both builds warn that the publisher could not find exact
license text for `fax`, `fax_derive` and `glidesort`.

## Linux browser coverage

The hardware runs used Chromium 156.0.8078.79 and Linux 7.2.14-xanmod1.
With Chromium Vulkan enabled and the NVIDIA ICD selected, both `auto` and
forced WebGPU rendered the expected cyan foreground. Dawn selected the
non-fallback NVIDIA Lovelace adapter. The unmodified desktop configuration
on this machine exposed no usable WebGPU adapter and `auto` presented via
the AMD Radeon 890M ANGLE/OpenGL renderer.

With WebGPU disabled and ANGLE on Vulkan, the WebGL2 smoke passed both
automatic fallback and forced selection at 393×851 CSS pixels and DPR 2.
ANGLE reported the GeForce RTX 4060 Laptop GPU with NVIDIA 595.112.07.
The smoke checked playback, rejection of an invalid edit, Glass, transitions,
an inline Filter, same-playhead recovery and diagnostic RGBA readback.
Ordinary presentation made zero application pixel readback calls. On a
six-player page, the first player recovered on a replacement canvas and the
other five kept their original canvases, visible foreground and 1 s playheads.

The forced WebGPU and WebGL2 screenshots cover the effect catalog and recipes,
camera, Glass, light vocabulary, raster images, SVG, Latin and CJK glyphs,
emoji, Typst, a shared-object transition and matrix shear. The largest mean
per-pixel maximum RGB-channel difference was 0.100 on the 0–255 scale, on
the shared-object transition. The sampled frames had matching alpha.

The production-quality WebGL2 probe drew every requested queue-depth-one
sample at scale 1. Fence completion p95 includes timer polling in Chromium.

| Scene | One player | Three players | Six players |
| --- | ---: | ---: | ---: |
| First Move | 8.3 ms | 9.3 ms | 13.0 ms |
| Glass | 10.1 ms | 14.6 ms | 23.6 ms |

The first 60 s foreground First Move trace recorded 3,296 compositor
display events and 54.7 player draws per second with one player. The
matching single-player Glass trace recorded 3,294 display events and
54.9 draws per second. The raw trace, player draws, scheduler drops and
whole-GPU memory samples are retained for each player count.

The local Web rollback loaded the candidate and then the previous immutable
SDK on a stable preview URL. Fresh WebGPU profiles reported the expected
build IDs and both displayed the same cyan foreground at 2.5 s.

The forced WebGPU A/B used the same scene, scale and player count on the
NVIDIA adapter. A second First Move run fixed the viewport at 1280×840 and
alternated five fresh-profile pairs. Median player readiness moved from
340.8 to 352.6 ms, median startup GPU completion from 732.4 to 539.2 ms,
and median frame completion p95 from 7.5 to 5.8 ms. One of those pairs had
20.1% slower readiness. The median readiness increase was 3.5%.

| Scene | Players | Compute frame p95 | Vello GPU frame p95 |
| --- | ---: | ---: | ---: |
| First Move | 1 | 7.5 ms | 5.8 ms |
| First Move | 3 | 16.8 ms | 9.7 ms |
| First Move | 6 | 26.2 ms | 20.1 ms |
| Glass | 1 | 16.5 ms | 11.5 ms |
| Glass | 3 | 52.9 ms | 31.1 ms |
| Glass | 6 | 95.9 ms | 43.9 ms |

The one-player rows summarize repeated pairs. The three- and six-player
rows each use one completed pair. An earlier three-player Compute Glass
run timed out, and the fixed-viewport retry completed.

## Linux native coverage

The CLI and GUI built with `video,vello-gpu` in the default Nix shell with
FFmpeg 8.0.1. The application logs identify the RTX 4060 Laptop GPU and
NVIDIA 595.112.07 Vulkan adapter. Three candidate frame contracts and 27
offscreen tests passed. The same 27 offscreen tests passed with the legacy
renderer. The BGRA harness explicitly asks for a fallback adapter, so the
complete offscreen run exposed both NVIDIA and the pinned Mesa Lavapipe ICD.
The shader unit tests also use the headless fallback helper by design. All
three passed, covering the built-in fragment pipelines and compute Pixelate.
The CLI also executed the actual Pixelate plugin on NVIDIA Vulkan with both
renderers.

The committed baseline and candidate both passed `check`, `test`,
`no-video`, `render-smoke`, and `examples`. The initial snapshot comparison
used a separate Cargo target path that the native smoke scripts could not
locate. The corrected runner puts each target directory in its own snapshot.
The workspace test reruns exposed the hardware and fallback ICDs on both
sides. The clean image and example gates each covered 72 fixtures.

On the NVIDIA adapter, the effect catalog and recipes at 1 s had mean
per-pixel maximum RGB-channel differences of 0.048 and 0.032. Camera at
3.5 s and Glass at 3 s differed by 0.079 and 0.080. The light vocabulary
sample at 4 s had a mean difference of 1.099, with 62,278 pixels differing
by more than three channel values and a maximum difference of 128. The
sweep, linear and radial ramps show visible differences. The legacy Vello
0.11 ramp converts stops to sRGB and does not use the authored interpolation
space. Vello GPU passes the interpolation space to its ramp encoder.
The two hardware frames and an amplified difference image are in
`native-media/`. The renderers' light vocabulary results require visual
review before adopting the candidate for deployed content.

Glass PNG and WebP exports decoded identically on each renderer. Both
narrated MP4 exports decoded to 48 H.264 frames, with 2 s video and contiguous
AAC packet timestamps. The 1 s frame comparison to PNG gave 40.80 dB PSNR
for legacy and 40.81 dB for Vello GPU.

Both GUI backends played, paused, sought to 3 s, resized and exported
1280×720 PNG and WebP on NVIDIA Vulkan. Switching the format tabs previously
kept the suggested `.mp4` destination and made image encoding fail. The fix
updates the suggested suffix while preserving a custom path. The release
GUI was retested through both export tabs. The full GUI library suite with
`video,vello-gpu` passed 304 tests, with one ignored.

An initial desktop run logged repeated ALSA `POLLERR` messages. The
evidence includes an excerpt. The export retest used an isolated X11 display
with the same NVIDIA Vulkan adapter.

The 24 fps brand reel chapter export produced the seven chapters in playback
order and a continuous audio track. The second run reused all seven parts and
audio. Editing the slogan reused Hub, Logo and audio, and rebuilt Slogan and
the downstream chapters that depend on the persisted actor. The same cache
directory produced misses on the first legacy run and hits on the second.
The resulting MP4 had 67.417 s of video and 67.400 s of audio.

The rollback rehearsal verified the checksums of the retained Linux package
at `97a168eb` and reopened a simple scene with that package in a preview
environment that supplied the Vulkan loader.

The native export driver measured 120 Glass frames at 1280×720 on the
same NVIDIA adapter. Legacy and Vello GPU ran in alternating order.
Throughput was 403.4 versus 362.2 fps and 694.2 versus 469.4 fps. The
candidate was slower in both pairs, with the exact magnitude varying
between runs. Native export throughput needs follow-up before a default
switch.

The CPU quick comparison matched all 85 saved baseline workloads. The
policy reported zero material regressions and three inconclusive rows
(`analyzer_update/large`, `analyzer_update/medium`, and
`code_compile/highlighted_palette`). Both runs passed the default load
preflight. The baseline estimates, candidate estimates, and verdict are
retained under `quick-baseline/`, `quick-candidate-logs/`, and
`quick-compare.log`.

A focused repeat of the three inconclusive workloads used 150 samples per
case and the same policy. All three passed. This repeat has its own raw
estimates and report under `quick-focused/`.

macOS and Windows package checks are available from predecessor CI runs.
The candidate has no CI run or hardware evidence on macOS, Windows, iOS or
Android. [macOS archive job](https://github.com/j4ger/animatix/actions/runs/44261436918/job/134655470586) ·
[Windows archive job](https://github.com/j4ger/animatix/actions/runs/44270696604/job/134686268521)

## Locked dependency graph

The minimal engine gate passes and both strict cleanup gates report
remaining legacy packages. The complete graph includes Vello Compute 0.11,
the historical Hybrid benchmark, and the separate Vello CPU versions
resolved through Typst and egui. `Cargo.lock` was byte-identical before and
after all three checks. The published Typst SVG 0.15.1 manifest still asks
for Hayro 0.7.1, and published epaint 0.37.0 still asks for Vello CPU 0.2.0.
[Typst SVG manifest](https://docs.rs/crate/typst-svg/0.15.1/source/Cargo.toml) ·
[epaint manifest](https://docs.rs/crate/epaint/0.37.0/source/Cargo.toml)

## Closeout measurements on 2026-11-20

The follow-up tested source `6bbf3e64` on the same RTX 4060 Laptop GPU.
Raw ledgers, screenshots and temporary experiment patches are under
`target/ci/vello-gpu-followup/`.

### Web cold startup

Both Vello GPU SDKs used the pinned `.#web-build` shell and the same clean
source. The WebGPU-only build passed release identity verification and the
cold PNG capability proof. Its 4,736,540 raw and 1,774,873 gzip base bytes
fit the published 5,400,000 and 1,900,000 byte limits. The dual SDK used
6,431,830 raw and 2,432,901 gzip base bytes. The WebGPU-only build ID is
`959e9e75b0bf03f0a6d091dbe6fefe1216aad413040b3c9c7fb098a06f7949bb`.
The dual build ID is
`89ffa890eeb260103aa8734a73bf9b81c765103a32882e50a39c6882587a9c31`.

The split prototype used one embed loader and these two immutable
manifests. It chose the dual SDK when the browser exposed no WebGPU API,
or after the primary SDK failed to initialize WebGPU. It measured initial
negotiation only. The dual SDK's existing device and context recovery
results are recorded above.

Each row uses five interleaved runs on the loopback server with fresh
Chromium profiles, browser caching disabled, one production-quality
First Move player and render scale 1. The missing-API case hid
`navigator.gpu`. The rejected case exposed the API and rejected
`GPU.requestAdapter()`, with the attempted calls counted. The WebGPU path
used the non-fallback NVIDIA adapter. The WebGL2 path used NVIDIA ANGLE
over Vulkan and disabled Chromium GPU compositing after fence failures
with the initial browser configuration. The first run of each row
checked the visible cyan foreground at 2.5 s. All runs counted zero
application pixel readbacks.

| Initial route | Wasm body bytes before ready | Resource transfer bytes before ready | Median ready | Median first GPU completion | Median frame completion p95 |
| --- | ---: | ---: | ---: | ---: | ---: |
| Compute WebGPU baseline | 1,866,359 | 2,209,776 | 293.3 ms | 664.0 ms | 6.5 ms |
| Dual, WebGPU succeeds | 2,432,901 | 2,812,279 | 313.1 ms | 437.3 ms | 4.9 ms |
| Split, WebGPU succeeds | 1,774,873 | 2,128,442 | 317.4 ms | 436.7 ms | 4.9 ms |
| Dual, WebGPU API absent | 2,432,901 | 2,812,279 | 404.6 ms | 420.6 ms | — |
| Split, WebGPU API absent | 2,432,901 | 2,812,645 | 402.7 ms | 416.9 ms | — |
| Dual, adapter request rejected | 2,432,901 | 2,812,279 | 400.8 ms | 415.3 ms | — |
| Split, adapter request rejected | 4,207,774 | 4,856,294 | 515.5 ms | 530.2 ms | — |

Transfer bytes use the browser's Resource Timing entries with completed
responses before `amxready`. Startup GPU completion uses a device queue
drain or a WebGL2 fence. The WebGL2 rows stopped after the startup fence.
The steady-state WebGL2 timing and foreground cadence above apply to the
dual SDK. The first Compute startup was an outlier at 2,087.4 ms to ready
and 2,627.9 ms to GPU completion. It remains in the five-run median.

The split prototype saved 658,028 Wasm bytes on successful WebGPU. An
adapter failure cost 1,774,873 additional Wasm bytes and increased median
startup GPU completion by 27.7% against the dual SDK, beyond the 10%
limit. The prototype stays out of the release candidate.

### Native Glass export

The export driver used 1280×720 Glass on NVIDIA Vulkan, with 30 warmup
frames and pipelined texture readback. Its throughput covers scene
evaluation, raster, effects and readback. FFmpeg encode is measured in
the separate video proof above.

Three alternating 600-frame runs had a median of 364.5 fps for Compute
and 323.3 fps for Vello GPU, or 88.7% of Compute. A second experiment
rotated the order of Compute, the unchanged candidate and a patch that
replaced the destination sample and scratch copy with premultiplied
fixed-function blending. Each configuration ran 1,200 frames six times.

| Renderer | Median export fps | Run range |
| --- | ---: | ---: |
| Compute | 658.9 | 482.4–702.2 |
| Vello GPU | 437.1 | 374.0–507.1 |
| Vello GPU with in-place blend | 379.3 | 306.9–504.6 |

The improvement changed sign across runs. The patch was reverted. Both
sets of runs left the candidate below the 90% of Compute throughput
target.

An instrumented candidate run sampled 360 frames, including 286 that
painted Glass. For those frames, median CPU elapsed time in the main
raster call was 1.484 ms and the isolated children pass took 0.516 ms.
The backdrop seed, effects chain, backdrop composite and children
composite took 0.044, 0.134, 0.061 and 0.070 ms respectively. A second
instrumented run split the main raster call into scene preparation
(0.640 ms), Vello command encoding (0.163 ms) and queue submission
(0.187 ms). These are median CPU times from a separate run. Readback
accounts for GPU completion. The temporary probes were removed.

Reusing the Vello GPU scene and resetting it between raster calls raised
the median from 388.3 to 394.8 fps over another six rotated 1,200-frame
runs. Compute measured 540.9 fps in that series. The 1.7% change was
small compared with run-to-run variation and left throughput at 73.0%
of Compute. The reuse patch was reverted.

The paired hardware crop for `32_light_vocabulary.amx` at 4 s shows the
same shapes, stroke and text placement. The sweep arc and the bar ramp
show the color interpolation difference described above. The crop is
`target/ci/vello-gpu-acceptance/native-media/light-pair.png`.

### Cleanup status

`minimal-engine` passed again. `wasm-no-legacy` and `full-no-legacy`
both reported remaining packages and left `Cargo.lock` byte-identical.
The published manifest for Hayro 0.7.1 depends on Vello CPU 0.0.8.
[Hayro manifest](https://docs.rs/crate/hayro/0.7.1/source/Cargo.toml)
