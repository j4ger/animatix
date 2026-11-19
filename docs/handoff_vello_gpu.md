# Vello GPU release acceptance handoff

Use `experiment/vello-gpu-migration` for the candidate and `204b668a` for the
Compute baseline. The [migration archive](history/2026-11-19-vello-gpu-migration-baseline.md)
records the development SDK, native offscreen, shader and software-browser
results. The tested release candidate exceeds the base Wasm budgets. Use it
for device compatibility testing and rerun the size gate before a Web default
switch.

## Build two immutable SDKs

In separate clean checkouts, use the same pinned `.#web-build` environment.
Save each manifest, its `build.source_commit`, and the output of the size gate.
The base Wasm gzip budget is 1,900,000 bytes. Record every reported size before
running the gate so a failure on the first limit still leaves the full report.

```sh
git fetch origin main experiment/vello-gpu-migration
git switch experiment/vello-gpu-migration
baseline="$(pwd)/../animatix-vello-baseline"
git worktree add --detach "$baseline" 204b668a

(cd "$baseline" && nix develop .#web-build -c bash web/tools/build/build-web.sh)
ANIMATIX_VELLO_GPU=1 nix develop .#web-build -c bash web/tools/build/build-web.sh

# Run both commands in each checkout after its build.
node web/tools/sdk.mjs verify web/runtime --release --current --commit "$(git rev-parse HEAD)"
bash tools/quality/size-report.sh
bash tools/quality/size-report.sh --check
```

Run `node web/tools/test/web-raster-proof.mjs web/runtime` and the `text-parity`
CI gate against the release candidate. For the Chromium smoke, start
`python3 web/tools/dev/serve-web.py 8124` in the candidate checkout, then run
`node web/tools/test/vello-gpu-webgl-smoke.mjs` and repeat with
`WEBGL_CANDIDATE_MODE=webgl2`. Preserve the built SDK directories between tests.

## Linux hardware GPU acceptance

A Linux workstation with a hardware GPU is the required graphics acceptance
environment. Use it for browser WebGPU, browser WebGL2 and native Vulkan.
Record the GPU model, active driver, browser version, kernel, and SDK build ID.
Record the renderer actually selected by Chromium and the adapter used by the
native process. A software adapter is valid for regression smoke and leaves
the hardware row open.

Use the normal Chromium GPU configuration to verify that `backend=auto`
selects WebGPU first. The fallback run disables browser WebGPU while keeping
ANGLE on hardware Vulkan. Run it at the existing phone-sized viewport to
retain the mobile layout and compositor regression assertions.

```sh
# Start the candidate server on 127.0.0.1:8124 in another terminal.
vulkaninfo --summary
WEBGL_CANDIDATE_GPU=1 node web/tools/test/vello-gpu-webgl-smoke.mjs
WEBGL_CANDIDATE_GPU=1 WEBGL_CANDIDATE_MODE=webgl2 \
  node web/tools/test/vello-gpu-webgl-smoke.mjs
```

Save the renderer and SDK IDs printed by the smoke. Record the backend from
the player's `backend` property and the WebGPU adapter shown in Chromium GPU
diagnostics on the normal WebGPU run. A second Linux browser is useful for
additional compatibility evidence when available. iOS Safari, Android browsers,
macOS Metal and Windows DX12 stay in the coverage record as hardware untested.
For shipped Windows and macOS binaries, record the existing build and package
smoke results from CI. Linux acceptance supports a staged Web rollout with the
previous SDK ready for rollback. A broader hardware compatibility claim needs
evidence from those platforms.

- Open `/dev/vello-gpu-candidate.html?backend=auto` and `?backend=webgpu` on
  ordinary hardware Chromium. Confirm visible frames on both. The hardware
  smoke above covers automatic fallback and forced WebGL2.
- Exercise the smoke fixtures, images and SVG, CJK and color glyphs, the effects
  catalog, shared-object transitions, edits and scrubbing. Save representative
  frames and validate visible foreground pixels and expected color/alpha.
  Retain the active document and playhead through context loss. On a six-player
  page, confirm that losing one WebGL2 context leaves the other five usable.
  Verify zero CPU pixel readbacks during ordinary presentation.
- Use `/dev/perf-probe.html?scene=../content/scenes/first-move.amx&backend=webgl2&players=1&scales=1&frames=90&reps=3`.
  Repeat with 3 and 6 players and with the heavier Glass fixture. Save
  `window.__perf` or the rendered JSON ledger. It records actual backend,
  build ID, pixels, CPU time and queue-depth-one completion p95. WebGL2 polls
  canvas fences and includes browser scheduling overhead. Use a foreground
  60-second playback trace for actual delivered frame rate and dropped frames.
- A/B the baseline and candidate with forced WebGPU, matching scene, scale,
  viewport, refresh rate and player count. Use `base` on the probe to select
  the SDK directory per page load. Serve the two verified directories as
  `/baseline-sdk` and `/runtime` on the same test origin. Test in alternating
  order and retain the raw runs. The proposed first-frame and frame p95
  regression limit is 10%. For the representative foreground WebGL2 scene,
  check sustained 30 fps with one player on the available Linux GPU and record
  3/6-player cadence, memory and dropped frames. Retain the actual GPU in the
  report so the result describes its measured hardware class.

Use `first_ready_ms` for embed startup and `startup_queue_completed_ms` for
completion of the startup submissions. For first-frame completion latency,
use a single player with a fresh profile. The probe loads production quality
on both builds and rejects samples in which a player skipped drawing.

On the candidate page, pause and seek to 1 s. Use Chromium DevTools to repeat
the WebGL2 loss manually, then check the new canvas, playhead and visible frame.

```js
const player = document.querySelector('amx-player');
player.pause();
player.seek(1);
const canvas = player.shadowRoot.querySelector('canvas');
player.addEventListener('amxbackendrecovered', () => console.log({
  backend: player.backend,
  time: player.time,
  freshCanvas: player.shadowRoot.querySelector('canvas') !== canvas,
}), { once: true });
canvas.getContext('webgl2').getExtension('WEBGL_lose_context').loseContext();
```

## Native and video

Run the required native matrix on the same Linux Vulkan GPU and driver as the
browser acceptance. Use the pinned native release toolchain and record the
actual adapter reported by the application. The default Nix shell provides
FFmpeg 8 and `bindgenHook`.

```sh
nix develop -c python3 tools/ci/compare-snapshot-gates.py \
  --baseline 204b668a --candidate "$(git rev-parse HEAD)"
nix develop -c env ANIMATIX_REQUIRE_GPU=1 ANIMATIX_RENDER_BACKEND=vello-gpu \
  cargo test --locked -p animatix-render --features vello-gpu \
  --test offscreen_render -- --test-threads=1
nix develop -c cargo build --locked --release -p animatix-cli -p animatix-gui \
  --features video,vello-gpu
```

Repeat the offscreen run with `ANIMATIX_RENDER_BACKEND=legacy` on the same
hardware. Open the Glass fixture in the GUI with each backend and exercise
play, pause, seek, preview resizing and export. Save screenshots and the
`--perf-log` JSONL. Use the same scene for PNG and WebP exports.

```sh
out="$(mktemp -d)"
export ANIMATIX_RENDER_BACKEND=vello-gpu
target/release/animatix video examples/animation/41_speech_sync_captions.amx \
  --fps 24 --duration 2 --codec libx264 -o "$out/narrated.mp4"
ffprobe -v error -count_frames -show_streams -show_format -of json \
  "$out/narrated.mp4" > "$out/narrated.json"
```

Check 48 decoded video frames, a two-second video track and a continuous
audio track. Compare a decoded frame with the PNG at the same timestamp using
the lossy codec's tolerance. In a scratch copy of the examples, run
`tools/content/chapter-export.py` on `examples/gallery/brand_reel/main.amx`
with the video-enabled CLI. Verify chapter order and continuous audio. Run
again unchanged, then after a one-chapter edit. Check cache hits for unaffected
parts and rebuilds for the edited chapter and affected transitions. Repeat with
the legacy backend and confirm that candidate parts are not reused.

Save `tools/quality/perf-bench.sh save quick` on the baseline and run
`compare quick` on the candidate with the same `PERF_BASELINE_DIR` on an idle
machine. Keep the complete result together with the native GPU/export timing
and its adapter identity.

## Default switch and dependency cleanup

Use independent Web and native default-switch commits after their acceptance
passes. Exercise restoring the previous immutable Web SDK or native package
on a preview system. Compare first-frame failures and backend-recovery
failures during the production observation window. Roll back the affected
platform on a reproducible blank frame, state loss or a material regression.

After the default switch stabilizes, delete the old Compute code and vendor
patches and retire the old Hybrid benchmark. Verify the locked graph and
`Cargo.lock` on all targets. The local Cargo index showed that both
`typst-svg 0.15.1 → hayro ^0.7.1 → vello_cpu ^0.0.8` and
`egui 0.37.0 → epaint 0.37.0 → vello_cpu ^0.2.0` remain. Hayro 0.9.0 in that
index still uses `vello_cpu ^0.3`, outside Typst's current range. Check upstream
releases before changing either pin. The strict `wasm-no-legacy` and
`full-no-legacy` results remain required for full cleanup.

## Cloud environment gaps

The Linux container has no hardware GPU, Android or iOS device connection, or
macOS and Windows graphics stacks. Its installed FFmpeg is 7.1.5 and
`pkg-config` cannot find the FFmpeg development libraries. The pinned Nix
shell attempt timed out connecting to `cache.nixos.org`. The CPU comparison
reported that no saved `quick` baseline was available. Carry the native,
video and performance rows to the corresponding test devices and attach
their logs and artifacts before changing defaults.

## Cloud checks completed on 2026-11-19

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

The baseline passed the SDK size gate. The candidate needs to recover 2,443,036
raw base bytes and 922,974 compressed base bytes. Check which renderer code
lands in the base module, starting with the WebGL-only paths through wgpu and
Naga and any reachable Compute code. Measure the actual cold transfer for
forced WebGPU and forced WebGL2 when testing partition changes. Keep the
existing SDK limits in the comparison.

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
