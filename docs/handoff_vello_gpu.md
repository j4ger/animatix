# Vello GPU release handoff

The default renderer is Vello GPU for Web playback, CLI raster and video
exports, and GUI preview and exports. The single Web SDK uses WebGPU when
available and falls back to WebGL2. Native plugins with compute effects
continue to use their declared WGSL and the existing effect ABI.

The [migration archive](history/2026-11-19-vello-gpu-migration-baseline.md)
records functional parity. The [Linux acceptance record](history/2026-11-19-vello-gpu-linux-acceptance.md)
retains the immutable Compute SDK from commit `204b668a`, the tested Vello GPU
SDKs, and the paired startup and export measurements. Use those builds and
raw results as references for later optimization rounds. Report package size
and performance data independently of functional migration acceptance.

## Verify the SDK and native packages

Build from a clean checkout in the pinned shells. Keep the previous SDK and
native packages available for release rollback.

```sh
nix develop .#web-build -c tools/ci/ci.sh gate wasm-check
nix develop .#web-ci -c tools/ci/ci.sh gate web-build
nix develop .#web-ci -c tools/ci/ci.sh gate text-parity
nix develop .#web-ci -c tools/ci/ci.sh gate web-raster-proof
nix develop .#web-ci -c tools/ci/ci.sh gate web-smoke
nix develop .#web-ci -c tools/ci/ci.sh gate web-site-smoke
nix develop -c env ANIMATIX_REQUIRE_GPU=1 tools/ci/ci.sh gate test
nix develop -c cargo build --locked --release -p animatix-cli -p animatix-gui --features video
```

For browser acceptance on Linux hardware, confirm the standard embed selects
WebGPU. Serve the SDK on port 8124 and run
`node web/tools/test/vello-gpu-webgl-smoke.mjs` with
`WEBGL_CANDIDATE_GPU=1` for both automatic WebGL2 fallback and forced WebGL2
(`WEBGL_CANDIDATE_MODE=webgl2`). Record Chromium's renderer, SDK build ID,
and the player's backend. The smoke exercises edit rollback, Glass, transitions,
context recovery and presentation without CPU pixel readback.

Use `tools/release/native/video-proof.sh` for packaged video smoke on macOS
and Windows. On Linux, export a narrated fixture with the release CLI and
check the decoded video and audio streams. Check the GUI on the same release
package for playback, seeking, preview resize and export. Retain package and
browser acceptance results for macOS Metal, Windows DX12, iOS Safari and
Android alongside the Linux results.
After publishing an authorized release, observe first-frame and recovery
failures and restore the previous immutable package on a reproducible blank
frame or loss of state.

## Dependency audit

The workspace renderer uses `vello_gpu` 0.4.0. The Compute crate and its
shader package, the local Vello vendor directory and the Hybrid benchmark
have been removed. Run the locked audit with the following commands.

```sh
nix develop -c python3 tools/ci/dependency-gates.py
nix develop -c cargo tree --locked --workspace --all-features --target all -i vello_cpu@0.0.8
nix develop -c cargo tree --locked --workspace --all-features --target all -i vello_cpu@0.2.0
```

The strict Wasm and workspace checks also report third-party Vello packages.
Typst 0.15.1 brings Hayro's CPU PDF rasterizer into the rich-text dependency
graph through its HTML/SVG stack. The paged realizer also imports the HTML
crate directly. On native GUI builds epaint 0.37.0 uses `vello_cpu` for its
font atlas. Removing those edges requires upstream changes or maintained forks
of dependencies that provide rich text and GUI chrome.

| Check | Result after switching defaults |
| --- | --- |
| `minimal-engine` | Pass. The engine without optional features has no graphics device dependencies. |
| `wasm-no-legacy` | Reports `vello_cpu` and `vello_common` 0.0.8 through Typst and Hayro. |
| `full-no-legacy` | Also reports the epaint stack at 0.2.0 and a lockfile-only `vello_common` 0.0.9 entry. |

## Linux migration verification

With the default renderer on NVIDIA Vulkan, the effects catalog exported
PNG and WebP with identical decoded pixels. The narrated fixture decoded to
48 H.264 frames and two seconds of audio. The 48 decoded Glass video frames
matched the previous Vello GPU build. The Brand Reel chapter export kept seven
chapters in order and reused all parts and audio on repeat. Editing the slogan
reused the two unaffected chapters and audio and rebuilt the edited and
dependent chapters.

The GUI played and paused Glass, sought to three seconds, retained its preview
after a window resize, and exported both PNG and WebP at 1280 × 720. The
workspace tests and both Wasm feature configurations passed.

The packaged SDK passed text parity, cold PNG encoding, player transactions,
and the assembled-site smoke. Chromium on the RTX 4060 Laptop GPU used
WebGPU by default and fell back to ANGLE/Vulkan WebGL2 when WebGPU was disabled.
Both renderers presented without CPU pixel readback. A WebGL2 context-loss
test replaced one canvas and preserved all six players' playheads. The
rollback rehearsal restored the Compute SDK and reproduced the sampled frame.

## Historical evidence from the opt-in builds

### Cloud checks completed on 2026-11-19

The Linux GUI passed `cargo check --locked -p animatix-gui --all-targets
--features vello-gpu` after adding the matching ALSA development package in
scratch storage. The workspace all-targets check passed after migrating the
caption fixture to `Scene::draws()` and correcting the concrete filter backend
used by the GPU interaction example. All 121 engine contract tests passed,
including seven caption and speech-sync tests.

The candidate CLI exported PNG and WebP through SwiftShader.
Both files decoded to the same 1280×720 pixels and matched the sampled shape and
background colors. SwiftShader also exercised per-canvas fences with 1, 3 and
6 WebGL2 players and zero pixel readbacks. The SDK and supporting JavaScript
suites passed 16 and 48 tests respectively.

Both release SDKs passed integrity verification and the cold raster capability
proof. The repository build script ran with the same host toolchain for both
artifacts. The manifests record Rust 1.100.0, wasm-bindgen 0.2.128, Binaryen 130
and the compressor identity.

| SDK | Source | Build ID |
| --- | --- | --- |
| Compute baseline | `204b668a` | `1c90db7c4d23769f6c464c2fca315fa9fd625d9a42a1f1a34f3cfd7c3803c302` |
| Vello GPU candidate | `d2d35423` | `2604023bf576067031b7d571c49173d7e6a5e2bc3871e2279dc621737eca82b6` |

| SDK bytes | Baseline | Candidate | Budget |
| --- | ---: | ---: | ---: |
| Base Wasm | 5,365,940 | **7,843,036** | 5,400,000 |
| Base Wasm gzip | 1,866,411 | **2,822,974** | 1,900,000 |
| All Wasm modules | 24,696,767 | 27,152,024 | 28,000,000 |
| Compressed fonts | 2,062,576 | 2,062,576 | 2,400,000 |
| SDK total | 38,359,409 | 41,830,242 | 42,000,000 |

The baseline passed the SDK size gate. This early candidate exceeded the
published limits by 2,443,036 raw base bytes and 922,974 compressed base bytes.
The investigation below attributes the differences.

The candidate release passed automatic WebGL2 fallback and forced WebGL2
in Chromium 156 with SwiftShader. Both runs covered playback, edit rollback,
Glass, transitions, Filter, context recovery and RGBA readback. The production
quality probe completed 1, 3 and 6 players with per-canvas fences, confirmed
draws and zero pixel readbacks. The software BrowserWebGPU/WebGL2 comparison
matched four of five frames byte for byte, including the Latin/CJK sample.
The remaining frame had a maximum channel difference of 1 out of 255.

The fixed candidate build can be verified with
`--current --commit "$(git rev-parse d2d35423)"` from that commit. The later
acceptance-only Rust test and example fixes change the source fingerprint for
a fresh build. Both host builds reported missing exact upstream license text
for `fax`, `fax_derive`, `glidesort` and `rust-std` in their license inventories.

### Functional parity follow-up

The effects catalog and recipes exposed a change in the coordinate system for
bounded effects. Both renderers now use the same padded dispatch extent for
`Filter` and `Glass`. `Filter` keeps its fractional placement, and the backdrop
seed clears padding before reuse. The native regression compares bounded
`Grain` and `Vignette` on both renderers at two playhead positions.

On SwiftShader, all 30 frame and offscreen tests passed. Three additional
effect tests validated and executed the 14 built-in fragment pipelines and
the native `Pixelate` compute shader. The CLI also loaded the demo as a native
shared library and rendered its scene on both backends. Its mean per-pixel
maximum RGB-channel difference was 0.008 on the 0–255 scale. PNG and WebP
exports of the same effects catalog frame decoded to identical pixels.

The native catalog's mean per-pixel maximum RGB-channel difference dropped
from 3.32 to 0.04 on the 0–255 scale. The same measurement on the effects
recipes dropped from 7.56 to 0.02. The software run also sampled the shape
gallery with raster and SVG assets, masks, camera motion, layout and captions,
cross-file compositions, transitions and a matrix shear. Fifteen of the 16
native frame pairs had a mean difference of 0.08 or less.

Chromium with SwiftShader rendered 17 frames on each of the forced candidate
backends, including a frame with Latin and CJK text, emoji and Typst. Fifteen
frame pairs had identical RGBA bytes. The effects catalog and Glass transition
had a maximum difference of one channel value. The packaged SDK also passed
the automatic fallback and forced WebGL2 presentation smokes, including edit
rollback, context recovery and zero presentation readbacks. The cold PNG
encoding proof and SDK integrity checks passed.

The tested build ID is
`95538f7690311e205e4dcb39d35a5d1c96b391cd3bf79d4bcb8962b4069c575a`.
The SDK was built from the working tree before the code commit and passed
`verify --release --current` after the commit. Rebuild from the branch checkout
for a clean source-commit record on the hardware run. The base Wasm gzip was
2,434,237 bytes and the complete SDK was 40,262,340 bytes.

Include `32_light_vocabulary.amx` in the hardware visual comparison. The
software comparison shows a visible difference at the sweep gradient's seam
and a smaller color difference in its linear gradient. Save the two hardware
frames for review along with the effect catalog and recipes.

### Release base Wasm investigation

Use source `1fcd3725` for the two new builds. They use the same Vello GPU frame
renderer and document capabilities. Both select a concrete renderer at compile
time, which keeps the legacy Vello Compute render, filter and blit code out of
the Vello GPU frame path. `wgpu/webgl` is the additional backend feature in
the dual build. The WebGPU-only build is a diagnostic control.

```sh
ANIMATIX_VELLO_GPU=1 nix develop .#web-build -c bash web/tools/build/build-web.sh
bash tools/quality/size-report.sh
dual_sdk="$(cd web/runtime && pwd -P)"

ANIMATIX_VELLO_GPU=webgpu nix develop .#web-build -c bash web/tools/build/build-web.sh
bash tools/quality/size-report.sh
webgpu_sdk="$(cd web/runtime && pwd -P)"
```

The cloud comparison used native Binaryen 130 with the release `-Oz
--strip-debug` flags and Zopfli `--i100`. Reoptimizing the previous dual
build's original linked artifact with the same native tool gave 7,839,124
raw bytes and 2,823,216 gzip bytes. The previous archived package used
the Node Binaryen executable at the same version.

| Build | Base bytes | Base gzip bytes |
| --- | ---: | ---: |
| Current budgets | 5,400,000 | 1,900,000 |
| Previous dual candidate, same native optimizer | 7,839,124 | 2,823,216 |
| Dual candidate with compile-time renderer selection | 6,434,514 | 2,433,979 |
| Dual candidate with an additional `-Oz --converge` pass | 6,430,085 | 2,427,945 |
| WebGPU-only control with compile-time renderer selection | 4,742,500 | 1,776,169 |

Excluding the legacy frame path saved 1,404,610 raw bytes and 389,237 gzip
bytes. The dual candidate remains 1,034,514 raw bytes and 533,979 gzip bytes
over budget. The repeat optimizer pass saved 4,429 raw bytes and 6,034 gzip
bytes. The complete dual SDK is 40,260,835 bytes against the 42,000,000-byte
budget. It passed integrity, cold PNG encoding, WebGL2 playback, forced and
automatic selection, context recovery, and both-backend software readback
regressions. Its build ID is
`9092b7d12687584eed7ab86cd1567cca4fcf81461e94993434058751fcd41db3`.

The WebGPU-only control passed all SDK size limits. Its complete SDK is
37,904,564 bytes and its build ID is
`00bb65612a22e3bf8d8179d56c388944f464a8b2d0963f0d70ab7f45ee95730b`.
The release control passed cold PNG encoding and BrowserWebGPU software
readback of the inline Filter and Glass transition fixtures.
Enabling the wgpu WebGL2 path on the same source adds 1,692,014 raw base bytes
and 657,810 gzip bytes. The Wasm dependency graph contains `wgpu-core` in the
dual build and omits it in the control. These are measurements of the entire
feature delta, including the GLES backend, validation and shader translation.

Use the same-source 1,774,873-byte WebGPU-only build as the cold-start
reference. For any revised packaging, measure successful WebGPU, an
absent API and rejected WebGPU initialization. The basic split fetched
both base modules after rejection. Recheck first-frame time, frame
completion p95 and device-loss recovery on hardware before switching the
Web default.
