# Animatix Roadmap

Canonical source of truth for **remaining** work. Completed, resolved, and
closed work lives in [`history.md`](history.md) — do not re-add it here.

---

## Backlog & Prioritization

### GUI UX Redesign (active)

Source of truth: `docs/gui_design_language.md` §12 (diagnosis, target
information architecture, per-surface redesign, open decisions). The
2026-09-11 phases shipped directly on `main`; the remaining items below are
small enough to land there too.

Track solo shipped 2026-09-14 as an authored property rather than an
ephemeral view toggle: `solo: true` hides every other subtree recursively in
preview and export alike (see `docs/spec.md`), and the timeline track header
gained a solo button that edits the source through the normal property-edit
pipeline, so it is undoable and persists in the `.amx`.

Multi-actor curves shipped 2026-09-14: the Curves panel overlays every selected
actor, qualifying labels as `actor · property.X` so legend keys and widget ids
stay unique, shading later actors' canonical channel colours deterministically,
and committing each drag against the track that owns the keyframe. The value
window already derives from the visible curves, so it fits whatever the legend
leaves on. The Inspector's read-only graph editor still shows one actor.

Export/settings polish shipped 2026-09-14: the failure tag keeps its short
label but reveals the full encoder message on hover with a copy action, the
dialog gained explicit `Encoder` (codec + libx264 preset) selectors for the
formats that honour them, and a reset control restores the default settings
while keeping the output path. CRF/bitrate controls remain unscheduled — they
need encoder-parameter plumbing in `animatix-render`, not just dialog state.

Diagnostics peek shipped 2026-09-14: the status-bar chip raises a transient
overlay anchored above itself (single click peeks, double click pins the docked
panel; Escape, a click outside, or picking a diagnostic closes it).

Phase 0 shipped 2026-09-11: visible auto-key (default off), caret-anchored
completion, click-latched Delete scope, layout-preserving Inspector toggle,
explicit keyframe-diamond model, labeled scene inspector, platform-aware
shortcut display, action-identified Layers menu, remapped tool keys
(`V/A/R/S/G/P`), and real group scale/rotate.

Layout phase shipped 2026-09-11 (screenshot-verified): Inspector and Code
share one right-hand tab group; region sizes are `clamp(ratio × available,
min, max)` with a per-frame pixel-bound pass and a 120px tile floor; Animate /
Code / Inspect / Focus presets plus Reset layout; sidebar merged 6 → 3 labeled
tabs (Project / Outline / Library) with the editor promoted into the detail
region; `pill_tab_bar` degrades label-first.

Bottom tab group shipped 2026-09-11: Timeline and Curves share one bottom tab
group (Timeline active by default). The Curves tab is an interactive F-curve
editor over the selected actor — horizontal drag retimes through the batched
`MoveKeyframes`, vertical drag rewrites a keyframe value through the new exact
`SetKeyframeValue` command, right-click sets easing, click/Shift+click selects
(the selection is shared with the timeline), and a ruler scrubs the playhead.
Reachable from the toolbar, command palette, and `ViewAction::ShowCurves` /
`ShowTimeline`.

Later phases shipped 2026-09-11: timeline keyframe model (property-granular
`KeyframeId` selection, multi-keyframe drag with a single undo step via batched
`MoveKeyframes`, empty property lanes from the animatable-property registry,
actor track header eye/lock); autosave to a `<file>.amx.autosave` sidecar with
a Recover/Discard prompt on startup; in-editor find-match highlighting; Library
drag-to-place onto the canvas; narrow-window compact mode (48px sidebar icon
rail + overlay drawers). The `eparts` library got a batch of fixes (scrolling
virtualized `List`/`Tree`, themed `Select` with keyboard nav and a focus ring,
overlay-aware `Dialog`, dead slots wired, new per-component slot groups, a
macro-generated partial theme layer, and `Theme` grouped into
`palette`/`components`/`elevation`).

Remaining:

| Item | Scope | Status |
|---|---|---|

### Web playback performance (active)

Method and measurements: `docs/performance_evaluation.md` §3.7. Shipped
2026-09-30: a browser frame-cost harness (`debug_bench_frames` /
`debug_gpu_drain` / `web/demos/perf-probe.html`, `?base=` for A/B of two engine
builds), display-matched raster (`set_render_scale`), the canvas backing store
sized to the displayed pixels, and a page-wide quality step driven by the shared
rAF tick interval — six embeds on one page went 21.7 → 9.4 ms per tick.

Native parity, measured on the same GPU/scene/resolution and **confirmed in a
focused window**: web 3.42 ms/frame at display size (1052×592) against native
1.63 ms at the larger 1280×720 raster, which additionally copies 3.7 MB back to
the CPU. The wasm CPU path and the canvas blit were both **ruled out by A/B** as
the cause, so the residual is GPU-side: matching native's frame cost takes
rasterizing at roughly half the display size. One embed at its displayed size
still has ~4.9× headroom on a 60 Hz budget, so this is fidelity headroom, not a
smoothness problem. Per-frame CPU cannot be measured from the harness (the page
quantizes `performance.now()`), and rAF cadence is unmeasurable in the automated
tab, which is why the harness drives frames by hand.

Remaining:

| Item | Scope | Status |
|---|---|---|
| Attribute the GPU-side gap | Narrowed by a WebGPU microbenchmark (§3.7 "Where the gap is not"): not fill rate (a full-target colour write is 0.058 ms vs a 3.42 ms frame), not the present blit (0.003 ms), not pass count (pinned vello uses exactly two compute passes). It sits inside the execution of those passes in the browser — Dawn's dispatch/barrier handling, plus a per-frame encoding upload bounded at ~0.22 ms/MB. JS-side compute timing is unusable (it reports CPU submission), so the next step is a GPU capture: RenderDoc against a `--no-sandbox` Chromium, or Dawn tracing via `--enable-dawn-features`. | Not started |
| Render vello directly into the canvas surface | Would delete the offscreen target and the entire blit pass (~0.2 ms). Chromium accepts `RENDER_ATTACHMENT \| STORAGE_BINDING` in `GPUCanvasContext.configure` (probed 2026-09-30), contradicting the assumption the offscreen+blit workaround was built on — but a `Bgra8Unorm` swapchain may not be a legal vello target, and Firefox/Safari are unverified. Needs a capability probe plus a fallback. | Not started |
| `wasm-opt` speed pass | Release builds are size-optimised (`-Oz`). The build-profile A/B says the whole wasm CPU slice is small and unmeasurable from JS, so this is unlikely to be where the gap is — but it is untested. | Not started |
| Frame cache on the web path | `restore_frame_cache` bails whenever a filter backend is present, and the web player always passes one — so every browser frame is a full re-evaluation. Small, but pure waste. | Not started |
| Scale animated `Filter` scopes | `GpuFilterBackend` allocates at scene resolution, so a filtered scene keeps its full-resolution cost under a reduced raster scale (correct output, no saving). | Not started |
| rAF-cadence sampling in automation | The quality-step logic is currently only exercisable in a foreground tab; the automated harness measures throughput, not smoothness. | Not started |
| `debug_readback` intermittently traps the wasm | The readback probe is flaky rather than wrong at a particular time, and with `panic = "abort"` a trap takes the whole page's shared engine with it: after one, later probes in that tab fail (`error` states, `Runtime.evaluate` timeouts) until the browser restarts. Evidence from the 2026-10-01 sweep (flake `.#web` Chromium, `--webgpu`): the same call alternates — `tour/scenes/syntax.amx` `debug_readback(1.95)` timed out once and returned 27 distinct colours on the repeat; `web/demos/hash/scene.amx` `debug_readback(19.5)` hung twice while 19.0/20.0/20.5 were fine; one tour sweep surfaced `RuntimeError: unreachable` with frames inside `animatix_web_bg.wasm`. Meanwhile `render_frame` at the same time is fine and the transformer page's seven scenes, the hero and the hash/sorting compositions all read back — so the shipped demos render; only the probe path dies. Affects `web/demos/multi-probe.html?readback=1` and the `svg-probe/*` harnesses, which is why they need a fresh browser per probe until this is understood. | Not started |

Done (recorded so they are not re-litigated):

| Item | Outcome |
|---|---|
| wasm build profile (`wasm-release`: fat LTO, 1 codegen unit, `panic = "abort"`, `+simd128`) | **No measurable frame-time effect** in an alternating A/B (2.67–2.86 ms vs 2.68–2.74 ms, 3 loads per arm). Kept for the artifact size: raw −11%, brotli −4.9%. |
| Canvas backing store at the displayed size | Done (embed `_applyRenderScale`). Worth ~0.14 ms where `display px × dpr` is below the scene resolution, and a no-op above it. |
| Foreground-window measurement | Done 2026-09-30. The gap is **real**, not background-tab throttling: 3.42 ms/frame at display size vs native 1.63 ms at a larger raster. |

## Planned Features

| Feature | Notes | Status |
|---|---|---|
| `Code` custom highlighting theme | Token colours now follow the active colorscheme via a role→named-token palette (`ResolvedColorscheme::highlight_palette`, resolved at build). A fully custom palette (arbitrary per-role colours, or a user-supplied `.tmTheme`) is not exposed as an authoring surface; deferred until a real content need. | Not started |
| Re-declaration morphs sharing one keyframe stamp | When two actor re-declarations share a single `#t` stamp, the actors render their **target** geometry on every frame before the morph starts (an `Ellipse`→`Rect` re-declaration shows a square on pre-morph frames; a size-less `Polygon` leaked as a default-size `Ellipse`). One re-declaration per stamp is safe — `web/tour/scenes/morph.amx` exercises the full sequence leak-free. Found while frame-auditing `web/scenes/hero.amx` (probes in the 2026-10-01 site session); the hero now choreographs its trio with rotations/resizes instead. Fix direction: a morph track's pre-start evaluation must hold the *previous* declaration, not the target. | Not started |

## Dependency & Verification Constraints

| Item | What it is | Status |
|---|---|---|
| Vello pin lift | The workspace pins Vello to `d8686d52` because upstream #1558 (image-atlas residency) makes an image-bearing render draw nothing when a non-image render runs between two image renders — our multi-scope filter shape hits it on every frame. Lift the pin when upstream ships a fix, then re-run the dependency-boundary matrix (`crates/animatix-render/tests/vello_img_probe.rs`) and `animatix video dogfood/projects/effects-wave1/entry.amx`, confirming the backdrop survives every frame. The invariant this protects is documented on `RendererCore`. | Pinned, guard in place |
| Fewer Vello renders per frame | Since 2026-09-16 each `Filter` scope renders, seeds, and dispatches at its **region** size (content bounds ∪ support, 64-px quantized) instead of the full canvas — see `docs/effects.md` §3. Measured floor (Intel UHD 770, `demos_frame_cost`): every example demo is ≤14 ms/frame except the synthetic 13-scope `30_effects_catalog` (63-71 ms ≈ 15 fps). Probes show the chain itself is only ~12 ms of that; the rest is main vello render + per-scope fixed render/harvest costs, serialised per scope. Scope merging into one shared sub-scene render is the structural lever; on this iGPU it projects to only ~50 ms for the 13-scope reference, so it stays **Not scheduled** without a discrete-GPU or many-scope content driver. Experiments that did not move the floor: halving the resolution (+15%, i.e. not pixel-bound), encoder merging, and the zero-readback pending evaluate path (all neutral). | **Not scheduled** — measured floor documented, lever known |
| GUI panel-level screenshot regression | The headless screenshot driver (`ANIMATIX_SCREENSHOT`, `ANIMATIX_SCREENSHOT_SIZE`) reviews a single, non-interactive app state. The diagnostics peek, the export dialog, and multi-actor curves were verified by compile + unit tests only, because `--demo-script` can only play/pause/scrub. Extending its grammar to switch tabs / open dialogs would make those surfaces screenshot-testable. | Not started |

## Planned Effects

Wave 1 (`Sharpen`, `Vignette`, `MotionBlur`, `Grain`, `Levels`) shipped
2026-09-13, wave 2 (`Duotone`, `Posterize`, `Edge`, `LensDistortion`,
`DropShadow`) shipped 2026-09-14 — see `docs/history.md` for both.

`Bloom`, soft `DropShadow`, and a generic chain `Mix` are blocked on the
second-input-texture ABI bump (`docs/effects.md` §4.1) and are **not**
scheduled: the ping-pong chain overwrites the original after the first pass,
so an add-back has no source. `Mix` additionally needs named intermediate
chain outputs — a separate chain-model change, not part of that bump.

## Morph: `strategy: fade` has no cross-fade for state shapes

`MorphStrategy::Fade` is documented as a cross-fade overlay, and the tour
once taught it that way, but for shapes whose geometry lives in the shape
state rather than a `vector_paths` track (Rect, Ellipse, and points-list
Polygons — everything the PF-4 shortcut covers) a fade re-declaration
renders as a color lerp followed by a one-frame silhouette swap: nothing
cross-fades. A real implementation needs the morph span to evaluate both the
start and end states and draw them at opposite opacities, which means the
frame path must be able to build a second state from the *start* type's
primitive (the same machinery `render_type_name` now dispatches per frame).
Until then the tour demos morph strategies that visibly work: auto, match,
path_arc, stretch.

## Web-demo review pass — engine findings (2026-10-02)

Behaviour gaps found while polishing the site's scenes; all worked around
scene-side, none fixed in the engine:

| Item | What it is | Status |
|---|---|---|
| `Arrow` silently ignores `color:` | The primitive paints shaft and head from `stroke_color` only (`primitives/arrow.rs`), but `color` is a registered property, so an authored `color:` on an Arrow builds, renders grey (`stroke.default`), and warns nothing. Either consume `color` as the stroke fallback (like `Line`?) or emit the drop warning the code rules require. Found making attention's amber accent contract false on screen. | Not started |
| `import` inside a scene block freezes `always` clocks | A single-scene document with `# Scene` + `import "../lib/theme.amx"` after the header evaluates `always` blocks at a frozen `t` (matrix/rotation's hub card was a static poster of a rotation that never happened; moving the import above `config` fixed it; multi-scene docs with in-scene imports animate their non-`always` content). Needs a root-cause pass in the module/scene build path, and it should at least warn. | Not started |
| Single-line `text_align` / `text_max_width` do not move the anchor | A one-line `Text` is always centred on `at:` regardless of `text_align` (probe: left/right/plain identical), so right-aligned label columns overlap their bars and left-aligned captions jitter when swapped. The alignment should apply to the line box's anchor, or the combination should warn. Worked around by hand-computing `at:` x per label. | Not started |
| `pulse intensity` is additive | `intensity: 1.04` means 2.04× scale (peak = start × (1 + N)) — three demo diagrams were destroyed at their thesis beat before the semantics were spotted. Now documented in `docs/spec.md`; a gentler authoring story (percentage semantics or a lint for `intensity > 1`) is open. | Documented |
