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
web/demo.css                 shared stylesheet for both pages
web/index.html               player gallery — all six scenes as embeds
web/demos/transformer/       "The Transformer Architecture, Animated" — six scenes + article page
web/demos/multi-probe.html   QA harness: four embeds on one page (shared engine, readback check)
web/pkg/, web/pkg-slim/      build output (gitignored)
scripts/build-web.sh         wasm build + wasm-bindgen + wasm-opt + brotli
scripts/serve-web.py         local static server with brotli negotiation
```

## Run it

The demo pages serve the playback-only profile, so build **both** or just slim:

```bash
scripts/build-web.sh --slim          # playback-only (~4.7 MB raw / 1.1 MB brotli)
scripts/build-web.sh                 # full profile  (~29.8 MB raw / 7.8 MB brotli)
python3 scripts/serve-web.py 8124    # serves web/ with application/wasm + .br
# open http://127.0.0.1:8124/
```

The six demo scenes use plain text only, so `web/index.html` and the transformer
walkthrough point `data-runtime-base` at `pkg-slim` — 1.1 MB over the wire
instead of 7.8 MB. The full profile is for scenes that need Typst markup,
equations or image/SVG assets. If the configured profile is missing the
component falls back to `pkg` beside itself rather than showing an empty figure,
so a full-only build still works.

Deploying is the same story: run the build script, copy `web/` (plus the
`pkg*` output) to any static host. Requirements: a WebGPU browser (Chrome/Edge
113+, Firefox 141+, Safari 26+); embeds show guidance when it's missing.

## Embedding scenes in any page (`<amx-player>`)

```html
<script type="module" src="https://your-host/amx-player.js"
        data-runtime-base="./pkg-slim"></script>

<amx-player src="./figures/attention.amx" autoplay loop controls
            title="Scaled dot-product attention" aspect="16:9" hold="1.5"></amx-player>
```

Behavior:

- **Lazy** — nothing is fetched until the element nears the viewport
  (IntersectionObserver, 200px margin); the whole page shares one engine
  download and one WebGPU device, no matter how many embeds.
- **No build-time poster** — the skeleton (aspect-ratio placeholder with a
  shimmer) is replaced by the scene's *finished* frame, not its first one.
  These scenes build up from an empty stage, so frame 0 is a blank box: a
  reader who never presses play, or who asked for reduced motion, would have
  nothing to look at.
- **Autoplay** — plays on visibility unless `prefers-reduced-motion` or
  `navigator.connection.saveData` says otherwise; offscreen instances pause
  automatically; without `autoplay` (or in those quiet modes) the embed stops
  on that finished frame with a play button.
- **Attributes**: `src` (required), `autoplay`, `loop`, `controls` (hover
  play/scrub bar), `hold` (seconds, default 0.7), `title` (a11y label, shown
  while loading), `aspect` (`16:9`/`4:3`/`1:1`/`9:16`, auto-detected from the
  scene afterwards).
- **`data-runtime-base`** goes on the `<script>` tag, not the element: it names
  the directory holding `animatix_web.js` (+ its wasm), absolute or relative to
  the page. The engine is a page-level singleton, so it is a per-page choice.
  Default: the `pkg` directory beside this component's parent.

### Looping

A scene's timeline ends at whichever comes first: its last keyframe, or a
duration declared with `config { duration: N }`. A declared duration *overrides*
the inferred length rather than extending it, so the scenes here declare one
(`duration: 6.0` and friends) to buy themselves a trailing rest — without it the
composition reaches full density on the final frame, and a naive wrap cuts
straight back to an empty stage. Measured on the pre-redesign demos, the wrap
dropped the frame from 89 to 13 distinct colours (positional: 159 to 1).

So a looping embed plays `duration + hold`: the finished frame rests at full
opacity for `hold` seconds, then dissolves out over the last ~0.28 s and the
build-up fades back in. Set `hold="0"` for an unadorned loop. `hold` is the
embed's own knob and applies to any scene, declared duration or not — useful for
third-party scenes that cannot be edited.

### Authoring notes that bit these scenes

- **An entrance action settles on the target's authored `opacity`**, except for
  an authored `opacity: 0`, which every entrance action treats as a "start
  hidden" seed and lifts to 1.0. This used to be inconsistent: `fade-in` forced
  every target to 1.0 (so the old `halo` at 0.22, `expand`/`project` at
  0.75/0.4 and `layerN` at 0.6 were silently flattened) while `wipe-in`,
  `draw-in` and `reveal-in` never touched `opacity` at all, leaving an authored
  `opacity: 0` invisible for the whole timeline. Both halves are fixed; the rule
  is in `docs/spec.md`.
- **Paint order is declaration order** — a later actor paints over an earlier
  one, matching `root_nodes`, which is built in declaration order. Verified
  numerically rather than by eye: the same panel and crossing arrow, with the
  arrow declared first versus last, differ by 6/255 in the frame average, and the
  animation used (`draw-in`/`fade-in`/none) makes no difference.
  *How this was first got wrong:* the first probe declared its actors without an
  explicit `opacity`, which makes them hidden-by-default, so the crossing actor
  never rendered and every ordering measured identically — see the next note.
- **An actor declared before the first keyframe is hidden until an entrance
  action reveals it** (there is a `never-revealed` warning for this, and an
  explicit `opacity` opts out of the seed). This is worth stating twice: it
  invalidates any probe that forgets it, because a hidden actor silently
  contributes nothing to the frame.
- **A `Path` fills with `color` by default** and only strokes when given a
  `stroke` plus `fill_opacity: 0.0`. Without that an open arc renders as a
  filled lens with a white outline — SVG's fill rule, and the white outline is
  because an unset `stroke` on a shape falls back to the colorscheme's
  `stroke.default`.
- `Text` uses one face, so `font_weight: "bold"` is inert on the plain path, and
  the fast-path face has no glyph for `⋮` (U+22EE), `ᵀ` (U+1D40) or `ₖ`
  (U+2096). They render as tofu with no diagnostic on any path. Draw dots
  instead, and write formulas in ASCII.

Hosting requirements for the runtime host: serve `.wasm` as
`application/wasm`, and prefer precompressed `.br` twins (the build script
emits them; any CDN negotiates brotli transparently). The `.amx` files may
live anywhere CORS permits. `serve-web.py` sends `no-cache` for pages,
stylesheets and `.amx` sources (so editing a scene and reloading actually shows
the edit) and a short `max-age` for the generated engine artifacts. Rebuild the
component after edits:

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

### Capabilities and limitations

- **Multi-scene transitions blend** — the GPU compositor the desktop uses runs
  in the player, so `play` edges render their transition instead of cutting.
- **Assets fetch** — `Image`/`Svg` actors with a literal `url` resolve relative
  to the scene file (fetched alongside it; absolute URLs work too). Dynamic
  `url = expr` assignments are not listed.
- **Runtime fonts** — set `data-fonts` on the embed to a space-separated list
  of TTF/OTF URLs; they are registered before the scene compiles, so
  `font_family` can name them. WOFF2 is not decodable. Without a font covering
  the script, non-Latin text renders empty (the sandbox cannot see system
  fonts, and only Open Sans + Fira Math are bundled).
- **Audio tracks are not played** (no Web Audio wiring yet).
- **Export** stays desktop-only (video via FFmpeg; PNG/WebP via raster-encode).
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
- `web/demos/svg-probe/` — `Svg`/`Image` media probes: one page per scenario
  (`index.html` full scene, `variants.html` rect-vs-circle content bisection)
  with the same `__probe_state` readback hook.
- `debug_fill(r,g,b)` / `debug_readback(t, cb)` / `debug_svg_stats(t_ms)` on
  `AmxPlayer` — the wasm methods behind the hooks above. `debug_svg_stats`
  reports the SVG pipeline stage by stage (asset-cache path counts, per-track
  static/evaluated path counts and opacity at `t_ms`, vello draw/path totals);
  sample at or after the actor's entrance, since a hidden-by-default actor
  reads as empty before its reveal.
- `m.build_id()` on the engine module — identifies the running wasm build.

> Two probe traps have burned this pipeline before (see `docs/history.md`,
> "The wasm-SVG item that never was"): a scene screenshot catches whatever
> loop moment is on screen, so pin frames with `debug_readback(t)`; and
> always compute a backdrop-only baseline before calling an average "empty".

## Testing

- `cargo test -p animatix-web` — the parse→build pipeline (repo examples and
  all six transformer scenes, embedded via `include_str!`) runs natively in
  the normal test suite.
