# Vello GPU release acceptance handoff

Use `experiment/vello-gpu-migration` for the candidate and `204b668a` for the
Compute baseline. The [migration archive](history/2026-11-19-vello-gpu-migration-baseline.md)
records the development SDK, native offscreen, shader and software-browser
results. The release and hardware results below govern the default switch.

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

## Phones and tablets

Use the oldest supported iOS Safari combination and a current one. On Android,
cover Adreno and Mali with current Chrome and include a phone whose browser
has WebGPU disabled. Record the model, GPU, OS, browser, SDK build ID, and
`isSecureContext`. Use trusted HTTPS when the phone connects to another host.
For Android USB testing, `adb reverse tcp:8124 tcp:8124` also lets Chrome use
`http://localhost:8124`.

- Open `/dev/vello-gpu-candidate.html?backend=auto` and record the selected
  backend from the element's `backend` property. Repeat with `backend=webgl2`
  and, where available, `backend=webgpu`. For the automatic fallback check,
  record how WebGPU was disabled and verify that the actual choice is WebGL2.
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
  regression limit is 10%. The proposed low-end WebGL2 target is sustained
  30 fps for one player. Record 3/6-player cadence, memory and dropped frames.

On the candidate page, pause and seek to 1 s. Use the phone's remote inspector
to lose the context, then check the new canvas, playhead and visible frame.

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

Run the matrix on Linux Vulkan, macOS Metal and Windows DX12. Use the
platform's pinned native release toolchain and record the actual adapter
reported by the application. On Linux the default Nix shell provides FFmpeg
8 and `bindgenHook`. On macOS and Windows, follow the native package build
instructions and use the matching FFmpeg headers and libraries.

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
