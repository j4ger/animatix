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

The six demo scenes use plain text only, so they run on the slim engine —
1.1 MB over the wire instead of 7.8 MB. Their pages still point
`data-runtime-base` at `pkg-slim` (the legacy exact-directory form); new pages
don't need the attribute at all — an embed without `profile` uses slim. The
full profile is for scenes that need Typst markup, equations or image/SVG
assets: set `profile="full"` on those elements. If the configured profile is
missing the component falls back to the other one rather than showing an empty
figure, so a single-profile build still works.

Deploying is the same story: run the build script, copy `web/` (plus the
`pkg*` output) to any static host. Requirements: a WebGPU browser (Chrome/Edge
113+, Firefox 141+, Safari 26+); embeds show guidance when it's missing.

## Embedding scenes in any page (`<amx-player>`)

```html
<script type="module" src="https://your-host/amx-player.js"></script>

<!-- plain scene: the default slim engine serves it -->
<amx-player src="./figures/tokens.amx" autoplay loop controls
            title="From text to tokens" aspect="16:9" hold="1.5"></amx-player>

<!-- a figure needing rich text / image assets: full engine, export fidelity -->
<amx-player src="./figures/equation.amx" profile="full" quality="production"
            title="Attention as soft maximum"></amx-player>
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
- **Attributes**: `src` (required), `autoplay`, `loop`, `controls` (bottom
  control bar — play/pause, seek scrubber, time readout), `hold` (seconds,
  default 0.7), `title` (a11y label, shown while loading), `aspect`
  (`16:9`/`4:3`/`1:1`/`9:16`, auto-detected from the scene afterwards),
  `profile` (below), `quality` (below).
- **Controls UX** — one model for mouse and touch, following media-player
  conventions: the canvas's first tap reveals the bar, the next toggles
  playback, and the bar auto-hides after 2.5 s while playing (paused figures
  keep it up; on PC, pointer movement also reveals it). The scrubber covers
  the timeline only — a looping embed's `hold` rest appears as a dimmed
  trailing segment, so the bar explains why the figure sits on its finished
  frame before restarting. Keyboard: the scrubber is focusable (`role`
  slider), arrow keys seek ±1 s, Home restarts, Space/K toggles. Touch
  targets are ≥40 px (`44` under `pointer: coarse`).
- **`profile`** — per element, `"slim"` (default) or `"full"`: which engine
  build backs the figure. Omitted means slim, so a page of plain-text scenes
  needs nothing; a figure with Typst markup, `Math`/`Code` highlighting or
  `Image`/`Svg` assets sets `profile="full"`. Embeds sharing a profile share
  one engine download and one WebGPU device; a page mixing both profiles
  downloads each once (each wasm instance owns its context). The engine
  directories come from the loader script's `data-runtime-base` (below), and
  a profile whose directory the host did not build falls back to the other
  one instead of showing a blank figure.
- **`quality`** — per element, `"draft"` (default) / `"preview"` /
  `"production"`: the build fidelity, i.e. what `BuildQuality` bakes into the
  document at build time. Draft matches the desktop GUI's editing preview;
  production matches what a desktop export renders — the visible difference
  is plot-family sampling (a draft build samples plot curves with 4× the
  tolerance). Changing the attribute rebuilds the scene.
- **`data-runtime-base`** goes on the `<script>` tag, not the element: it
  names the directory tree holding the engine builds — absolute or relative
  to the page. A value ending in `pkg`/`pkg-slim` is the legacy exact-directory
  form (its ±`-slim` sibling completes the pair); anything else is a parent
  directory expected to contain `pkg/` and `pkg-slim/`. Default: the parent of
  the directory this component lives in.

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
- `Text` uses one face, so `font_weight: "bold"` is inert on the plain path.
  Characters no registered face covers — `⋮` (U+22EE), `ᵀ` (U+1D40), `ₖ`
  (U+2096) among them — render as tofu, but no longer silently: the build now
  warns `missing-glyph` naming each uncovered character with its codepoint
  (the probe mirrors which path shapes the text, so it also covers the Typst
  path these three take — they sit outside the fast path's Latin gate). Draw
  dots instead, and write formulas in ASCII.

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
  `SourcesOnly` mode. The played document, the shared `examples/lib/*.amx`
  library (embedded at compile time, so repo examples resolve their imports),
  and any fetched import modules live in the in-memory source map; no disk
  access exists on the platform.
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
- **Imports fetch** — a scene may `import` `.amx` files beyond the bundled
  library. The build stops on an unsupplied import and reports the resolved
  path in `missing_imports`; the embed fetches it relative to the scene
  (transitive imports close over repeated rounds) and retries. Shells driving
  the player directly do the same with `add_module(path, text)`.
- **Assets fetch** — `Image`/`Svg` actors with a literal `url` resolve relative
  to the scene file (fetched alongside it; absolute URLs work too, including
  inside imported modules — their urls resolve against the module's own
  location, but the engine keys the cache by the literal url string, so asset
  names share one flat namespace across the scene and its imports). Dynamic
  `url = expr` assignments are not listed.
- **Runtime fonts** — set `data-fonts` on the embed to a space-separated list
  of TTF/OTF URLs; they are registered before the scene compiles, so
  `font_family` can name them. WOFF2 is not decodable. Without a font covering
  the script, non-Latin text renders empty (the sandbox cannot see system
  fonts, and only Open Sans + Fira Math + a Noto Sans SC subset are bundled) —
  though the build now warns which characters are uncovered.
- **Audio tracks are not played** (no Web Audio wiring yet).
- **Export** stays desktop-only (video via FFmpeg; PNG/WebP via raster-encode).
- Native plugins don't exist on this platform; `libloading`-based extensions
  are desktop-only.
- No in-browser editing — that is the desktop app's job.

### Differences from the desktop renderer

The render stack is the desktop's — same engine crates, same vello/wgpu
`RendererCore`, same `GpuFilterBackend` per filter target (including the
zero-readback pending-composite blit), same `TransitionCompositor` for
multi-scene blends. What differs is the platform around it:

| | Desktop (CLI / GUI) | Web full | Web slim |
|---|---|---|---|
| Rich text (Typst markup, Code highlighting, Math) | ✓ | ✓ | falls back to the plain fast path |
| `Image` assets | ✓ | ✓ | hard feature-gate error |
| `Svg` actors | ✓ | ✓ | primitive unregistered: `unknown-actor-type` warning, actor skipped |
| System fonts | scanned | none (bundle + `data-fonts` only) | same |
| Extensions (`plugin-loading`) | ✓ | ✗ | ✗ |
| Audio | muxed at export | not played | not played |
| Export (video / PNG / WebP) | ✓ | ✗ | ✗ |
| Build quality | `Production` (export), `Draft` (GUI editing) | `quality` attribute: `draft` default, `preview`/`production` opt-in | same |

The pixel-level consequence of that last row: plot-family actors (`Graph`,
`PlotCurve`, `VectorField`, …) sample with 4× the tolerance under the default
`draft`, so dense curves can be slightly coarser than a desktop export renders
them — `quality="production"` closes that gap at rebuild cost. The GUI preview
is the same `Draft`, so the default shows what an author saw while editing.
Two smaller gaps: a
single-scene document using `persist` with no successor scene skips the
`PersistTargetNotCarried` warning the desktop build emits (behaviour
degrades to state resetting between loops), and `perf-tracing` is compiled
out, so stage traces exist only on native. What the web side adds and the
desktop has no equivalent of: the shared per-page `EngineContext` (one
adapter/device/vello renderer for every embed), the offscreen-target + blit
presentation (the `STORAGE_BINDING` workaround), and the `debug_fill` /
`debug_readback(t)` / `debug_svg_stats(t_ms)` / `build_id()` diagnostic
surface.

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
