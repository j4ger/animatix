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

> **Instrument warning, 2026-10-03.** `debug_bench_frames` on a *demo* page is
> not usable for small A/B comparisons: the page's adaptive quality step changes
> the raster scale between runs, and p50 swung 1.7 ms → 55.2 ms across two runs of
> the same build. Pin `set_render_scale` first, and prefer
> `web/demos/perf-probe.html` with `?base=`, which was built for this. A frame-cache
> gating change was measured on `web/demos/matrix/` (four embeds, machine busy)
> and the result — p50 45.2 ms before, 49.4 ms after, at a pinned 0.5 scale — is
> **inconclusive in both directions**, so no perf claim is attached to it.

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

## Test-suite flake worth fixing

`cargo test -p animatix-text` fails `fast_path_caches_results` when run with
default parallelism (it passes with `--test-threads=1`). The text cache is a
process-global memo, so tests that exercise it are order-dependent. The repo
workflow already runs the suite serially, which hides it — but a contributor
typing the obvious command gets a red build for no reason. Either give the cache
a per-test scope or make that test own its cache generation.

## Planned Features

| Feature | Notes | Status |
|---|---|---|
| `Code` custom highlighting theme | Token colours now follow the active colorscheme via a role→named-token palette (`ResolvedColorscheme::highlight_palette`, resolved at build). A fully custom palette (arbitrary per-role colours, or a user-supplied `.tmTheme`) is not exposed as an authoring surface; deferred until a real content need. | Not started |
| Re-declaration morphs sharing one keyframe stamp | When two actor re-declarations share a single `#t` stamp, the actors render their **target** geometry on every frame before the morph starts (an `Ellipse`→`Rect` re-declaration shows a square on pre-morph frames; a size-less `Polygon` leaked as a default-size `Ellipse`). One re-declaration per stamp is safe — `web/tour/scenes/morph.amx` exercises the full sequence leak-free. Found while frame-auditing `web/scenes/hero.amx` (probes in the 2026-10-01 site session); the hero now choreographs its trio with rotations/resizes instead. Fix direction: a morph track's pre-start evaluation must hold the *previous* declaration, not the target. | Not started |

## Dependency & Verification Constraints

| Item | What it is | Status |
|---|---|---|
| Vello pin lift | The workspace pins Vello to `d8686d52` because upstream #1558 (image-atlas residency) makes an image-bearing render draw nothing when a non-image render runs between two image renders — our multi-scope filter shape hits it on every frame. The dependency-boundary matrix (`crates/animatix-render/tests/vello_img_probe.rs`) now **asserts** its A–H history rather than printing counts, and was verified to fail when a draw is forced empty. Probing forward is no longer only a rev change: upstream `main` (`f3000c8d`, measured 2026-10-04) builds against wgpu 30 while this workspace pins 29, so the two `wgpu::Device` types do not unify and `animatix-render` does not compile — the pin lifts with a **wgpu 29→30 bump**, after which the probe and `animatix video dogfood/projects/effects-wave1/entry.amx` decide whether #1558's regression is gone. The invariant this protects is documented on `RendererCore`. | Pinned — blocker renamed from "upstream fix" to "wgpu major bump" |
| Fewer Vello renders per frame | Since 2026-09-16 each `Filter` scope renders, seeds, and dispatches at its **region** size (content bounds ∪ support, 64-px quantized) instead of the full canvas — see `docs/effects.md` §3. Measured floor (Intel UHD 770, `demos_frame_cost`): every example demo is ≤14 ms/frame except the synthetic 13-scope `30_effects_catalog` (63-71 ms ≈ 15 fps). Probes show the chain itself is only ~12 ms of that; the rest is main vello render + per-scope fixed render/harvest costs, serialised per scope. Scope merging into one shared sub-scene render is the structural lever; on this iGPU it projects to only ~50 ms for the 13-scope reference, so it stays **Not scheduled** without a discrete-GPU or many-scope content driver. Experiments that did not move the floor: halving the resolution (+15%, i.e. not pixel-bound), encoder merging, and the zero-readback pending evaluate path (all neutral). | **Not scheduled** — measured floor documented, lever known |
| GUI panel-level screenshot regression | The headless screenshot driver (`ANIMATIX_SCREENSHOT`, `ANIMATIX_SCREENSHOT_SIZE`) reviews a single, non-interactive app state. The diagnostics peek, the export dialog, and multi-actor curves were verified by compile + unit tests only, because `--demo-script` can only play/pause/scrub. Extending its grammar to switch tabs / open dialogs would make those surfaces screenshot-testable. | Not started |

## Planned Effects

Wave 1 (`Sharpen`, `Vignette`, `MotionBlur`, `Grain`, `Levels`) shipped
2026-09-13, wave 2 (`Duotone`, `Posterize`, `Edge`, `LensDistortion`,
`DropShadow`) shipped 2026-09-14 — see `docs/history.md` for both.

The second-input-texture ABI bump shipped 2026-10-04 (`docs/effects.md` §4.1:
binding 5 carries the pre-chain original), and with it `Bloom` and
`DropShadow.softness`. A generic chain `Mix` is deliberately **not** planned:
`Bloom`'s `keep` parameter is already a linear mix of the chain result with the
original, so a second effect over the same math would be a duplicate — an N-way
mix would need named intermediate chain outputs, which is a chain-model change
rather than another input texture.

What is left in this family after `Glass` shipped (2026-10-05,
`docs/spec.md` "Glass"): the backdrop pass reads the live render target, so a
scope's children can only ever composite **above** the frost — there is no way to
put content *underneath* it — and every `Glass` scope costs its own region copy +
chain run + two composites, with no merging between scopes (`n` panels are `n+1`
passes over their regions). `EffectRegion` is still a plain rect; the rounded
panel clip is a signed-distance term in the blit shader, so any other
non-rectangular scope shape still wants real region masks. And the root loop's
`can_post_composite_filter` rule ("nothing may render after me") still forces
`Filter` scopes that are not last onto the inline readback path — `Glass` no
longer inherits that restriction, but the filter side of it is unchanged.

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

Behaviour gaps found while polishing the site's scenes, each worked around
scene-side first. Four of the nine are now fixed in the engine (see the resolved
list at the end of the easing-pass section); what is left is below.

| Item | What it is | Status |
|---|---|---|
| Single-line `text_align` / `text_max_width` do not move the anchor | A one-line `Text` is always centred on `at:` regardless of `text_align` (probe: left/right/plain identical), so right-aligned label columns overlap their bars and left-aligned captions jitter when swapped. The alignment should apply to the line box's anchor, or the combination should warn. Worked around by hand-computing `at:` x per label. | Not started — attempted 2026-10-03 and reverted; see below |
| `text_align` cannot be fixed without deciding what it means | An implementation was written and measured, then reverted. Two findings from it, both needing a decision before anyone tries again:

1. **The property default is `"left"` but the renderer has always centred.** `property_registry.rs` seeds `text_align` with `"left"`, while `compile_text_*` centred the measured ink regardless of the value — so the default has been lying for as long as it has existed, and `"left"` / `"center"` / `"right"` were indistinguishable (the roadmap's original probe). Any real implementation has to pick: either the default becomes `"center"` to match observed behaviour, or `"left"` starts meaning left and every un-aligned caption in the language re-anchors.
2. **Making alignment real moves a large part of the corpus.** Measured with two binaries differing *only* in the alignment work (box-centring for bounded blocks + an anchor shift for unbounded ones, default set to `"center"` so untouched scenes should have been stable): about two thirds of the corpus frames differed (`examples/basics/00_hello.amx` at PSNR 40, i.e. genuinely visible). So the shift is not confined to scenes that ask for alignment — it reaches ordinary centred text everywhere, most likely through the bounded path where the declared box is wider than the drawn line. The earlier count of "200 of 304" is not noise-controlled and should be re-derived if it ever matters; see the noise-floor row below.

What that means for the fix: it cannot be a renderer tweak. **DECIDED 2026-10-03 — split the two meanings into two properties.** `text_anchor: "left"|"center"|"right"` places the *whole text block* relative to `at:`; `text_align` keeps its existing meaning of how lines sit *inside* a wrapped box. Both default to what the engine actually does today (block centred on the anchor), so nothing moves until an author asks. `text_anchor` must be **appended** to `PROPERTY_DESCRIPTORS` (`property_id_order_is_pinned` forbids reordering) and needs both derivations to follow — `schema::raw_property_types()` and `property_registry::BINDINGS` — plus the content pass over `examples/`/`web/` that deletes the site's hand-computed `at:` x for right-aligned label columns, which is the workaround this lands on top of. | Not started — design decided |
| Plot actors' axes render pure white under `dynamic_layout` | `Graph`/`BarChart` axes come from `DEFAULT_WHITE` — the brightest pixels on the ink page, out-shining the amber lead. Outside `dynamic_layout` the scenes dim them with `stroke: stroke.default` (verified in `web/tour/scenes/plots.amx`); inside a `dynamic_layout` scene that same property blanks the whole graph, so the epicycles/spectrum axes stay white. Needs the plot axis style to read the colorscheme's `stroke.default` by default. (Its companion finding — `Graph.map()` returning screen coords at half the px/unit the curve uses — is fixed; see below.) | Not started |
| `pulse intensity` is additive | `intensity: 1.04` means 2.04× scale (peak = start × (1 + N)) — three demo diagrams were destroyed at their thesis beat before the semantics were spotted. Now documented in `docs/spec.md`; a gentler authoring story (percentage semantics or a lint for `intensity > 1`) is open. | Documented |
| Native `FontContext::new()` does not register `BUNDLED_FONTS` into its fontdb | The premise needs correcting: the **plain fast path never needed it** — `compile_text_fast` consults `BUNDLED_FONTS` by family before the database, which is how bundled faces reach text on a machine that has no Open Sans installed at all (`fc-match "Open Sans"` → Noto Sans CJK, measured 2026-10-05). What the native context *does* miss is the database entry, so (a) `resolve_font_family`/system-fallback questions and (b) the typst world's face list still depend on `with_fonts_and_fallback` being the embed-only path. The remaining question is narrower than written and worth re-asking before anyone bundle-registers into every native context. | Opened narrower |
| Percentage and `fill` child sizes collapse in a `Row` | `examples/layout/27_layout_text.amx`'s PercentSizing scene is the reference for `size: (25%, 80)` / `(50%, 80)` / `(fill, 80)` inside `row: Row, size: (900, 120)`, and it renders two ~2px slivers and one ~100px square instead of 225/450/fill — verified identical at HEAD and after the `min_width` correction, and unchanged by adding `dynamic_layout: true`. `timeline/layout.rs:946` hardcodes `let parent_content_size = [0.0f32, 0.0f32];` on the path the comment labels "If the container has no layout_size", so percentages resolve against zero and `min_width` does not lift the result either. Needs the container's declared `size` to reach the child constraint pass. | Not started |
| Layout has no `max_width` constraint, and the name is already taken | `animation_track.rs` gives geometry `min_width`, `min_height` and `max_height` — and no `max_width`; `timeline/layout.rs:952` hardcodes `let max_w = f32::INFINITY;` where the child cap would be read. So a Row/Col item can be given a floor but never a ceiling. It cannot be fixed by adding the property, because `max_width` is bound to the text wrap width (`ActorField::TextMaxWidth`), which is why `27_layout_text.amx` could carry `b: Rect, max_width: 400` with a comment claiming a cap for as long as it has existed. Needs a distinct name (`max_item_width`?) plus the geometry field and the taffy constraint. | Not started |
| `Applicable::Everything` rows hide the drop class this lint exists for | `color`, `opacity`, `at`, `offset`, `transform`, `solo` and friends are declared applicable to every actor, so `inapplicable-property` passes them unconditionally — and the two silent drops found and fixed this round (the plot family discarding `opacity`, `Arrow` discarding `color`) are both in that set. The rows are also what the inspector offers and what plan slots are filtered by, so a wrong `Everything` is a wrong editor too. Durable fix is to narrow each such row to the primitives that read it, which is the same audit `applicability_table_agrees_with_reads.rs` does for the `Actors(&[…])` rows — that test cannot see these because they are not name lists. | Not started |
| Static keyframe `.text =` assignments overprint | **Fixed 2026-10-03** — the text path is now fenced, so an undated swap holds the declared string until its own stamp (`an_undated_text_swap_holds_the_old_string_until_its_own_stamp`; `web/demos/hash/lookup.amx` draws one formula before `#5.2s` instead of two overlapping). The one-actor-per-string workaround in `web/demos/hash/scene.amx` is no longer needed and can be folded back. The same fence now covers numbers and colours. | Resolved |
| ~~A bare assignment animates from the previous keyframe~~ **Resolved 2026-10-03**: `docs/spec.md` wins. `write_property_plan_slot` now fences an undated change one millisecond before its stamp, so `r.opacity = 0.0` at `#2s` holds until 2s instead of easing there from the previous keyframe. Two extension tests had encoded the ramping reading and were rewritten to assert the step. Verified by **positive control, not pixels**: on the pinned probe scene at 1 s the fenced build leaves 2725 lit pixels (still opaque) and the unfenced one 2125 (already half-ramped). A whole-corpus A/B — 149 `.amx` × 2 times = 298 frames, two binaries differing only in the fence — found 280 byte-identical and 18 differing by 1–44 raw RGBA bytes out of 3.7 MB, which is *below* the same-binary control on the very same scenes, so the change has no measurable visual effect: every scene that wanted a ramp already wrote a `[duration]`. (The numbers first shipped with this commit — "288 identical, the other 16 at PSNR ≥107 dB" — compared a build with itself and were wrong; a later "18 of 304 frames change" repeated that mistake by counting PNG-byte drift as a change.) | Resolved |
| Pixel A/B has no noise floor yet | The render-either-side-and-`cmp` method this round used for content checks cannot resolve a small change: **the same binary is not byte-stable across processes.** Measured 2026-10-03 on `dogfood/probes/008-render-correctness/text.amx` at 1 s — three renders of one binary differ from each other by 20, 36 and 46 raw RGBA bytes (of 3 686 400), while that binary's two builds (which really do differ) are 12 and 24 bytes apart. PNG file sizes drift as well, so comparing files is noisier still, and `web/demos/transformer/scenes/pipeline.amx @ 4` came out *identical* between builds but 8 bytes apart between two runs of one build. Text/glyph scenes show it and `web/demos/matrix/scene.amx` does not, so the suspect is glyph rasterization or shaping order. Until that is pinned down: any engine A/B claim needs (a) a same-binary control and (b) a positive control proving the change is live — the track-level probe above is the shape to copy. | Not started |

## Easing pass — what else it turned up (2026-10-02)

Landed: one easing name table (there were four), `ease: cubic-bezier(…)` /
`ease: spring(…)`, `expo-out` / `expo-in-out` / `spring`, `bounce` →
`bounce-in`, a physical `bounce` action, and a formatter that stops deleting
eases. What follows is everything the pass surfaced but did not fix, sorted by
whether the behaviour is *wrong* or merely *shaped badly*.

### Bugs — the behaviour is wrong

| # | Item | Evidence |
|---|---|---|
| 2 | **`animatix fmt` deletes comments and reflows declarations.** | Formatting `web/scenes/hero.amx` drops its 19-line header block and collapses every multi-line declaration onto one line (202 → 124 lines). The formatter rebuilds from the AST and never re-emits trivia, so the "single lossless tokenizer" promise stops at the parser. Re-scoped 2026-10-03: statement-position comments are dropped at exactly one place (`parser/token_parser.rs:28`, the token filter), and `Stmt::Comment` already exists, already prints, and is already tolerated by every walker — but **statements carry no byte spans** (only `Property::value_span` does), so the `attach_trailing_comments` offset trick cannot be reused for them. Real options: (a) let `Comment` tokens through the grammar and make every `choice`/`delimited_by` site skip them, or (b) give statements spans and back-fill like trailing comments. Neither is small; (b) is the one that also gives diagnostics better locations for free. |
| 3 | **The native-plugin ABI silently flattens most easings to Linear.** | `easing_code` (`extension_native_plugin.rs:1577`) maps everything outside {EaseIn, EaseOut, EaseInOut} onto code 0, so a plugin reading a `spring`, `bounce`, or `cubic-bezier` track sees a linear one. Pre-dates this pass. Widening the code space is a plugin-API version bump, not an easing change. |
| 5 | **A component actor declared between keyframes renders half-drawn.** | `hash/lookup.amx`'s `q2`/`q3` (`LabeledBox`) appeared as a dim box *without its label* in beats before their own `fade-in`. Worked around scene-side with `opacity: 0.0`, but the engine question stands: "declared inside a keyframe is visible immediately" should not produce a partially drawn component. Narrowed 2026-10-03: the mechanism first blamed for it — a re-declaration sampling its own target because a sibling wrote a keyframe at the same `#t` — was probed on plain shape actors in four orderings and does **not** reproduce (the pre-frame keeps the old shape and colour and the morph still lands on target); `a_redeclaration_starts_from_what_it_replaces_not_from_its_target` pins that. What is left is specific to *component* actors, whose parts are separate sub-actors seeded during expansion, and it still needs a minimal repro outside the demo. |
| 13 | **A string literal's backslashes grow every time a file is formatted.** | The lexer stores the raw text between the quotes (`token.rs:397` `lex_string` skips escape pairs but never decodes them) while `format_expr`'s `Str` arm re-escapes (`s.replace('\\', "\\\\")`), so `"…\n…"` becomes `"…\\n…"` and then `"…\\\\n…"`: `animatix fmt` is not idempotent and each pass drifts the rendered text further. Four repo files are affected (`dogfood/probes/008-render-correctness/text.amx`, `examples/gallery/sorting_theatre.amx`, `examples/layout/27_layout_text.amx`, `web/demos/transformer/scenes/attention.amx`); `roundtrip_examples.rs` excuses instability *only* when the changed line contains a backslash, so a fifth cause cannot appear silently. The fix is a language decision, not a printer tweak: decode escapes at lex time so `Expr::Str` holds the value (then `\n` in `text:` finally means a newline, and Typst's `\sqrt` stops needing `\\sqrt`), or declare the field raw-text and give the ~10 GUI `Expr::Str(…)` construction sites an explicit escaping constructor. Today the field means both things at once, which is why no printer rule can be right. |

### Design flaws — the shape invites the wrong thing

| # | Item |
|---|---|
| 14 | **An action's preposition is stored as one of its targets.** `move b to (300, 200)` parses to `targets: ["b", "to"]`, `args: [Num(300), Num(200)]` — the grammar (`parser/stmt.rs:505`) reads any run of identifiers as targets and then flattens a parenthesized argument group into the same `args` list, so the AST cannot tell `to (300, 200)` from `to 300 200`, nor a preposition from a second actor. Consequences: the printer has to *guess* punctuation (`format_action` now wraps ≥2 args in parens to stay reparseable), `animatix fmt` could not round-trip the positional form at all until that fix, and every `.amx` in this repo therefore uses the keyword form instead (`move panel [to: (500, 260), 700ms]`) — which is either good documentation or a scar from the formatter, and is not something any test can currently tell. Giving `Action` an explicit argument *shape* (preposition kept, groups preserved) is the durable fix; until then no tooling can distinguish "move two actors" from "move one actor to". |
| 8 | **Nothing keeps a coupled ease pair in sync.** The hero's carriage "rides the draw tip" by repeating the `draw-in`'s start, span *and* ease. A mechanical retune of the draw silently separated the two and no test or diagnostic noticed. Either give the follower its own construct, or warn on two segments with identical span and different easing. |
| 9 | **Curve names that promise physics.** An easing is a scalar map over progress between two values; it can only modulate travel *along* that line, so no easing can produce an arc. `bounce` was renamed `bounce-in` and the boundary is now written into `docs/spec.md`, but `elastic` still promises a spring and delivers a progress wobble. |
| 10 | **Headless cannot see the site's primary surface.** Partly solved 2026-10-03: `AmxPlayer::debug_readback_rgba(t, cb)` hands back the frame's RGBA bytes, which a browser harness turns into a PNG through an `OffscreenCanvas` — verified end to end against `web/demos/matrix/`, capturing a real 1280×720 WebGPU frame out of headless Chromium (usage in `web/README.md`). Still open: it captures the *scene*, not the *page* (site chrome, scroll-reveal and hover furniture remain invisible), and a probe frame costs seconds, so it is a review tool rather than a CI gate. |
| 11 | **Nothing keeps the corpus on its motion vocabulary.** The site was 155-of-207 annotations on one quadratic curve, with ~880 timed statements carrying no ease at all (so: Linear). Fixed by convention plus `docs/spec.md`'s reference table, but nothing enforces it — a scene can slide back to monoculture unnoticed. A lint over "which role uses which curve" is the durable version of this pass. |
| 12 | **The GUI cannot author a parameterized ease.** `ease: cubic-bezier(…)` / `spring(…)` parse, evaluate, plot in the curve panel and round-trip through source, but the inspector's dropdown is registry-driven and offers only named curves. Editing control points needs a small numeric editor in the keyframe table. |

### Also resolved by this pass

`ease: custom` silently ≈ ease-in-out (dead escape hatch) · a second
`parse_easing_name` in the engine that had already drifted from the canonical
table · the GUI's source-edit path overwriting any custom curve with `linear`
· the formatter deleting `ease:` from property assignments ·
`bounce [restitution: …]` warning about a modifier its own signature declares
· the matrix scenes drawing +y downward against a `(cos, sin)` readout ·
morph spans with mismatched point counts · the nav playhead hanging half off
the page at scroll 0 · `figcaption` squeezing its caption to 12px below
~500px.

Resolved by the follow-up round (2026-10-03): #1 (`play` transition easing —
parsed, printed, threaded to the compositor) · #6 (a `web-build` dev shell, and
`build-web.sh` no longer overwrites the linker flags it is run with) · #7 (the
`assert_eq!(arms, arms)` guardrails are gone; `ast_format_coverage.rs` now
probes every `Stmt`/`Expr`/`InlineItem` variant and compares whole ASTs, and it
was checked to fail when the deleted fields are restored) · and a defect #7's
removal turned up immediately: **`animatix fmt` rewrote any positional action
with two or more arguments into text the parser rejects**
(`move b to (300, 200)` → `move b, to 300, 200`), so one format pass destroyed
such a file; `format_action` now emits a reparseable form and
`roundtrip_examples.rs` checks serializer stability across every `.amx` in
`examples/`, `dogfood/` and `web/`.

Also #4, whose real size the row understated: `unused-label` fired **347 times**
across this repo's own corpus — every one of them on an *actor*, none on a
binding or component — because an unreferenced actor still renders. A warning
that is wrong on every legitimate file is not a warning, and it also made the
column unreadable for any lint added later. The actor case is now a hint (a
`let` nobody reads still warns), and `animatix check`/`lint` fold hints from
their text output while `--format json` keeps them, so the 56 files carrying one
still say so once. `never-revealed` in the build layer remains the lint that
catches an actor that is declared and never shown.

From the web-demo review pass, same round: **`Arrow` ignored an authored
`color:`** (it is now one of the stroke-only shapes that inherit it, via a new
`ShapeKind::is_stroke_only()` so the next such shape cannot be forgotten) ·
**the plot family discarded authored `opacity`** (one fix in the shared dispatch
covers Graph, BarChart, ContourSet and VectorField) · **`Graph.map()` returned
screen coords at half the px/unit the curve it describes uses**, because it read
the dotted half-size track while `map_inverse` read the side channel · and
**an `import` written inside a scene block froze that scene's clock**, whose
chain turned out to be worth writing down because nothing in it is obvious:
`group_scenes` promoted a `config` block into `Scene.config` only while the
scene was still empty, so the import demoted the block into the scene body →
`Scene.config` empty → `extract_duration_from_config` found nothing → the
duration fell back to keyframe-span inference → and the web player wraps its
clock on that duration, so `always` blocks kept re-evaluating a truncated loop.
Position no longer matters: any `config` inside a scene is now promoted, and
multiple blocks merge.

And the engine-level lint the round was meant to add: **
`inapplicable-property`** (`build/actor.rs`, next to the existing typo warning, sharing
its call sites so coverage matches). It asks the same `Applicable::includes`
predicate the inspector filters with, so the editor and the build cannot
disagree, and it carries the property's own byte span — the first build-phase
property lint that does, which is why it does not need the CLI's
label-to-position heuristic. Running it over the corpus found three things on
its first pass:

- `text_max_width`'s row listed only `Legend`, while the text engine reads it as
  the canonical wrap width — 94 shipped scenes were being described to the
  inspector as setting a property their actor ignores. The row is now
  `Any[Actors(Legend), TextLike]`.
- `font_size` omitted `Legend`, which reads it for its own labels — the same
  class, on the false-positive side, and the reason `applicability_table_agrees_with_reads.rs`
  now cross-checks every `Actors(&[…])` row against the primitives' own match
  arms (verified to fail when a type is removed from a row it reads).
- `examples/layout/27_layout_text.amx` declared `max_width: 400` on a `Rect` and
  told readers it capped the item at 400px. It did nothing: layout has no width
  ceiling at all, and the name belongs to the text wrap width. Both are open
  items above; the example now demonstrates `min_width` and says so. See `docs/history.md`, "The Easing Pass".

## Property value forms the checker rejects but the builders read

**Resolved 2026-10-03** for the four rows below, kept because the shape is still
live: `animatix_syntax::schema::raw_property_types()` is one `Type` per property
*name*, so one property shared by two readers can only have one row. A property
whose builder accepts `"auto"` therefore needs `Type::Union`, and widening a
*shared* row is only safe once every other reader reports what it drops — which
is exactly what unblocked `gap`:

| Property | Row before | Also accepted at build | Now |
|---|---|---|---|
| `gap` | `Num` | `auto` / `"auto"` — BarChart bar spacing (`build/plot.rs`) | `Union(Num, Str)`, and each container now pushes `InvalidPropertyValue` for a value it cannot read (`container_layout_value_it_cannot_read_is_reported` covers Row/Col/Grid) |
| `bar_width` | `Num` | same, BarChart only | `Union(Num, Str)` |
| `max_value` | `Num` | same, BarChart only | `Union(Num, Str)` |
| `show_axis` / `show_labels` | `Bool` | `"true"` / `"false"` | `Union(Bool, Str)`, plus a diagnostic for a string that is none of `true/1/false/0` — previously `show_axis: "yes"` silently read false |

Wrong values still fail: they now produce the *consumer's* message
(`BarChart 'c' bar_width expects a number or "auto", got Str("wide")`) instead of
only a type warning, which is the stronger signal either way. Pinned by the
golden `broken_corpus/19_bar_chart_documented_forms.amx`, which asserts zero
analyzer diagnostics for every documented form.

### Anything declared inside a container is not property-checked at all

Found while writing that golden. The semantic walker (`crate::walk::walk_stmts`)
recurses into statement bodies but **not** into `ActorDecl.children`, so:

```
row: Row, gap: 24 { r: Rect, size: (10, 10), opacity: "late" }   // no warning
top: Rect, size: (10, 10), opacity: "late"                      // type-mismatch
```

Both `unknown-property` and `type-mismatch` skip every actor inside braces, which
is most of the language's content. Fixing it means teaching `walk_stmts` (or the
diagnostic pass) to visit `InlineItem::Labeled` declarations, and every existing
scene then gets checked for the first time — expect a real wave of findings
(`check_examples.sh` currently reports 19 allowed warnings; this could triple
that) and duplicates to dedupe first. Worth doing, not a drive-by.

This came out of closing the last open item of the phase-2 gallery handoff
(closure record in `docs/history.md`, "Phase 2 handoff — closed out";
`BarChart` missing from the `gap` applicability row), which also produced
`bar_chart_gap_is_applicable_to_bar_chart` in
`crates/animatix/tests/applicability_table_agrees_with_reads.rs` — the generic
test in that file cannot see BarChart, because the chart's properties are read by
the shared plot builder rather than by a file in `src/primitives/`.

## Motion-vocabulary round — what it left open (2026-10-05)

The round itself is recorded in [`history.md`](history.md) ("The Motion-Vocabulary
Round — Landed Work"). Four things are still owed, and none of them is a feature
the language is missing:

- **Continuous `font_weight` on the plain-text fast path.** The Typst path instances
  the Open Sans `wght` axis; the fast path resolves a static face, because
  `ttf-parser` reads `fvar`/`gvar` and never applies the deltas. Doing it means
  either pre-instancing N static weights offline (the axis range is 300–800, so
  `Regular`/`Medium`/`Bold` would cover what authors ask for) or moving the fast
  path's outline extraction onto `skrifa`, which owns variation instances. Until
  then the slim web embed steps, and `docs/spec.md` says so.
- **Two analyzer/engine perf residuals** against the 2026-10-02 baseline:
  `analyzer_update__small` +10.4% (116.8 → 129.0 µs, tight within-run spread) and
  `property_plan_lookup_and_sample` at 9.98 ns against a stored 8.3 ns. Both are
  small-input fixed costs; `analyzer_update/large` (-15.3%) and `/dogfood`
  (-11.8%) got *faster* on the same code.
  **Attributed, partly: neither comes from this round's features.** Measured A/B on
  this machine with the same command, `property_plan_lookup_and_sample` is
  9.9453/9.9853/10.050 ns at `ded1b17d` (before `Glass`, the continuous weight, the
  applicability fixes) and 9.8812/9.9827/10.166 ns at HEAD — the same number, so the
  +17% predates every feature commit in batch 6 and merely *surfaced* there, because
  the earlier compares were run while concurrent builds polluted them. The regression
  therefore lives somewhere in batches 1-5; the next probe is an env-gated stage timer
  inside `Analyzer::update` for a `00_hello.amx`-sized fixture plus a two-point
  re-measure at the batch boundaries, since no profiler exists on this box
  (`perf`/`valgrind` both absent).
- **The rest of the site content pass.** Four tour sections now close differently,
  each with a measured signature rather than a taste claim (see `f79099c1`'s
  successor): `effects` loses detail before existence (edge energy −91.6% while
  3.8% of the frame is still content), `plots` exits positionally by `shift`
  (content 56.8% → 12.6%, edges only −64.6% — the data leaves, the type stays
  crisp to the end), `glass` fogs over, `light_camera` ends on the light itself.
  The remaining sections deliberately keep the shared reverse-order wipe; the
  larger design question — palette, chrome, page structure — is owned by
  `handoff_web_redesign.md`, and the demo-poster deletion is the owner's call.
- **The scope-property rule is now a convention, not a mechanism.** `color`
  carries an explicit `Applicable::Except(&["Glass", "Filter", "Mask"])` list and
  the scopes have no `ShapeKind`, so today they report — but a *new* container
  primitive gets no such protection automatically. Deriving "this actor paints no
  surface" from the caps (one flag, consulted by the paint predicates) is the
  follow-up that would make it structural.
- **The keyframe query path has one property table now** (`Self::field_for_property`,
  `f79099c1`), but the same copy-until-one-path-stops-honoring-it shape still lives
  in the read side: `read_property_value*` / `apply_property_to_field` resolve by
  name independently of it. Auditing those against the mapper is the follow-up — the
  fold itself is the pattern, not the last instance.
- **`web/demos/posters/*.png`.** Eight 1280×720 stills nothing references since the
  hub switched to live `data-hoverplay` embeds; `web/README.md` says so. Deleting
  them is the owner's call, so they stay.

## Web delivery — the two claims a real browser still owes (2026-10-06)

Batch 7 (`history.md`) fixed the engine fault behind the site's page errors and
made every delivered page verifiable headless. Two claims cannot be closed on
this box, and the next session should not pretend the harness covers them:

- **`Glass` frost pixels in the web player.** Headless Chromium cannot make a
  readable RGBA texture (`Could not find SharedImageBackingFactory … RGBA_8888 …
  WebgpuRead`) — the readback path loses the device the moment it allocates — and
  `--screenshot` returns byte-identical frames for a looping scene, so canvas
  contents are not in the capture either. What *is* proven in the browser: the
  scene builds under the shipped profile, the engine initialises, and the figure
  renders through the same scale-aware `drain_pending_layers` the native path
  uses. The pixel behaviour of the frost rests on the native measurement
  (`(93,27,32)` frosted vs `(255,0,0)` unfrosted on a 4 px border). A machine with
  working WebGPU in a browser can settle it: open `/tour/` §05b and read the panel
  interior at `t=1.0` (radius 0) against `t=3.0` (radius 22) — the scene is built
  as its own A/B — via `<amx-player>`'s `debug_readback_rgba`.
- **One page mixing both engine profiles.** `demos/svg-probe/profiles.html` loads
  slim and full in the same document; headless hands the second engine no adapter
  (`No suitable graphics adapter found`), so the probe reports one scene error
  here that says nothing about the page. `web/README.md` claims a mixed page
  downloads each profile once and holds one device per profile — that claim is
  untested against a real browser.

Then the maintenance item this batch made cheap: `cargo test -p animatix-web
--test site_scenes` in both feature sets is now the content gate, and the Pages
job runs it plus an embed-bundle drift check. If a future scene needs the full
profile, the fix is `profile="full"` on the element that plays it — the gate names
the page, not just the scene.

## Found by the transformer review, left open on purpose (2026-10-06)

- **String escapes keep their backslash in the value.** `lex_string`
  (`crates/animatix-syntax/src/token.rs:421`) skips a `\x` pair so an escaped
  quote cannot end the literal — and then takes the source slice verbatim, so
  `text: "1 · \"it\" looks back"` renders the backslash. The transformer page
  worked around it with typographic quotes; the grammar question is real and
  unanswered: `\$` inside a `Typst` payload (`examples/gallery/sorting_theatre.amx`)
  *depends* on backslashes surviving to the markup, so "unescape every `\x` in
  the lexer" would silently change that scene. Any fix needs to say which
  escapes are `.amx`'s and which belong to the payload — a per-property or
  per-actor-kind rule, not a global one. `docs/spec.md` currently documents
  neither, which is the smaller half of the problem.
- **`differs` cannot see a thin rule.** The check samples the frame sparsely, so
  a beat whose entire change is a 3px line (0.26% of a 1280×720 plate) reads
  0.00% — three beats in the transformer set are like this and are recorded as
  comments in the `*.verify.txt` files rather than asserted at a floor the
  sampler cannot honour. A region-scoped `differs` (the same idea as `reveals`,
  which already restricts to an actor's bounds) would close it.
- **The block pulse in `pipeline.amx` is a whisper.** As the token climbs, each
  block grows to 584×83 and settles back: 1.0% of that block's region changes,
  which is legible as motion but not as an event. Left alone deliberately —
  the alternative (a colour lift or the `pulse` action) changes the figure's
  register, and this pass had no way to judge "more vivid" except by measuring
  the change, which is exactly the metric that says the current pulse is small.

## Found by the demo and feature-page review, left open (2026-10-06)

Measured by A/B rendering (same scene with and without one beat) and counting
changed pixels, so each item below is a reproduced dead end rather than a
reading of the code.

- **An authored `opacity: 0.0` on a component *instance* makes every entrance on
  it a no-op.** Two identical `LabeledBox` declarations, one with `opacity: 0.0`
  and one without, both with `fade-in` then `shift`: the plain one reveals and
  travels (11 168 px changed across the shift), the authored-zero one contributes
  **0 pixels in its whole band** — it never appears, so nothing can move.
  `AGENTS.md` ("Entrance opacity and declared duration") documents the opposite
  rule for built-in primitives: an explicit `0` is a seed any entrance lifts to
  1.0. Component instances do not get that lift. `web/demos/hash/lookup.amx` hit
  this: query chips 2 and 3 were invisible for the whole figure, which the sweep
  could only report as two "dead beats". Worked around in content by hoisting the
  declarations above the first keyframe (where the automatic seed already hides
  them) and dropping the authored `0.0`; the real fix is to make the instance path
  honour the same entrance-opacity rule as the primitives, and then to warn when an
  entrance targets an actor whose opacity track it cannot lift — nothing in
  `animatix check` said a word about a chip that never rendered.
- **`highlight` / `unhighlight` paint on nothing but a `Typst` actor, and the
  per-fragment path is unreachable in practice.** `Highlight` writes
  `track.highlight.*` for any target and the signature promises a rectangle
  "behind equation fragments", but `examples/projects/fft_explain.amx` renders
  byte-identical frames with its three `highlight decomp_eq.fN` beats deleted
  (1 px of 921 600 at the peak, 0 px four frames later), and a minimal
  `Equation { Fragment, Fragment }` scene shows the same for both `highlight` and
  `reveal-in` on a fragment — per-fragment *actions* record, and never draw.
  `scene_eval.rs:1670` does build `HighlightLayer` commands from `frags`, so the
  question is why the Equation branch is not the one that renders these scenes
  (`primitives/equation.rs` has no highlight code at all, only a doc comment
  promising it). Needs a rendered-frame test at the boundary — the existing
  `highlight.rs` tests assert keyframes land on the track, which is exactly the
  half that works.
- **The raw/dotted label asymmetry in those actions is worth a look but is not a
  demonstrated bug.** Both highlight actions validate a dotted target with
  `ensure_target_exists` (which resolves `eq.f1` → `f1`) and then look the *raw*
  string up in `tracks`. Routing every lookup through the resolved label changed
  no rendered frame and no test could be written that failed without it, so the
  change was reverted rather than shipped as a "fix" — the leaf label evidently
  reaches the map either way. If the paint gap above gets a rendered-frame test,
  start there and settle this at the same time.
- **`differs`/`DEAD-BEAT` sampling is meaningless for a multi-scene plate.** The
  per-keyframe sweep samples `#Ns` stamps as absolute times, but in a file with
  `# Scene` sections those stamps are scene-local — `demos/hash/scene.amx` was
  flagged at "10.5s→11.2s" while its q3 shift at that label moves 9 831 px. The
  sweep now restricts per-beat checks to single-scene files and keeps only the
  held-frame check for plates; a plate needs its beats resolved through
  `animatix timeline` before it can be swept beat by beat.

## Graph tick labels collide at the origin (2026-10-06)

`web/tour/scenes/plots.amx` draws `0.0` (the y-axis label) and `0` (the x-axis
label) at the crossing of the two axes, with the axis rule running through both.
It is the `Graph` primitive's tick layout, not that scene's authoring — every
`Graph` with both axes labelled and a zero in both domains gets the same
overprint. The fix belongs in the tick-label pass: skip the zero tick on one
axis when both axes are drawn, or offset the pair, which is what most plotting
libraries do. Left alone in this pass because the only content-side workaround is
to move the domains off zero, which changes what the figure teaches.

## Browser-only: an imported colorscheme is lost by every `# Scene` section (2026-10-06)

**The five demo pages and the home page render in the fallback palette in a real
browser, and nothing native says so.** Each `<amx-player>` on those pages logs

    amx-player: descent.amx built with 1 diagnostic(s)
    { severity: "warning", code: "unknown-colorscheme",
      subject: "scene 'Descent'",
      message: "Unknown colorscheme 'ink'; using the default-dark built-in scheme instead." }

`ink` is not built in — it is `pub let ink = Colorscheme { … }` in
`web/demos/lib/theme.amx`, which every one of these files imports. The CLI
resolves it (`animatix check web/demos/gradient/descent.amx` → OK, no
diagnostics), the slim CLI agrees, and the wasm `site_scenes` gate passes because
it asserts the build *succeeds*, not that it is quiet. So the only witness is a
devtools console, and what it costs is the palette: background, text and accent
tokens all come from `ink`, and the fallback is `default-dark`.

The discriminator is the scene header, and it is exact across the whole site: a
file with `# Section` headers logs one warning per section (`gradient/scene.amx`
— 6 sections, 6 diagnostics; `hash/scene.amx` — 3, 3), while a file whose config
is file-level and has no section header logs none (`tour/scenes/glass.amx`,
`tour/scenes/components.amx`, and the nine recipes scenes, all importing the same
theme and the same `colorscheme: "ink"`). Merging the section's
`config { colorscheme: "ink" }` into the file-level `config` does **not** fix it —
tested — so the palette registry, not the config placement, is what the section
build does not see.

Where to look: the embed builds each section as its own timeline
(`crates/animatix-web/src/web.rs` → the host's build path in
`crates/animatix-web/src/host.rs`), and the module graph is handed over as
fetched sources. The native single-source path evidently registers imported
`Colorscheme` values before evaluating configs; the per-section path does not,
or does it after the lookup. A regression test belongs in `site_scenes`: assert
**no warning-or-worse diagnostics** for each embedded scene, built through the
same protocol the embed uses — that is the check that would have caught this, and
it will fail until the fix lands, which is the point.
