# Animatix Web Player

The Animatix engine, running in the browser: a WebGPU player for `.amx`
scenes and a dependency-free `<amx-player>` embed component — served as a
fully static site, no backend, no runtime dependencies beyond the browser.

Use the desktop workbench or a text editor to author projects. The website
adds local, in-page source editing for selected figures; it is not a general
project editor and does not save those edits to disk.

## Start here

- [Run locally](#run-it) to build and preview the site.
- [Embed a scene](#embedding-scenes-in-any-page-amx-player) for attributes and APIs.
- [Capabilities](#capabilities-and-limitations) for browser/native differences.
- [House style](DESIGN.md) for new scenes and pages.
- [Diagnostics](#diagnostics--probes) for runtime investigation.

## Source map

| Location | Purpose |
|---|---|
| `crates/animatix-web/` | Wasm engine and player API |
| `web/embed/src/` | `<amx-player>` source; builds to `web/embed/amx-player.js` |
| `web/runtime/` | Generated SDK and manifest; not committed |
| `web/fonts/` | External font-pack policy and manifest |
| `web/site.css`, `web/site/`, `web/site-chrome.js` | Shared page style and navigation |
| `web/works.json` | Showcase metadata used by generated page blocks |
| `web/tour/` | Language tour and live figure editor |
| `web/demos/`, `web/recipes/`, `web/gallery.html` | Walkthroughs and example scenes |
| `web/arena/` | Side-by-side candidate evaluation |
| `web/dev/` | Developer probes, outside the showcase |

### Live figure editors


The tour's and the demo walkthroughs' figures carry `data-editable`;
`tour/editor.js` swaps each into a two-column view — animation beside a
textarea — where **Apply** (or Ctrl/Cmd+Enter) rebuilds the scene in place.
The element-level
`applySource(text)` is asynchronous: it prepares imports, assets, fonts, and
required runtime capabilities, then replaces the document on success. It
resets the clock and landmarks, and returns the build's diagnostics — the same
set the LSP surfaces — for the editor panel. A failed build throws with
`.diagnostics` attached and the previous scene keeps playing, so a typo never
blanks the figure. Reset restores the original source; edits live only on
the page.

Both `applySource(text)` and `setVars(obj)` return promises. `destroy()` explicitly
releases an element's player resources; removing or reparenting the element does
not destroy it.

Calls made during the 150 ms `setVars()` debounce share one rebuild result; each
returned promise settles with that batch. A failed or superseded source reload
rejects outstanding variable promises and restores `player.vars` to the last
committed values. Explicit destruction rejects pending work with
`error.kind === "cancel"`. A detached element can be reattached with its player
and document intact; after `destroy()`, create a new element to mount again.

Loading and attribute-driven rebuild failures emit a bubbling `amxerror` event
with `detail: { src, kind, message, error, diagnostics? }`. `kind` is one of
`resource` (source, import, font or media fetch), `capability` (WebGPU or a
required runtime capability), `version` (manifest, Wasm integrity or runtime
identity mismatch), or `build` (source analysis or document build). `error` is
an `Error` carrying the same `kind`; `diagnostics` is present when the build
produced diagnostics. Superseded or destroyed operations reject with
`error.kind === "cancel"` and do not emit `amxerror`. Explicit `applySource()`
and `setVars()` failures reject their returned promises with the same
`kind`/`diagnostics` fields instead of emitting an event for the caller's
operation.

The parameter-slider counterpart is `setVars(obj)`: build-time overrides —
the CLI's `--set NAME=VALUE` — merged into the scene's `let` defaults and
rebuilt asynchronously in place, debounced, with the playback position preserved. See
`data-vars` under "Embedding scenes in any page".

## Run it

From the repository root, enter the web build shell from a plain terminal:

```sh
nix develop .#web-build
scripts/ensure-web-pkg.sh          # build only when stale
python3 scripts/serve-web.py 8124
# Open http://127.0.0.1:8124/
```

Use `scripts/build-web.sh` for an explicit SDK rebuild. Do not nest the web shell
inside the default development shell: the outer Rust toolchain can remain first
on `PATH`. The web toolchain needs the `wasm32-unknown-unknown` target.

### Verification and inspection tools

```bash
# Inspect all <amx-player> instances on a page and save canvas frames:
python3 scripts/web-inspect.py --url http://127.0.0.1:8124/demos/transformer/ --output-dir /tmp/shots

# Extract and render keyframes from an .amx scene and generate a contact sheet:
scripts/render-keyframes.sh web/scenes/hero.amx /tmp/hero_frames
scripts/contact-sheet.sh /tmp/hero_frames /tmp/hero_sheet.png
```

For a `can't find crate for 'std'` error, check that the active toolchain includes
the Wasm target:

```sh
ls "$(rustc --print sysroot)/lib/rustlib/"
```

`wasm-bindgen-cli` must match the pin in `crates/animatix-web/Cargo.toml`.
The web shell supplies the matching CLI on x86_64 Linux. On other platforms or
outside Nix, use `scripts/ci.sh setup wasm-bindgen`, which installs a verified
release in a user-writable directory. `scripts/wasm-tools.sh` reads the pin; do
not duplicate its version in setup commands.

`scripts/ci.sh gate web-smoke` (run it in `nix develop .#web-ci`, after
`web-build`) loads the home, transformer and matrix pages in headless Chromium
and fails unless every `<amx-player>` reaches `ready` with a painted canvas. It
is the gate that notices a runtime wiring break which `web-build`/`site-scenes`
cannot see.

The web build uses the pinned `zopfli` 1.0.3 tool only to create the base
Wasm's gzip representation. Brotli and gzip capability/font assets have their
own packaging paths; this pin does not imply that every package asset is
compressed with zopfli.

Two hardware checks are intentionally manual and are not PR matrix gates:
`scripts/ci.sh gate text-visual-parity` compares browser text rendering against
the native reference, and `scripts/ci.sh gate web-performance` measures browser
runtime performance. Both require `WPR_GPU=1` and reject software adapters;
the performance run also requires an idle host. Run them in `nix develop
.#web-ci` with the final SDK build available. Their results are evidence for
the tested machine and build, not cross-platform GPU coverage.

### Web performance policy

The Web performance harness keeps cold and warm paired samples, relative
new/old cost ratios, paired absolute cost deltas, and both 95% confidence
intervals in its JSON report. Its three primary user-impact metrics are cold
player-ready startup, warm player-ready startup, and queue-depth-one p95
completion latency. Warm startup creates a player in a reused context; it does
not measure `applySource()` cached-source rebuilding.

Frame-cost comparisons pin the engine raster to the requested scale and disable
the embed's adaptive quality controller in the benchmark page. The report
includes the startup raster before pinning and verifies the fixed scale and
raster dimensions before and after each measured phase. This isolates renderer
cost at equal resolution; it does not measure adaptive playback behavior. The
legacy frame hash is a measurement reference, not a visual golden. Review the
current renderer's captured frames for visual quality.

The 5% relative margin remains a signal. A primary metric blocks only when the
lower bound of both its relative-ratio interval and absolute-delta interval
exceeds the corresponding limits. A result is “no material regression” when
the upper bound of either interval stays within its limit. If uncertainty
crosses both limits, or required pairs are missing or invalid, the result stays
inconclusive and the gate exits with status 2. The absolute floors are
provisional engineering materiality limits, not validated product SLOs:
at least three valid pairs are required for each metric interval.

| Primary metric | Provisional absolute floor | Basis |
| --- | ---: | --- |
| Cold player-ready startup | 50 ms | About three 60 Hz frame intervals |
| Warm player-ready startup | 16.7 ms | About one 60 Hz frame interval |
| Queue-depth-one p95 completion | 0.167 ms | One percent of a 60 Hz frame interval |

The relative margin and three absolute floors can be overridden with
`WPR_RELATIVE_MARGIN`, `WPR_COLD_READY_FLOOR_MS`,
`WPR_WARM_READY_FLOOR_MS`, and `WPR_QUEUE_DEPTH_ONE_P95_FLOOR_MS`. The chosen
values and their provisional status are recorded in the report.

CPU frame timings, source/document phases, batch throughput, JavaScript heap,
browser-wide memory, and Wasm memory capacity are diagnostic-only; they do not
change the primary verdict. Unsupported optional measurements are reported as
unverified. `wasm_memory_capacity_bytes` is linear-memory buffer capacity, not
resident memory or process RSS. `heap_bytes` is a JavaScript heap sample after
the run, not peak memory. The browser-wide memory API may be unavailable because
the benchmark server does not establish cross-origin isolation.

Queue-depth-one latency includes `render_frame()` plus awaited
`debug_gpu_drain()`, including browser and Wasm promise overhead; it is a
pipeline-completion proxy, not pure GPU time or v-sync playback latency. The
paired Student-t intervals describe the six cold and three warm pairs from one
Chromium process. They do not cover between-process/day variance or correct for
multiple comparisons, so the report treats only the three designated primary
metrics as blockers. Integrity, matching quality/scale/raster dimensions,
actual draw counts, nonempty output, and proof that each engine-created device
uses NVIDIA remain hard checks.

Nix users have a lighter option for *serving* — a single static-web-server
binary, no Python (`nix run .#serve`, port/root overridable via `SERVE_PORT` /
`SERVE_ROOT`): it sends `application/wasm` for the engine, compresses
text responses with brotli on demand (the same bytes the prebuilt `.br`
twins hold), and answers with ETags so scene edits show on reload.

Release builds go through the `wasm-release` profile (root `Cargo.toml`: fat
LTO, one codegen unit, `panic = "abort"`) with `+simd128` enabled, kept separate
from the `release` profile so tuning it cannot move a native benchmark baseline.
The compiler profile's earlier A/B measurements are recorded in
`docs/performance_evaluation.md`; they do not substitute for measurements of
the current split SDK.

The SDK package serves all scenes through one runtime; optional capability
chunks and font faces are fetched only when required.

Deploying is the same story: run the build script, copy `web/` including the assembled `runtime/` SDK directory to any static host. Requirements: a WebGPU-capable browser; embeds show guidance
when it's missing. Font faces are fetched from the SDK manifest; gzip is
decompressed with the native `DecompressionStream` API.

### GitHub Pages

[The Pages workflow](../.github/workflows/pages.yml) deploys after successful CI
on `main`, or by manual dispatch. It checks out the triggering commit and prefers
the verified `web-runtime` artifact from that CI run. A missing artifact triggers
a rebuild and web checks; a downloaded artifact with a wrong identity or corrupt
contents fails verification rather than silently rebuilding.

The workflow checks site scenes and assembles `_site` through the gates in
`scripts/ci.sh`. `scripts/site-artifact.sh` validates the SDK manifest and files,
excludes development sources such as `web/tools` and `web/embed/src`, and writes
`.nojekyll`. Release packaging uses the same assembler. Consult the workflow and
[contributor guide](../docs/contributing.md) for the current gate list rather than
copying it into deployment instructions.

Enable Pages with **GitHub Actions** as its source. The public project site is
[amx.j4ger.dev](https://amx.j4ger.dev/); pages also use relative paths for subpath
hosting. See the hosting requirements below for MIME types, compression and CORS.

## Embedding scenes in any page (`<amx-player>`)

Replace the host and illustrative `figures/` paths below with your deployed SDK
entry and scene files.

```html
<script type="module" src="https://your-host/amx-player.js"></script>

<!-- Plain Text is literal; optional features load when needed. -->
<amx-player src="./figures/tokens.amx" autoplay loop controls
            title="From text to tokens" aspect="16:9" hold="1.5"></amx-player>

<!-- rich text / image scenes request their capabilities as needed -->
<amx-player src="./figures/equation.amx" quality="production"
            title="Attention as soft maximum"></amx-player>
```

Behavior:

- **Lazy** — nothing is fetched until the element nears the viewport
  (IntersectionObserver, 200px margin); the page shares one SDK runtime and one WebGPU device across its embeds.
- **No build-time poster** — the skeleton (aspect-ratio placeholder with a
  shimmer) is replaced by the scene's *finished* frame, not its first one.
  This provides a complete composition for readers who do not play the scene
  or prefer reduced motion. New showcase scenes should also have a composed
  opening frame, as described in [the house style](DESIGN.md).
- **Autoplay** — plays on visibility unless `prefers-reduced-motion` or
  `navigator.connection.saveData` says otherwise; offscreen instances pause
  automatically; without `autoplay` (or in those quiet modes) the embed stops
  on that finished frame with a play button.
- **Attributes**: `src` (required), `autoplay`, `loop`, `controls` (control
  bar below the picture — play/pause, landmark scrubber, time, speed),
  `hold` (seconds, default 0.7), `title` (a11y label, shown while loading),
  `aspect` (`16:9`/`4:3`/`1:1`/`9:16`, auto-detected from the scene
  afterwards), `quality` (below), `data-vars` (build-time
  overrides, below), `sealed` and `fit` (page-driven mode, below),
  `no-veil` (suppress the generic loading veil so a page can paint its own
  `.poster-plate` under the player; error veils still show),
  `debug-frame` (frame readout, below).
- **Controls UX** — the strip lives *below* the canvas, full-height and
  always visible: it never covers the picture, and it doubles as the
  timeline inspector. The engine reports the author's `#2s` keyframe
  declarations (and, in compositions, scene starts and transition windows);
  the strip draws them — ticks for keyframes, diamonds for scene starts,
  hatched spans for transitions — so the bar *is* the scene's structure.

  **`debug-frame`** replaces the time chip with a readout for locating a broken
  frame: `f<frame> / <total>  <seconds>  @60fps  rs<render scale>`. The frame
  index is a label on a nominal 60 fps reporting grid (the player renders on
  rAF, so it is not a clock); the seconds are the reproducible value and go
  straight into `animatix image --time`. `rs` is the adaptive render scale the
  quality controller settled on, which distinguishes a raster-scale shimmer
  from a geometry defect. Turn it on per element with the attribute, for a
  whole page with `?amxdebug` on the URL (works on a deployed page without
  editing markup), or per focused strip with `d`. Then **Shift+←/→ step exactly
  one frame** (pausing first, so the rAF clock cannot move off it) and **`c`**
  copies the line to the clipboard and the console. Needs `controls` — the
  readout lives in the strip; `player.frame`, `player.totalFrames` and
  `player.debugReport()` work on any ready player regardless.

  **`?amxdebug`** also adds a page-level stats panel — the readout for "the page
  feels choppy", as opposed to "this frame looks wrong". Fixed
  top-right, pointer-transparent, 5 Hz, one `<pre>` rewritten in place, so it
  does not change what it measures. It reports the achieved page cadence
  (`fps / 60` grid) against how many embeds are visible, playing and drawing;
  the page's raster-pixel total against its budget; and one line per embed:
  state, `rs` (the scale the engine actually rasters at) against `disp` (the
  display match) with the two multipliers that took it (`×q` quality ladder,
  `×px` page pixel budget), `▼` when the raster is below the display match
  (upscaled blit) and `▲` when it supersamples, the raster size, target fps,
  measured render cost, missed-vsync pressure, the controller's current reason
  (`ok`, `budget:fps`, `budget:raster`, `budget:pixel`), and the share of ticks
  the frame dedup absorbed. Players read from the document rather than from the
  module's instance set, so a page that imports the bundle twice still reports
  every element.
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
- **`quality`** — per element, `"draft"` (default) / `"preview"` /
  `"production"`: the build fidelity, i.e. what `BuildQuality` bakes into the
  document at build time. Draft matches the desktop GUI's editing preview;
  production matches what a desktop export renders — the visible difference is
  plot-family sampling (a draft build samples plot curves with 4× the
  tolerance). Changing the attribute rebuilds the scene. Not to be confused with
  *raster* scale, which the element also chooses for itself (below).
- **`data-vars` / `setVars(obj)`** — build-time variable overrides, the web form
  of the CLI's repeatable `--set NAME=VALUE`: each name shadows the scene's own
  `let NAME = …` default, so one template serves many values
  (`web/dev/var_probe.amx` is the probe scene a slider demo can drive). The
  attribute holds a JSON object of number/string/boolean values — write numbers
  as numbers, because a JSON string becomes a string *literal*; `setVars`
  *merges* into the set in effect (so a slider sends just its own name) and
  returns a promise for the debounced rebuild (~150 ms). Playback keeps its
  proportional position
  (the same normalized `t` on the new timeline) and a playing embed keeps
  playing. A set that does not build warns and keeps the previous document, so
  an out-of-range slider value never blanks the figure; a value the engine
  cannot hold (an object, `NaN`, a string with a newline or a backslash) is
  dropped with a console warning, and the wasm `set_vars` reports how many
  entries it accepted. `player.vars` reads the set back. Overrides reach a
  single-scene document only — a composition reports `vars-not-applied` as a
  warning diagnostic, because the browser installs no `tracing` subscriber to
  hear the engine's own note.
- **Raster scale** — internal, no attribute. The canvas backing store tracks the
  device pixels the element actually shows (`CSS px × devicePixelRatio`, read
  from `devicePixelContentBoxSize`), supersampling only up to a **1.5× / 2-megapixel**
  cap (a HiDPI pane asks for more device pixels than a 1280×720 scene has detail
  to give, so 2× bought nothing but quadrupled GPU work). Plain `devicePixelRatio`
  is still honoured up to that cap. Layout is unaffected — the timeline still
  evaluates against the scene's `SceneDimensions`.
- **Page raster budget** — the per-element cap above only bounds one figure. The
  player sums every *playing and visible* embed's display-matched raster and, when
  the total exceeds `MAX_PAGE_RASTER_PIXELS` (**4,000,000**), scales them all by
  `sqrt(budget / total)` (uniform, so the page stays visually consistent). Read
  `window.__amxPageRasterPixels` to override the budget for a page or a probe;
  `player.displayScale` / `player.pixelScale` / `player.rasterPixels` expose the
  split. The pressure ladder and the page budget are combined with **min**, never
  multiplied, so they cannot compound. When pressure does bind, large embeds skip
  extra raster rungs (≥1.5 MP: +2, ≥0.6 MP: +1).
- **Page concurrency cap** — `window.__amxMaxPlaying = N` (finite, ≥1; read at
  allocation time like the raster budget) keeps at most N embeds *live*: the
  scheduler plays the N nearest the viewport centre and **parks** the rest —
  the timeline stops and nothing is drawn, but no play affordance is shown and
  the scheduler owns resumption, so a scroll hands the slot to the next figure
  without a reload. `_parkedFromPage` marks a parked embed and the `?amxdebug`
  HUD prints its state as `park`. This is the lever for a page carrying several
  figures: a browser frame costs a fixed full-target pass per playing embed, and
  the raster budget cannot buy that back (an empty scene costs about what a full
  one does — `docs/performance_evaluation.md` §3.7). Unset means every visible
  embed plays, the historical behaviour.
- **`data-fps` / `fps`** — per-element cadence cap, clamped to 12–60, default 60.
  The page-wide pressure ladder can lower it further, never raise it.
- **Adaptive quality** — the compensation for a page that cannot hold the frame
  rate is a page-wide **cadence ladder** (`CADENCE_STEPS = 60,50,40,30,24,20`),
  applied **fps before raster**: lowering a frame's *rate* costs no sharpness,
  while lowering resolution blurs the figure. The feedback is a **gated
  missed-vsync ratio** measured from the rAF timestamp gaps on the committed 60 Hz
  steps — *not* each embed's own render time. `queue.submit` returns before the
  GPU draws, so render time cannot see GPU cost (a 3.5 ms GPU frame reports
  ~0.5 ms of CPU encode); a controller fed on it never adapts. A single
  exogenous >200 ms stall, a scroll, or a hidden tab is excluded, so those do not
  cost a quality notch. Only at the ladder floor (20 fps) with sustained pressure
  is a smaller raster tried, and a trial is judged by the pressure ratio, not by
  CPU encode time (which barely changes with resolution). Each embed is given a
  phase offset so N figures do not all render in one tick, and a hard per-tick
  cap derived from the ladder. The loop is capped to the 60 fps timeline grid
  (measured refresh, so 120/144 Hz renders 60), autoplay draws are skipped while
  scrolling, and an embed whose held frame has not changed is not re-rendered.
  `player.set_render_scale(s)` / `render_scale()` still work; `player.pressure`
  and `debugReport()` report the live signal. `web/dev/adaptive-probe.html` with
  `scripts/adaptive-probe.mjs` measures the policy: `AP_MANUAL=0` is real rAF
  (needs a headed window), `AP_MANUAL=1` is the synthetic-tick mode used by CI
  (`AP_INJECT=<steps>` injects pressure deterministically). §3.7 of
  `docs/performance_evaluation.md` has the frame-cost numbers.
- **Custom SDK manifest** — import `configureRuntime` from the loader module
  and call `configureRuntime({ manifestUrl })` before any player starts loading.
  By default, the manifest is resolved beside the loader module.

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

### When a scene arrives

A figure loads when it comes within 200 px of the viewport, and its arrival is a
burst of main-thread work: fetch the `.amx` (plus imports and assets), one or two
*synchronous* wasm builds (23–54 ms for a tour scene, measured), then the first
render. Starting that while the reader is still scrolling puts a long task inside
the frames they are using to move — which is what a "choppy" page is made of,
since a steady frame on real hardware is ~4 ms.

So arrivals go through one page-wide queue: nothing starts until the scroll has
settled for 140 ms, and only one figure builds at a time. A figure scrolled past
before its turn is dropped and re-queued if it comes back. Both rules are capped:
a queue head waits at most 900 ms even on a page that never stops scrolling, and a
`src` change re-enters the queue immediately. The visible cost is that a
scene you stop on appears ~150 ms later than it used to, covered by its skeleton;
the cost avoided was a 40–170 ms hitch in the middle of every scroll. A grid of
figures fills in one at a time rather than all at once — the same total work, but
never stacked into the same frames.

A figure that has stopped changing does not redraw either. A loop rests on its
finished frame for the whole `hold` window while the player keeps asking 60 times
a second, so the engine compares what a frame depends on (time, scene and raster
and surface size, document revision) against what is already on the canvas and
does nothing when they match — measured at 13% of ticks on this site's own tour.
See `docs/performance_evaluation.md`, "A frame already on the screen is not a
frame to draw".

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

### Scene authoring

Review scenes visually with `animatix storyboard`, including the first and resting
frames. Follow [the house style](DESIGN.md) and the
[language specification](../docs/spec.md) for visibility and timing semantics.
In particular:

- Actors declared before the first keyframe without explicit opacity start
  hidden. Reveal them before judging render or overlap probes.
- Later declarations paint over earlier ones. Include visible actors when testing
  paint order.
- A `Path` fills by default. For an open stroked path, set `stroke` and
  `fill_opacity: 0.0`.
- Runtime fonts are explicit. Check `missing-glyph` diagnostics instead of
  assuming a browser-installed font or an arbitrary Unicode character is available.
- Avoid overlapping entrance actions on a container and its children; inspect the
  resulting opacity rather than assuming independent fades.

Hosting requirements for the runtime host: serve JavaScript as
`text/javascript` and Wasm as `application/wasm`. Keep the manifest's raw and
`.gz` files together. The loader streams gzip Wasm and font files through
browser decompression, including on static hosts without compression
negotiation; it also recognizes responses already decoded by the host.
The engine entry bundles its JavaScript dependencies and is executed as a
module script with Subresource Integrity. A cross-origin SDK host must allow
anonymous CORS with `Access-Control-Allow-Origin`; the page's CSP must permit
that host in `script-src` and allow WebAssembly compilation. Sources and
assets must also satisfy the page's `connect-src` and their hosts' CORS rules.
The `.amx` files may live anywhere CORS permits. `serve-web.py` sends `no-cache` for pages,
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
  Actors carrying the same `shared_id` in both scenes are suppressed from the
  two scene rasters and drawn once as an interpolated FLIP overlay, matching
  the desktop renderer's shared-element transition.
- **Imports fetch** — a scene may `import` `.amx` files beyond the bundled
  library. The build stops on an unsupplied import and reports the resolved
  path in `missing_imports`; the embed fetches it relative to the scene
  (transitive imports close over repeated rounds) and retries. Shells driving
  the player directly do the same with `add_module(path, text)`.
- **Assets fetch** — `Image`/`Svg` actors with a literal `url` resolve relative
  to the scene file at any nesting depth (scenes, components, keyframe and
  control-flow bodies, and container children), fetched alongside the scene
  (absolute URLs work too, including inside imported modules — their urls
  resolve against the module's own location, but the engine keys the cache by
  the literal url string, so asset names share one flat namespace across the
  scene and its imports). Dynamic
  `url = expr` assignments are not listed.
- **Runtime fonts** — the web engine no longer embeds font bytes. The player
  loads the face descriptors from `runtime-manifest.json` and fetches required
  font files alongside scene preparation, caches them per page, and registers
  them before compilation. Set `data-fonts` on the embed for additional
  TTF/OTF URLs; those are fetched in parallel and cached too, but are
  available alongside manifest-provided fonts. A failed custom font fetch
  rejects preparation as a resource error; a font that loads but cannot satisfy
  text compilation can fail the build. In either case the current document
  stays visible. WOFF2 is not decodable. Browser-installed fonts are not
  scanned; include a supported TTF/OTF face when the SDK faces do not cover the
  scene's text.
- **Audio tracks are not played** (no Web Audio wiring yet).
- **Video export** is native-only and requires FFmpeg. The web SDK exposes
  on-demand PNG encoding through `encode_rgba_png` for RGBA readbacks; this is
  not a browser video-export workflow.
- Native plugins don't exist on this platform; `libloading`-based extensions
  are desktop-only.
- No general in-browser editor: `applySource` backs the tour's and the demos'
  own figures (`tour/editor.js`), but authoring lives in the desktop app.

### Differences from the desktop renderer

The render stack is the desktop's — same engine crates, same vello/wgpu
`RendererCore`, same `GpuFilterBackend` per filter target (including the
zero-readback pending-composite blit), same `TransitionCompositor` for
multi-scene blends. What differs is the platform around it:

| | Desktop (CLI / GUI) | Web SDK |
|---|---|---|
| Rich text (Typst markup, Code highlighting, Math) | ✓ | on-demand capability |
| `Image` assets | ✓ | on-demand capability |
| `Svg` actors | ✓ | on-demand capability |
| Fonts | system scan plus bundled fonts | manifest faces plus optional `data-fonts` |
| Extensions (`plugin-loading`) | ✓ | ✗ |
| Audio | muxed at export | not played |
| Video export | FFmpeg with the `video` feature | not available |
| Raster export | PNG / WebP | on-demand PNG encoding API |
| Build quality | `Production` (export), `Draft` (GUI editing) | `quality` attribute: `draft` default, `preview`/`production` opt-in |
| Build-time vars | `--set NAME=VALUE` | `data-vars` attribute / `setVars()` |

The pixel-level consequence of build quality: plot-family actors (`Graph`,
`PlotCurve`, `VectorField`, …) sample with 4× the tolerance under the default
`draft`, so dense curves can be slightly coarser than a desktop export renders
them — `quality="production"` closes that gap at rebuild cost. The GUI preview
also uses `Draft`; choose matching quality when comparing render paths.
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

## Single SDK and runtime manifest

The web distribution is one SDK package. Its `runtime-manifest.json` records
the build identity, versioned modules, integrity metadata, font faces, and
capability chunks. The loader fetches the shared runtime once and activates
rich text, media decoding (images and SVG), or raster encoding only when the
scene or caller needs them. Text is always literal Unicode; markup belongs in
`Typst`, `Math`, `Code`, or `Equation`.

Build with `scripts/build-web.sh`; the assembled package is `web/runtime/`.
Deploy that directory with the loader. The loader defaults to its sibling
manifest, or a page can choose a manifest explicitly:

```js
import { configureRuntime } from "./runtime/amx-player.js";
configureRuntime({ manifestUrl: "https://cdn.example/animatix/runtime-manifest.json" });
```

Configuration is page-wide and must be set before loading starts. Font faces
are described by the manifest and fetched as needed; custom `data-fonts` remain
available. Build size and capability-split results are generated by the
distribution scripts and are not asserted here as acceptance evidence.

## Diagnostics & probes

- `web/dev/perf-probe.html` — the frame-cost harness
  (`?scene=&players=&frames=&scales=&reps=&base=`): loads a scene, sweeps render
  scales, prints a table and publishes `window.__perf`. It drives frames by hand
  and drains the GPU queue (`debug_gpu_drain`) rather than timing the page's rAF
  loop, because a backgrounded/headless tab delivers no rAF callbacks at all and
  `queue.submit` returns several milliseconds before the GPU has drawn anything.
  `reps` repeats each sample and keeps the fastest (a browser frame timed from a
  script is hostage to whatever else the machine is doing); `base` can point the page at an alternate SDK directory for an A/B comparison. The findings and the method are in
  `docs/performance_evaluation.md` §3.7.
- `web/dev/multi-probe.html` — four embeds on one page; publishes
  `{players, ready, playing, wasmFetches}` on `window.__probe_state` and, with
  `?readback=1`, a GPU-buffer readback (average color / distinct colors) of
  the first player — a compositor-independent pixel check.
- `web/dev/svg-probe/` — `Svg`/`Image` media probes: one page per scenario
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
- `web/dev/hybrid-vs-compute.html` — differential probe: the same synthetic
  vector scene rendered by the classic vello compute renderer (WebGPU,
  offscreen+blit) and by `vello_hybrid` (WebGL2, direct to canvas), from one
  wasm module (`crates/animatix-hybrid-bench`). Build it with
  `scripts/build-hybrid-bench.sh --dev` into `web/dev/pkg-hybrid-bench/`
  (gitignored), open the page with `scripts/serve-web.py`, and press Run; it
  reports per-frame `submit`/`gpu`/`clear` milliseconds and publishes
  `window.__bench`. The headless end-to-end check is
  `nix develop .#web-ci -c node scripts/hybrid-bench-smoke.mjs` (software
  SwiftShader; proves the wiring, not performance). For real numbers run a
  headed browser on the GPU: `HB_HEADED=1 HB_UNSAFE_WEBGPU=1 CHROME=/path/to/chromium
  node scripts/hybrid-bench-smoke.mjs`. On hybrid-GPU laptops Chrome gives
  WebGPU the discrete GPU but WebGL2 the integrated one, so the two backends are
  not always on the same GPU — the page reports which adapter each got. Dev-only:
  it measures raw renderer throughput, not effects, transitions or text shaping.

> Two probe traps have burned this pipeline before (see `docs/history/2026-09.md`,
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
