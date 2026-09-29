# Animatix Web Player

The Animatix engine, running in the browser: a WebGPU player for `.amx`
scenes and a dependency-free `<amx-player>` embed component — served as a
fully static site, no backend, no runtime dependencies beyond the browser.

Scene **authoring** lives in the desktop app (`cargo run -p animatix-gui`);
the web side only plays back and embeds.

## What's here

```
crates/animatix-web          wasm32 cdylib: build pipeline + AmxPlayer (the engine)
web/embed/amx-player.js      <amx-player> web component (committed bundle; source in embed/src)
web/index.html               player landing page (three embeds)
web/demos/transformer/       "The Transformer Architecture, Animated" — six scenes + article page
web/demos/multi-probe.html   QA harness: four embeds on one page (shared engine, readback check)
web/pkg/, web/pkg-slim/      build output (gitignored)
scripts/build-web.sh         wasm build + wasm-bindgen + wasm-opt + brotli
scripts/serve-web.py         local static server with brotli negotiation
```

## Run it

```bash
scripts/build-web.sh                 # full profile  (~29.8 MB raw / 7.8 MB brotli)
scripts/build-web.sh --slim          # playback-only (~4.7 MB raw / 1.1 MB brotli)
python3 scripts/serve-web.py 8124    # serves web/ with application/wasm + .br
# open http://127.0.0.1:8124/
```

Deploying is the same story: run the build script, copy `web/` (plus the
`pkg*` output) to any static host. Requirements: a WebGPU browser (Chrome/Edge
113+, Firefox 141+, Safari 26+); embeds show guidance when it's missing.

## Embedding scenes in any page (`<amx-player>`)

```html
<script type="module" src="https://your-host/amx-player.js"></script>

<amx-player src="./figures/attention.amx" autoplay loop controls
            title="Scaled dot-product attention" aspect="16:9"></amx-player>
```

Behavior:

- **Lazy** — nothing is fetched until the element nears the viewport
  (IntersectionObserver, 200px margin); the whole page shares one engine
  download and one WebGPU device, no matter how many embeds.
- **No build-time poster** — the skeleton (aspect-ratio placeholder with a
  shimmer) is replaced by the scene's own first frame; from there the embed
  looks like an animated figure.
- **Autoplay** — plays on visibility unless `prefers-reduced-motion` or
  `navigator.connection.saveData` says otherwise; offscreen instances pause
  automatically; without `autoplay` (or in those quiet modes) the embed stops
  on its first frame with a play button.
- **Attributes**: `src` (required), `autoplay`, `loop`, `controls` (hover
  play/scrub bar), `title` (a11y label, shown while loading), `aspect`
  (`16:9`/`4:3`/`1:1`/`9:16`, auto-detected from the scene afterwards),
  `data-runtime-base` (where the engine bundle lives; defaults to next to the
  script — same-host deployment needs no configuration).

Hosting requirements for the runtime host: serve `.wasm` as
`application/wasm`, and prefer precompressed `.br` twins (the build script
emits them; any CDN negotiates brotli transparently). The `.amx` files may
live anywhere CORS permits. Rebuild the component after edits:

```bash
cd web/tools && npm install && npm run build:embed
```

## Engine layering

- **Parse/typecheck/expand** — `animatix-syntax`'s module system in
  `SourcesOnly` mode. The played document and the shared `examples/lib/*.amx`
  library (embedded at compile time, so repo examples resolve their imports)
  live in the in-memory source map; no disk access exists on the platform.
- **Build** — the engine's font-context-aware entry points build a `Timeline`
  or multi-scene `Composition` exactly like the GUI does (`Draft` quality).
- **Render** — per frame: `timeline.evaluate_with_debug` produces a
  [`vello::Scene`] (with the GPU `GpuFilterBackend`, same as the export
  path). Vello renders through a compute pipeline that needs
  `STORAGE_BINDING` on its target — browser canvas contexts don't reliably
  expose that — so the scene is rendered into an offscreen texture at scene
  resolution and `RendererCore::blit_texture` composites it onto the canvas
  surface (the same shape as the GUI's `PreviewSurface`).

### Deliberate limitations

- Multi-scene transitions **cut** instead of blending (the desktop
  `TransitionCompositor` path is not wired yet).
- Audio tracks are not played (no Web Audio wiring yet).
- Assets (images/SVG) referenced by scenes are not fetched; scenes using them
  report load warnings.
- Native plugins don't exist on this platform; `libloading`-based extensions
  are desktop-only.
- No in-browser editing — that is the desktop app's job.

## Slim playback profile

The default build carries the full feature set (Typst rich text, raster
decoding, SVG). A playback-only profile compiles those out — Text falls back
to the plain fast path (no markup/Code highlighting/Math, non-Latin scripts
need a system font), image/SVG assets report diagnostics, and the wasm drops
from **29.8 MB to 4.7 MB raw / 7.8 MB to 1.1 MB brotli**:

```bash
scripts/build-web.sh --slim     # emits web/pkg-slim/
```

(The bundled bold/italic faces and Fira Math are rich-text-only — the plain
fast path always uses a family's first face — so slim carries Open Sans
Regular alone.)

This is the profile for motion-graphics embeds that use plain text only;
explainers with equations or styled text need the full build. Feature flags
behind it: `animatix-text/rich-text`, `animatix/image-decode`,
`animatix/svg`, `animatix-render/raster-encode` (all default-on; `animatix-web`
exposes them as its own default features so `--no-default-features` selects
the slim set).

## Diagnostics & probes

- `web/demos/multi-probe.html` — four embeds on one page; publishes
  `{players, ready, playing, wasmFetches}` on `window.__probe_state` and, with
  `?readback=1`, a GPU-buffer readback (average color / distinct colors) of
  the first player — a compositor-independent pixel check.
- `debug_fill(r,g,b)` / `debug_readback(t, cb)` on `AmxPlayer` — the wasm
  methods behind the hooks above.
- `m.build_id()` on the engine module — identifies the running wasm build.

## Testing

- `cargo test -p animatix-web` — the parse→build pipeline (repo examples and
  all six transformer scenes, embedded via `include_str!`) runs natively in
  the normal test suite.
