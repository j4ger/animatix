# Animatix Web Demo

The Animatix animation engine running in the browser: a `.amx` editor with
live diagnostics and a WebGPU player, served as a fully static site — no
backend, no runtime dependencies beyond the browser.

## Modes

- **Edit** (`/?mode=edit`) — CodeMirror source editor with syntax
  highlighting, diagnostics (errors/warnings from the real compiler
  pipeline), a transport bar (play/pause, scrub, loop, fps), and an example
  gallery. Edits rebuild the scene after a short debounce; the preview keeps
  rendering the last working build while the current source has errors.
- **Play** (`/?mode=play&example=epicycles`) — the embed-style player: full
  viewport canvas, auto-hiding controls, autoplay + loop. This is what a
  published `.amx` embed would look like.

## Run it

```bash
scripts/build-web.sh          # cargo build (wasm) + wasm-bindgen + copy examples
python3 -m http.server 8123 -d web   # or any static file server
# open http://127.0.0.1:8123/
```

The site is plain static files; deploy `web/` to any static host after
running the build script. First load downloads ~12 MB (gzip) of wasm — the
whole engine, including the Typst text layout stack, is embedded.

Requirements: a WebGPU browser (Chrome/Edge 113+, Firefox 141+, Safari 26+).
The page feature-detects and shows guidance otherwise.

## Architecture

```
crates/animatix-web          wasm32 cdylib: build pipeline + AmxPlayer (this demo's engine)
web/index.html, css/, js/    static shell: editor UI, player UI, diagnostics panel
web/vendor/cm.js             bundled CodeMirror 6 (built from web/tools, committed)
web/pkg/                     build output (gitignored): wasm, glue, examples
scripts/build-web.sh         build + glue generation + example copy
```

Layering mirrors the desktop app:

- **Parse/typecheck/expand** — `animatix-syntax`'s module system in
  `SourcesOnly` mode. The edited document and the shared `examples/lib/*.amx`
  library (embedded at compile time) live in the in-memory source map; no
  disk access exists on the platform.
- **Build** — the engine's font-context-aware entry points build a `Timeline`
  or multi-scene `Composition` exactly like the GUI does (`Draft` quality).
  Text uses the bundled Open Sans/Fira Math faces; no system fonts exist on
  the platform, so `font_family` names that the bundles don't cover fall back
  to Open Sans.
- **Render** — per frame: `timeline.evaluate_with_debug` produces a
  [`vello::Scene`] (with the GPU `GpuFilterBackend`, same as the export
  path). Vello renders through a compute pipeline that needs
  `STORAGE_BINDING` on its target — browser canvas contexts don't reliably
  expose that — so the scene is rendered into an offscreen texture at scene
  resolution and `RendererCore::blit_texture` composites it onto the canvas
  surface (the same shape as the GUI's `PreviewSurface`).

### Deliberate v1 limitations

- Multi-scene transitions **cut** instead of blending (the desktop
  `TransitionCompositor` path is not wired yet).
- Audio tracks are not played (no Web Audio wiring yet).
- Assets (images/SVG) referenced by scenes are not fetched; scenes using them
  report load warnings.
- Native plugins don't exist on this platform; `libloading`-based extensions
  are desktop-only.

## Diagnostics & probes

The shell and the wasm driver expose a few switches used during development
(and handy for bug reports):

- `?probe=1` — render 30 frames, then sample the canvas pixels and publish
  stats on `window.__probe_state` (non-black pixel ratio, fps, diagnostics).
- `?readback=1` (with `probe=1`) — render the current frame into an offscreen
  texture and read it back through a GPU buffer; publishes the average color
  and distinct-color count on `window.__probe_state`, independent of the
  compositor.
- `debug_fill(r,g,b)` / `debug_readback(t, cb)` on `AmxPlayer` — the wasm
  methods behind the modes above.
- `m.build_id()` on the engine module — identifies the running wasm build.

## Rebuilding the editor bundle

The CodeMirror bundle is committed (`web/vendor/cm.js`). To regenerate after
changing the dependency set:

```bash
cd web/tools && npm install && npm run build
```

## Testing

- `cargo test -p animatix-web` — the parse→build pipeline (including example
  sources embedded via `include_str!`) runs natively in the normal test suite.
- Manual smoke: `?probe=1` renders 30 frames, samples the canvas pixels, and
  publishes stats on `window.__probe_state` (used by the headless-Chromium
  check during development).
