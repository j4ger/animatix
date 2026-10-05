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
web/site.css                 shared dark "instrument" stylesheet for the whole site
web/site-chrome.js           shared nav + footer + timeline chrome (scroll playhead,
                             keyframe stamps, hover-play, theater), injected per page
web/index.html               project homepage — hero scene, features, demo entry points
web/tour/                    "The language" — an eleven-section tour with 10 live
                             scenes and 1 still,
                             every live figure editable in place (tour/editor.js);
                             tour/lib/components.amx is the shared component library,
                             tour/stills/textmath.png the one non-live plate
web/gallery.html             scene gallery — the seven transformer scenes as embeds
web/recipes/                 the recipes gallery — eight single-move scenes (light, dash,
                             ticker, camera, word reveal, sparks, bloom, bar race) as live
                             embeds, each with its .amx one click away; recipes/scenes/
                             holds them
web/demos/                   demo hub (course-style cards + posters) and the demos themselves:
                             demos/transformer/  the transformer walkthrough (7 scenes)
                             demos/epicycles/    Fourier epicycles walkthrough (5 scenes)
                             demos/sorting/      insertion-sort theatre (3 scenes)
                             demos/gradient/     gradient descent walkthrough (6 scenes)
                             demos/matrix/       linear-transformations walkthrough (3 scenes)
                             demos/hash/         hash-table walkthrough (3 scenes)
                             demos/lib/          shared .amx library the demo scenes import
                             demos/posters/      1280x720 stills, currently unreferenced:
                             the hub cards play live `amx-player[data-hoverplay]` embeds instead
                             demos/multi-probe.html, demos/perf-probe.html, demos/svg-probe/
                                                 QA harnesses (multi-instance, frame cost, SVG/profile)
web/demos/transformer/       "The Transformer Architecture, Animated" — seven scenes + article page
web/scenes/hero.amx          the homepage hero scene
web/pkg/, web/pkg-slim/      build output (gitignored)
scripts/build-web.sh         wasm build + wasm-bindgen + wasm-opt + brotli
scripts/serve-web.py         local static server with brotli negotiation
.github/workflows/pages.yml  deploys web/ + both engine builds to GitHub Pages
```

### Live figure editors

The tour's and the demo walkthroughs' figures carry `data-editable`;
`tour/editor.js` swaps each into a two-column view — animation beside a
textarea — where **Apply** (or Ctrl/Cmd+Enter) rebuilds the scene in place.
The element-level
`applySource(text)` on the embed component loads the new source through the
already-running engine (fonts, fetched imports and assets stay registered),
resets the clock and landmarks, and returns the build's diagnostics — the
same set the LSP surfaces — for the editor panel. A failed build throws with
`.diagnostics` attached and the previous scene keeps playing, so a typo never
blanks the figure. Reset restores the original source; edits live only on
the page.

## Run it

The demo pages serve the playback-only profile, so build **both** or just slim:

```bash
scripts/build-web.sh --slim          # playback-only (~5.5 MB raw / ~1.7 MB brotli)
scripts/build-web.sh                 # full profile  (~29.1 MB raw / ~8.5 MB brotli)
python3 scripts/serve-web.py 8124    # serves web/ with application/wasm + .br
# open http://127.0.0.1:8124/
```

**The build needs a toolchain that carries the `wasm32-unknown-unknown` std,
which the repo's default dev shell does not.** Run it in its own shell:

```bash
nix develop .#web-build          # rust with the wasm32 target + binaryen + brotli
scripts/build-web.sh --slim
```

`#default` is a GUI/dev shell: its `rust-bin` toolchain has no wasm32 std, so
the build fails inside it with `can't find crate for 'std'` in every
dependency and a misleading "the target may not be installed" — the target may
well be installed under `~/.rustup`, but that toolchain cannot see it.
`build-web.sh` now checks this up front and names the right shell.

**Enter `.#web-build` from a plain shell, not from inside `.#default`.**
Dev shells prepend to `PATH` rather than replacing it, so nesting them keeps
the outer `rustc` first — the shell *looks* like it loaded (wasm-opt and
brotli resolve fine) while the build still fails. Confirm before building:

```bash
ls "$(rustc --print sysroot)/lib/rustlib/"   # must list wasm32-unknown-unknown
```

The script also wants `wasm-bindgen-cli` at **exactly** the version
`crates/animatix-web/Cargo.toml` pins (`=0.2.128` as of writing;
`wasm-bindgen-futures` releases in lockstep, so a mismatch is a hard failure
at glue generation, several build minutes later). nixpkgs ships 0.2.114, which
is why the CLI is not in the flake:

```bash
cargo install --version 0.2.128 --locked wasm-bindgen-cli
```

Nix users have a lighter option for *serving* — a single static-web-server
binary, no Python (`nix run .#serve`, port/root overridable via `SERVE_PORT` /
`SERVE_ROOT`): it sends `application/wasm` for the engine, compresses
text responses with brotli on demand (the same bytes the prebuilt `.br`
twins hold), and answers with ETags so scene edits show on reload.

Release builds go through the `wasm-release` profile (root `Cargo.toml`: fat
LTO, one codegen unit, `panic = "abort"`) with `+simd128` enabled, kept separate
from the `release` profile so tuning it cannot move a native benchmark baseline.
Measured effect: the module is ~11% smaller raw and ~5% smaller brotli; frame
time is unchanged (the browser frame is GPU-bound, and an A/B of the two builds
could not separate them).

The seven demo scenes use plain text only, so they run on the slim engine —
~1.7 MB over the wire instead of ~8.5 MB. The transformer and gallery pages
still point `data-runtime-base` at `pkg-slim` (the legacy exact-directory
form) and the other demo pages rely on the default; new pages don't need the
attribute at all — an embed without `profile` uses slim. The
full profile is for scenes that need Typst markup, equations or image/SVG
assets: set `profile="full"` on those elements. If the configured profile is
missing the component falls back to the other one rather than showing an empty
figure, so a single-profile build still works.

Deploying is the same story: run the build script, copy `web/` (plus the
`pkg*` output) to any static host. Requirements: a WebGPU browser (Chrome/Edge
113+, Firefox 141+, Safari 26+); embeds show guidance when it's missing.

### GitHub Pages

`.github/workflows/pages.yml` deploys the site on every push to `main` that
touches `web/`, the wasm crate, or the build script: it builds **both** engine
profiles, assembles `web/` + `pkg-slim/` + `pkg/` into the artifact (dropping
the `.br` twins — Pages does not negotiate brotli, so the raw wasm is what
gets served; slim is ~5.5 MB, which is why the site's live figures all play on
the slim profile), and deploys via `actions/deploy-pages`. The wasm-bindgen-cli
version CI installs is read from the pin in `crates/animatix-web/Cargo.toml`,
so bumping that pin is the only version bump needed. Pages itself must be
enabled once in the repo settings (Source: "GitHub Actions"); the site lives
at `https://<owner>.github.io/animatix/` and every page uses relative paths,
so the subpath just works.

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
- **Attributes**: `src` (required), `autoplay`, `loop`, `controls` (control
  bar below the picture — play/pause, landmark scrubber, time, speed),
  `hold` (seconds, default 0.7), `title` (a11y label, shown while loading),
  `aspect` (`16:9`/`4:3`/`1:1`/`9:16`, auto-detected from the scene
  afterwards), `profile` (below), `quality` (below), `sealed` and `fit`
  (page-driven mode, below).
- **Controls UX** — the strip lives *below* the canvas, full-height and
  always visible: it never covers the picture, and it doubles as the
  timeline inspector. The engine reports the author's `#2s` keyframe
  declarations (and, in compositions, scene starts and transition windows);
  the strip draws them — ticks for keyframes, diamonds for scene starts,
  hatched spans for transitions — so the bar *is* the scene's structure.
  Mouse: hovering the strip freezes the clock and peeks the frame under the
  pointer (snapping to landmarks within 0.2 s); leaving resumes, unless a
  click latched the pause. Clicking the strip sets the position and toggles
  the latch; clicking the *canvas* pauses latched — resume only via a strip
  click (the inspection model the user asked for). Touch: tap toggles,
  drag scrubs. While pressed (or peeking) the clock is frozen — the
  inspected frame is what shows, and playback continues from there on
  release. Arrow keys step between landmarks, Home restarts, Space/K
  toggles. The speed chip cycles 1× → 1.5× → 2× → 0.5× (dt scaling keeps
  holds and dissolves proportional).
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
  production matches what a desktop export renders — the visible difference is
  plot-family sampling (a draft build samples plot curves with 4× the
  tolerance). Changing the attribute rebuilds the scene. Not to be confused with
  *raster* scale, which the element also chooses for itself (below).
- **Raster scale** — internal, no attribute. A browser frame costs a full-target
  vello pass (measured: an empty 1280×720 scene costs what a full one does), so
  the element sizes both ends of its frame to what it actually shows: the canvas
  backing store becomes `displayed CSS px × devicePixelRatio` (capped at the
  scene's own resolution, past which there is no more detail to render) and the
  offscreen raster follows it, which makes the final blit 1:1 and leaves the
  compositor no reason to resample. Layout is unaffected — the timeline still
  evaluates against the scene's `SceneDimensions`. When the page cannot hold the
  frame anyway, the shared rAF loop steps every playing embed down one quality
  notch (×0.85 → ×0.5) on a measured slow-tick signal and steps back up after a
  comfortable stretch. `player.set_render_scale(s)` / `render_scale()` set and
  read the raster scale directly; `docs/performance_evaluation.md` §3.7 has the
  numbers, including how this compares with the native renderer.
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

A `play()` issued from the poster/finished frame restarts at 0 rather than
continuing the rest — a loop kicked off by a card hover or a scroll gesture
always begins with the build-up, not the hold.

### Page-driven playback (`sealed`, `fit`, `amxready`)

The homepage and tour drive the players from the page's own JS instead of
handing them to the reader. Two attributes and one event back that mode:

- **`sealed`** — page-driven mode: no center play affordance, no canvas
  gestures; playback belongs to the page (`play()` / `pause()` / `seek()`).
  With `autoplay` the scene also resumes by itself on re-entering the
  viewport, since a sealed embed has no visible control to resume it with.
  Used for the full-bleed hero, the scroll-scrubbed duality figure and the
  demos-hub hover-play cards (`<amx-player data-hoverplay>` on a poster card
  starts on mouse/focus and pauses on leave — the chrome wires that).
- **`fit="cover"`** — the stage unlocks from its aspect-ratio box and the
  canvas crops to fill whatever box the page gives the element (default
  `contain` letterboxes the whole frame). Cover scenes need a safe center.
- **`amxready`** — a bubbling event fired once the scene has built; the
  element's `duration` (compiled timeline, excludes the loop rest) and
  `time` (current position, clamped to `duration`) getters read 0 until
  then, so page drivers wait for the event before computing scrub
  mappings.

### The ink design system

Every web scene draws in one palette so the plates melt into the page:
`web/demos/lib/theme.amx` defines the `ink` `Colorscheme` (extends
`editorial-dark`) plus shared spacing/type/radius tokens. A scene opts in by
importing the library and naming the scheme:

```amx
import "../lib/theme.amx"          // path relative to the scene
config { colorscheme: "ink", resolution: (1280, 720), duration: 7 }
```

`ink.scene.background` equals the site stylesheet's `--bg` (`#0b0e14`)
exactly, `text.primary` equals `--ink` (bone `#ece7db`), and the accent
hues are disciplined: amber `#f5b942` leads, coral `#ff8666` is a one-beat
drama voice, sage `#8fc7a3` and steel `#8ab4f8` only carry a diagram's
genuine semantic channels. New scenes should follow `web/tour/scenes/` and
`web/demos/transformer/scenes/` as the polished references.

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
  (U+2096), `→` (U+2192 in the slim wasm bundle) among them — render as tofu,
  but no longer silently: the build now warns `missing-glyph` naming each
  uncovered character with its codepoint. Draw dots instead, and write
  formulas in ASCII (`d -> 4d -> d`).
- **Authored opacity defeats entrance gating.** An actor with an explicit
  non-zero `opacity` is visible from frame 0 no matter which entrance follows —
  wipe/draw gate only the fill/stroke channels, and before a track's first
  keyframe it reads its default (1.0). A dimmed actor that appears late is
  authored `opacity: 0.0` and settles via a keyframe assignment
  (`rail.opacity = 0.40 [560ms]`); a full-opacity actor takes a plain entrance
  (assignments never lift the hidden-by-default seed). Never author `opacity:
  0` on a component instance either — fade-in's authored-zero branch sets only
  the instance track and never lifts the child seeds, so the component fades
  in as nothing.
- **Don't fade in a container and its child curve.** A `fade-in` on a
  container lifts the whole hidden-by-default subtree; a child's own entrance
  that starts mid-lift then settles on the *interpolated* opacity it observes
  at its start (measured: a `PlotCurve` inside a `Graph` faded this way
  rendered at ~17% — near-invisible). Fade the container, or fade the child
  after the lift completes, never both at once.

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
- No general in-browser editor: `applySource` backs the tour's and the demos'
  own figures (`tour/editor.js`), but authoring lives in the desktop app.

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
from **29.1 MB to 5.5 MB raw / 8.5 MB to 1.7 MB brotli** (the build script
prints the sizes it actually produced; the numbers here move whenever the
player gains features):

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

- `web/demos/perf-probe.html` — the frame-cost harness
  (`?scene=&players=&frames=&scales=&reps=&base=`): loads a scene, sweeps render
  scales, prints a table and publishes `window.__perf`. It drives frames by hand
  and drains the GPU queue (`debug_gpu_drain`) rather than timing the page's rAF
  loop, because a backgrounded/headless tab delivers no rAF callbacks at all and
  `queue.submit` returns several milliseconds before the GPU has drawn anything.
  `reps` repeats each sample and keeps the fastest (a browser frame timed from a
  script is hostage to whatever else the machine is doing); `base` points the page
  at a second engine build directory so two builds can be A/B'd — that is how the
  wasm build profile was measured, and it is why the engine build lives in its own
  directory rather than being swapped in place. The findings and the method are in
  `docs/performance_evaluation.md` §3.7.
- `web/demos/multi-probe.html` — four embeds on one page; publishes
  `{players, ready, playing, wasmFetches}` on `window.__probe_state` and, with
  `?readback=1`, a GPU-buffer readback (average color / distinct colors) of
  the first player — a compositor-independent pixel check.
- `web/demos/svg-probe/` — `Svg`/`Image` media probes: one page per scenario
  (`index.html` full scene, `variants.html` rect-vs-circle content bisection)
  with the same `__probe_state` readback hook.
- `debug_fill(r,g,b)` / `debug_readback(t, cb)` / `debug_svg_stats(t_ms)` on
  `AmxPlayer` — the wasm methods behind the hooks above.
- `debug_readback_rgba(t, cb)` on `AmxPlayer` — the same offscreen render as
  `debug_readback`, but `cb` receives `{ width, height, bytes }` (a `Uint8Array`
  of tight RGBA rows) instead of a statistics string. A headless browser
  screenshot cannot see a WebGPU canvas — it captures the player skeleton and
  nothing else — so this is the only way to get a real picture of what the page
  rendered. In the browser:

  ```js
  const c = new OffscreenCanvas(v.width, v.height);
  c.getContext("2d").putImageData(new ImageData(new Uint8ClampedArray(v.bytes), v.width, v.height), 0, 0);
  const png = await (await c.convertToBlob({ type: "image/png" })).arrayBuffer();
  ```

  Each call renders a whole frame at scene resolution and waits on the browser
  to poll the map, so it takes seconds — it is a probe, not a preview path. `debug_svg_stats`
  reports the SVG pipeline stage by stage (asset-cache path counts, per-track
  static/evaluated path counts and opacity at `t_ms`, vello draw/path totals);
  sample at or after the actor's entrance, since a hidden-by-default actor
  reads as empty before its reveal.
- `m.build_id()` on the engine module — identifies the running wasm build.
- `debug_bench_frames(n, dt)` / `debug_bench_warmup(n)` on `AmxPlayer` and the
  free `debug_gpu_drain()` — the frame-cost API: n timed `render_frame` calls,
  then one GPU drain, then wall time ÷ n. `debug_gpu_drain` resolves through
  `Queue::on_submitted_work_done`, so it waits for submitted work without
  copying pixels the way `debug_readback` does. `set_render_scale(s)` /
  `render_scale()` / `raster_width()` / `raster_height()` are the raster scale
  the numbers are taken at.

> Two probe traps have burned this pipeline before (see `docs/history.md`,
> "The wasm-SVG item that never was"): a scene screenshot catches whatever
> loop moment is on screen, so pin frames with `debug_readback(t)`; and
> always compute a backdrop-only baseline before calling an average "empty".

## Testing

- `cargo test -p animatix-web` — the parse→build pipeline (repo examples and
  all seven transformer scenes, embedded via `include_str!`) runs natively in
  the normal test suite. The scenes import the shared
  `examples/lib/components.amx` (a `LabeledBox` chip component); like every
  `examples/lib/*.amx`, it is **embedded into the wasm at build time** —
  editing it requires a `scripts/build-web.sh` rerun, and until then the
  browser shows `Unknown actor type …` while every native test stays green.
  The CLI resolves the same import through the
  `web/demos/transformer/lib` symlink.
