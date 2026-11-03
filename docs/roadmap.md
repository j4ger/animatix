# Animatix Roadmap

Canonical source of truth for **remaining** work. Completed, resolved, and
closed work lives in [`history/README.md`](history/README.md) — do not re-add it here.

---

## Backlog & Prioritization

### Structure gate (active)

The large-file/large-function programme is governed by [`ecs.md`](ecs.md) §8:
`scripts/structure.sh` fails on a production file >1000 lines (tests >2000), a
production function >200 lines (tests >300), or a stale allowlist row. A split
deletes its row and never raises one. [`ecs.md`](ecs.md) is the normative
description of the timeline storage, not a work list.

### Property reference generator (deferred)

[`properties.md`](properties.md) is hand-maintained. A generator over
`animatix-core::property::PROPERTY_DESCRIPTORS` would remove the drift risk, but
the reference is grouped by user-facing categories rather than descriptor order,
so the generator needs a category map first. Until then the name pin test and
`scripts/check-docs.sh`'s Applies-to check keep the table honest.

### Web playback performance (active)

> **Instrument warning, 2026-10-03.** `debug_bench_frames` on a *demo* page is
> not usable for small A/B comparisons: the page's adaptive quality step changes
> the raster scale between runs, and p50 swung 1.7 ms → 55.2 ms across two runs of
> the same build. Pin `set_render_scale` first, and prefer
> `web/demos/perf-probe.html` with `?base=`, which was built for this. A frame-cache
> gating change was measured on `web/demos/matrix/` (four embeds, machine busy)
> and the result — p50 45.2 ms before, 49.4 ms after, at a pinned 0.5 scale — is
> **inconclusive in both directions**, so no perf claim is attached to it.

Method and measurements: `docs/performance_evaluation.md` §3.7. The harness,
display-matched raster/quality work, and measured web/native gap are archived in
[`history/2026-10.md`](history/2026-10.md) ("Roadmap cleanup"). Remaining:

| Item | Scope | Status |
|---|---|---|
| Attribute the GPU-side gap | Narrowed by a WebGPU microbenchmark (§3.7 "Where the gap is not"): not fill rate (a full-target colour write is 0.058 ms vs a 3.42 ms frame), not the present blit (0.003 ms), not pass count (pinned vello uses exactly two compute passes). It sits inside the execution of those passes in the browser — Dawn's dispatch/barrier handling, plus a per-frame encoding upload bounded at ~0.22 ms/MB. JS-side compute timing is unusable (it reports CPU submission), so the next step is a GPU capture: RenderDoc against a `--no-sandbox` Chromium, or Dawn tracing via `--enable-dawn-features`. | Not started |
| `wasm-opt` speed pass | Release builds are size-optimised (`-Oz`). The build-profile A/B says the whole wasm CPU slice is small and unmeasurable from JS, so this is unlikely to be where the gap is — but it is untested. | Not started |
| Browser A/B of scale-animated `Filter` scopes | Re-run the browser measurement after the raster-scale work (`FilterBackend::set_raster_scale`, pixel-unit `EffectParamUnit::Pixel` parameters, ramp-width pin); see `docs/performance_evaluation.md` §The filter chain follows the raster. | Needs the browser A/B |
| rAF-cadence sampling in automation | The quality-step logic is currently only exercisable in a foreground tab; the automated harness measures throughput, not smoothness. | Not started |
| `debug_readback` intermittently traps the wasm | The readback probe is flaky rather than wrong at a particular time, and with `panic = "abort"` a trap takes the whole page's shared engine with it: after one, later probes in that tab fail (`error` states, `Runtime.evaluate` timeouts) until the browser restarts. Evidence from the 2026-10-01 sweep (flake `.#web` Chromium, `--webgpu`): the same call alternates — `tour/scenes/syntax.amx` `debug_readback(1.95)` timed out once and returned 27 distinct colours on the repeat; `web/demos/hash/scene.amx` `debug_readback(19.5)` hung twice while 19.0/20.0/20.5 were fine; one tour sweep surfaced `RuntimeError: unreachable` with frames inside `animatix_web_bg.wasm`. Meanwhile `render_frame` at the same time is fine and the transformer page's seven scenes, the hero and the hash/sorting compositions all read back — so the shipped demos render; only the probe path dies. Affects `web/demos/multi-probe.html?readback=1` and the `svg-probe/*` harnesses, which is why they need a fresh browser per probe until this is understood. | Not started |

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

## Effects — remaining surface gaps

Wave 1, wave 2, the second-input-texture ABI (`Bloom`, `DropShadow.softness`),
and `Glass` are shipped history: see
[`history/2026-09.md`](history/2026-09.md) ("Built-in effects, wave 1/2"),
[`history/2026-10.md`](history/2026-10.md) (Glass in the resolved-decisions
table), and "Roadmap cleanup" in the same volume for the chain-`Mix` decision.
What remains in this family:

- The backdrop pass reads the live render target, so a scope's children can only
  ever composite **above** the frost — there is no way to put content
  *underneath* it.
- Every `Glass` scope costs its own region copy + chain run + two composites,
  with no merging between scopes (`n` panels are `n+1` passes over their regions).
- `EffectRegion` is still a plain rect; the rounded panel clip is a
  signed-distance term in the blit shader, so any other non-rectangular scope
  shape still wants real region masks.
- The root loop's `can_post_composite_filter` rule ("nothing may render after
  me") still forces `Filter` scopes that are not last onto the inline readback
  path — `Glass` no longer inherits that restriction, but the filter side of it
  is unchanged.

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
scene-side first. Four of the nine were fixed in the engine and are archived in
[`history/2026-10.md`](history/2026-10.md) ("Easing pass" and "Web-demo review
pass"); what is left is below.

| Item | What it is | Status |
|---|---|---|
| `pulse intensity` is additive | `intensity: 1.04` means 2.04× scale (peak = start × (1 + N)) — three demo diagrams were destroyed at their thesis beat before the semantics were spotted. Now documented in `docs/spec.md`; a gentler authoring story (percentage semantics or a lint for `intensity > 1`) is open. | Documented |
| Native `FontContext::new()` does not register `BUNDLED_FONTS` into its fontdb | The premise needs correcting: the **plain fast path never needed it** — `compile_text_fast` consults `BUNDLED_FONTS` by family before the database, which is how bundled faces reach text on a machine that has no Open Sans installed at all (`fc-match "Open Sans"` → Noto Sans CJK, measured 2026-10-05). What the native context *does* miss is the database entry, so (a) `resolve_font_family`/system-fallback questions and (b) the typst world's face list still depend on `with_fonts_and_fallback` being the embed-only path. The remaining question is narrower than written and worth re-asking before anyone bundle-registers into every native context. | Opened narrower |
| Pixel A/B has no noise floor yet | The render-either-side-and-`cmp` method this round used for content checks cannot resolve a small change: **the same binary is not byte-stable across processes.** Measured 2026-10-03 on `dogfood/probes/008-render-correctness/text.amx` at 1 s — three renders of one binary differ from each other by 20, 36 and 46 raw RGBA bytes (of 3 686 400), while that binary's two builds (which really do differ) are 12 and 24 bytes apart. PNG file sizes drift as well, so comparing files is noisier still, and `web/demos/transformer/scenes/pipeline.amx @ 4` came out *identical* between builds but 8 bytes apart between two runs of one build. Text/glyph scenes show it and `web/demos/matrix/scene.amx` does not, so the suspect is glyph rasterization or shaping order. Until that is pinned down: any engine A/B claim needs (a) a same-binary control and (b) a positive control proving the change is live — the track-level probe above is the shape to copy. | Not started |

## Easing pass — what else it turned up (2026-10-02)

What follows is everything the pass surfaced but did not fix, sorted by whether
the behaviour is *wrong* or merely *shaped badly*; what it did land is archived in
[`history/2026-10.md`](history/2026-10.md) ("Easing pass — what else it resolved").

### Bugs — the behaviour is wrong

| # | Item | Evidence |
|---|---|---|
| 2 | **`animatix fmt` reflows declarations.** | Comment deletion is fixed (2026-11-03): `Formatter::format_source` reinserts every `//` class from the lossless token stream — file header, free-standing, in-block, trailing, comment-only — with round-trip, idempotence, and scrubbed-AST tests. Remaining: formatting still collapses every multi-line declaration onto one line (`web/scenes/hero.amx` 202 → 124 lines), and nested-block indentation is sometimes doubled. |
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

## Property values inside a container are not checked at all (2026-10-03)

Found while writing the resolved-forms golden (archived in `history/2026-10.md`). The semantic walker (`crate::walk::walk_stmts`)
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
(closure record in `docs/history/2026-10.md`, "Phase 2 handoff — closed out";
`BarChart` missing from the `gap` applicability row), which also produced
`bar_chart_gap_is_applicable_to_bar_chart` in
`crates/animatix/tests/applicability_table_agrees_with_reads.rs` — the generic
test in that file cannot see BarChart, because the chart's properties are read by
the shared plot builder rather than by a file in `src/primitives/`.

## Motion-vocabulary round — what it left open (2026-10-05)

The round itself is recorded in [`history/2026-10.md`](history/2026-10.md) ("The Motion-Vocabulary
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
  palette/chrome/page-structure direction they follow is recorded in
  `docs/history/2026-10.md` ("The instrument redesign"), and the demo-poster deletion
  is the owner's call.
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

## Web delivery — the two claims a real browser still owes (2026-10-06)

Batch 7 (`history/2026-10.md`) fixed the engine fault behind the site's page errors and
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
- **`differs` compares every pixel, so thin rules are visible.** The old
  sparse-sampling caveat is gone: a beat whose entire change is a 3px line
  (0.26% of a 1280×720 plate) is measured at full resolution and can be
  asserted. A region-scoped `differs` (the same idea as `reveals`, which
  already restricts to an actor's bounds) remains a convenience for thresholds
  that should apply to one actor's region instead of the whole frame.
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

## Imported palettes and the embed's module path (2026-10-06) — worked around, still open

**Fixed in content, unfixed in the engine.** Every scene under `web/demos/`
imported its palette as `import "../lib/theme.amx"`, and in the browser that
specifier loses the import's `Colorscheme`: each such player logged

    warning unknown-colorscheme: Unknown colorscheme 'ink';
      using the default-dark built-in scheme instead.

so the figure rendered in `default-dark` — wrong background, text and stroke
tokens — while `animatix check` on disk said nothing, because the CLI resolves
the same import correctly from the real file path. Rewriting the specifier to
`"../../demos/lib/theme.amx"` (the same file, reached by going up to `web/` and
back down, which is what the tour and recipes scenes already wrote and why they
never warned) cleared **27 scenes down to 0**, with no change to what the pages
fetch.

The mechanism is still a bug, and the shape of it is this: `host.rs:29` builds
every document from `ENTRY_PATH = "main.amx"` — a bare name with no directory —
so a relative import that steps up from the entry has nothing to step up from,
and the module ends up registered under a key whose exports the colorscheme
lookup does not see. It is not section handling and not config placement (both
tested on `descent.amx` and refuted), and it is not a fetch failure: there is no
404, and `missing_imports` converges. The fix belongs in the module graph's
normalization of `..` against a directory-less entry, or in giving the embed the
scene's real path as its entry. Until then the working spelling is the only
safe one, and `site_scenes` now fails the build on any warning-or-worse
diagnostic, naming the scene and the page — the assertion that would have caught
this in the first place, verified by putting one scene's import back.

## Preview-lag round — still owed (2026-10-06)

The landed fixes and the three ideas the measurements killed are archived in
[`history/2026-10.md`](history/2026-10.md) ("The preview-lag round"). Two claims
still need the owner's hardware, because every number came off a software raster:

- An A/B of the arrival gate on real GPU: does scrolling the tour feel different,
  and does a figure you stop on reach `ready` within ~150 ms.
- One 28 s single main-thread stall appeared in the sandbox during a gated tour
  sweep (`longtask` 28070 ms, with 3052 frames at ≤ 20 ms either side). Not
  reproduced on the desktop or a second time in the browser; every candidate
  explanation is SwiftShader. Worth a look only if it appears on real hardware.

## Found by the epicycles rewrite, left open (2026-10-07)

Each of these was reproduced by rendering a minimal scene, and each is worked
around in `web/demos/epicycles/` rather than fixed in the engine.

- **`BarChart` inside a `Graph` does not inherit the host's coordinates.**
  `docs/primitives.md` ("Inside a Graph") shows exactly this usage; a minimal
  chart of four bars in a `x_domain: (0, 8), y_domain: (0, 1.18)` graph draws
  its bars hanging *below* the axis, with the k=1 bar reaching about a third of
  the height the axis says it should. `docs/primitives.md` also claims
  `BarChart` supports "`standalone` (pixel coordinates) and `Graph`-child (math
  coordinates) modes" — only the first one renders. `web/demos/epicycles/spectrum.amx`
  therefore stays standalone and loses the 1/k envelope curve that was the
  point of nesting it.
- **The `#Xs` stamp in front of a `play` is parsed, threaded through the
  builder, and thrown away.** `extract_play_stmt` returns
  `(target, transition, Some(current_time))` and `composition/build.rs:443`
  binds it as `_play_time`. Verified: a 10 s scene ending in `#8s play B
  [fade, 500ms]` starts B at 9.50 s, i.e. at `duration − transition`. `docs/spec.md`
  §18 documents the opposite ("The play's time comes from the keyframe marker
  that precedes it") for this sibling form, and no warning fires — only the
  *nested* form (`#4s { play Next }`) gets `play-inside-keyframe`. Either honour
  the stamp or reject it; content currently writes stamps that look like
  schedule and are decoration, which is how a 15 s act ended up with its
  outgoing cascade tuned to 14.4 s.
- **`fade-in` on a stroke-only actor animates `stroke_progress`, so it fights a
  per-frame write to the same property.** An `Ellipse` with `fill_opacity: 0`
  used as an angle arc, revealed by `fade-in` and driven by
  `arc.stroke_progress = …` in `always`, renders as a closed ring and reports
  `always-overrides-keyframes` even after the declaration drops its
  `stroke_progress`. The warning is right that something is overriding, and
  wrong that the `always` block wins. The arc was removed from
  `web/demos/epicycles/complexplane.amx`; a `reveals`-style entrance that never
  touches `stroke_progress`, or a dedicated `show` action, would let it come back.
- **Re-check the `graph.map()` half-scale note.** `web/demos/gradient/descent.amx:44-47`
  says "`graph.map()` runs at half the px/unit of the plotted curve — double the
  math coords to land the ball on the drawn stroke". A fresh probe (a
  `x_domain: (-2, 2), y_domain: (-1, 1)` graph, `size: (600, 300)`, `at: (640, 360)`)
  put `g.map(1.0, 0.5)` exactly on `(790, 285)`, which is the correct
  150 px/unit mapping. Either the comment describes a bug that has since been
  fixed, or it describes something specific to that scene's graph — and if the
  former, the doubling in `descent.amx` is now placing its ball one half-scale
  off. Needs a rendered check on that scene before either is changed.
- **`stroke_progress` overshoots by one tessellation segment, so a marker
  cannot sit on a plot's leading edge.** Measured on `web/demos/epicycles`
  §02: a `Graph` at `x_domain: (0, 4 pi)`, `size: (660, …)`, `at: (870, …)`
  driven by `stroke_progress = u` with `ease: linear`, against a ball placed at
  `graph.map(4 pi · u, …)`. At `u = 0.065` the ball is at screen x 583 and the
  ink's rightmost pixel at 591; at `u = 0.283`, 727 vs 736. The offset is
  ~1.0% of the domain at every sample — one segment of the adaptive
  tessellation — so it hides under a 14px ball where the curve is flat and
  pokes ~20px of ink out from under it on a steep descent (at `u = 0.283` the
  ball's centre is y 446, the ink's end y 466). Nothing in the property's
  contract says the trim is quantised, so content authors discover it as "the
  trace isn't synchronised with the pen". Fixing it means trimming to the
  requested fraction rather than to the next vertex — or exposing the trimmed
  endpoint so content can place a marker on where the ink actually stopped.
  `onecircle.amx` and `hero.amx` now sidestep it entirely by plotting a
  scrolling history (`sin(θ_now − x)`) with the marker pinned to the window's
  left edge, which has no leading edge to miss; a scene that genuinely wants a
  growing trace still cannot land a marker on its end.

## Property-name / flag / default inversions found by the resolution-knob audit (2026-10-08)

The `resolution` clamp was not a lone defect — it is one instance of a shape:
**a property whose descriptor name, registry binding, frame-time consumer and
documented default disagree.** Two sweeps over the registry found many more.
The fixed one is archived in [`history/2026-10.md`](history/2026-10.md); the rest are
recorded rather than silently half-patched, because several would change shipped visuals.

### Confirmed silent frame-time no-ops (accepted, no diagnostic, no consumer)

Each was checked by rendering and comparing frames. `always` writes to these
change no pixels:

`stroke_progress` (the reason §05's angle arc had to be deleted — see the
`stroke_progress` overshoot entry above), `dash_pattern`, `dash_offset` (so
"marching ants" are only reachable via a keyframe assignment, as
`web/demos/sorting/scene.amx` does), `blend`, `char_progress`, `commands`,
`points`, `head_size` on `Arrow`, `standoff` and `to_offset` on `Callout`,
`corner_radius` on `Glass`, `min_width`/`min_height`/`max_height`, `height`,
`anchor`/`offset`, `bounds` on `Filter`, the four `highlight_*` properties,
`ascent`/`descent`/`baseline`, the five Legend properties, `volume` on `Audio`,
and `background_color` on a non-`scene` actor.

Also `Callout.label` is half-dead: `always { c.label = "" }` hides the text,
but any replacement string keeps the old glyphs (`callout.rs` uses the value
only as an empty/non-empty gate). And a hex *string* colour is accepted by
`override_color()` for shapes but the text style path matches only
`Value::Color | Value::Vec4`, so `always { t.color = "#00ff00" }` does nothing
while the tuple form works.

### Divergent defaults — same property, two answers

The declaration extractor and the registry binding disagree on the starting
value, so the number the render uses and the number the inspector/interpolator
report differ for any scene that does not author the property explicitly:

| property | extractor | registry |
|---|---|---|
| `resolution` | 96 | 48 |
| `tolerance` | 0.5 | 2.0 |
| `max_depth` | 10 | 12 |
| `x_domain` / `y_domain` | ±10 | ±5 |
| `t_domain` | `[0, tau]` | `[0, 1]` |
| `stroke_width` | 2.0 (`default_stroke_width`) | 1.0 (binding) — and `ActorField::default_value` says 2.0 again |
| `font_size` | 48 for Text/Typst/Math | 48 / 36 / 24 per kind; docs say 18 |
| `bar_width` | 20 | 0 |
| `gap` | 4 (BarChart), 8 (Legend) | 0 |
| `cols` | 1 (`unwrap_or(1)` in layout) | 2 |
| `from` / `to` | `[-50,0]` / `[50,0]` | `[0,0]` / `[100,0]` |
| `size` (Graph parent fallback) | `[500,500]` | `[50,50]` half-size |

`docs/primitives.md` also documents `resolution` 64 for Heatmap against 96 for
curves, and `docs/spec.md` documents `place: "auto"` where both tables say
`"right"`. The base-grid *floors* disagree too: `resolution.max(2)` for Heatmap,
`.max(8)` for ContourSet, `.max(4)` and `.max(8)` elsewhere in the same function.

The structural fix is one default per property — the extractor should read the
registry's default rather than restating a literal, the way
`PROPERTY_DESCRIPTORS` already pins the name list. Not done here because every
row above is a live behaviour change and several would move shipped scenes.

## The unified pipeline — claims that still need a real run (2026-10-08)

The enforcement design is archived in `history/2026-10.md`. Five claims in the same
work are **not** yet evidence, and each needs a real run:

- **`release.yml` has never executed.** It triggers on a `vX.Y.Z` tag, and the repo
  has no semver tag yet — the first `cog bump --auto --skip-ci` is its first run.
  Unknowns it will answer: whether a `macos-latest`/`windows-latest` runner can
  build `eframe`/`wgpu` at all (nothing here has ever compiled those crates for a
  release), and whether `cog changelog --at <tag>` resolves the tag correctly from
  the publish job's `fetch-depth: 0` checkout.
- **The `web`-shell gates have not run locally.** `wasm-check` and `web-build` need
  `nix develop .#web-build`, and the nested-shell PATH trap makes a run from inside
  `.#default` meaningless. CI is the first honest execution of both.
- **cog's staging behaviour around `pre_bump_hooks` is assumed.**
  `scripts/bump-version.sh` was tested on its own (it moved all 15 members to
  `0.1.1` and refreshed `Cargo.lock`, then reverted), but not inside a bump commit.
  If cog does not stage the hook's edits, the tag lands on a commit whose manifests
  still say the old version — and `dist-package.sh` then refuses the build, which is
  the correct outcome but a failed release. Check with `cog bump --patch --dry-run`
  on a scratch branch first.
- **The seven existing `nightly-*` assets have never been checked for portability.**
  They are nix-built and carry `/nix/store` RPATHs, so they may only run on NixOS.
  `patchelf --print-rpath` / `ldd` against a downloaded asset before trusting any
  past release; the new `dist-linux` gate inherits that question.
- **`_site` was never diffed against a real Pages build.** `scripts/site-artifact.sh`
  runs the same four commands the old inline YAML did, and was tested against stub
  bundles (including the negative: a 100% size drift fails the gate), but a
  byte-for-byte comparison against a deployed artifact has not happened.

Also open by choice: `docs/scene_stats.md` is a design note with no roadmap row yet,
and `.zcodeignore` is an editor-generated mirror of `.gitignore` that should not be
tracked.

## Performance & Pipeline Optimization (Remaining Backlog)

### 1. Text System Compilation & Layout Performance Overhaul

- **Scope**: `crates/animatix-text`, `crates/animatix/src/primitives/text.rs`
- **Current behaviour**: Rich text and math formulas rely on Typst/FontDB compilation. In animations with spatial changes or nested inside moving containers, text shaping and layout can trigger redundant font parsing and BezPath tessellation on the CPU rendering thread.
- **Target**:
  1. **Shaped Glyph Run Memoization**: Decouple font shaping from per-frame evaluation; cache shaped glyph runs keyed by content and style signature `(ContentHash, FontSize, FontFamily, Weight, Style)`.
  2. **Static/Dynamic Transform Separation**: Avoid recompiling text runs when text geometry undergoes purely spatial transformations (translation, rotation, scale, opacity); transform pre-shaped BezPath / glyph buffers directly via local transform.
  3. **FontDB Multi-Family Fallback Path Optimization**: Memoize family lookup resolution and fallback candidate chains to avoid redundant FontDB queries on font switches.
  4. **Incremental Split-Text Optimization & AOT Baseline Masking**: Precompute static line clip rectangles at build time so kinetic typography baseline reveals avoid per-frame line boundary traversal.
  5. **Vello Hardware Glyph Run Pipeline**: Move from extracting CPU `kurbo::BezPath` outlines to submitting structured `GlyphRun` streams directly to Vello GPU instancing/atlas for non-morphing text.
  6. **Typst Math & Code Pre-warming**: Cache persistent Typst standard library/document contexts for inline math formulas, and bypass Typst for standalone syntect code block highlighting directly on the fast-path grid.
- **Verification**: Benchmark with `scripts/perf-bench.sh compare text_rebuild` and `scrub_text_scene_100frames`.

### 2. GPU-Accelerated Procedural Curves & Chart Strips

- **Scope**: `crates/animatix/src/timeline/plot/` & `crates/animatix-render/src/instanced_line.rs`
- **Current behaviour**: Mathematical curves (`ProceduralPlot`), parametric functions, and bar chart transitions currently resample hundreds of evaluation points and construct Kurbo/Vello vector paths on the CPU every frame, occupying the CPU rendering thread.
- **Target**: Offload parametric curve evaluation and line-strip generation to the `InstancedLinePipeline` on GPU shaders, generating anti-aliased geometry directly on the GPU without round-tripping through Kurbo CPU tessellation.
- **Verification**: Compare CPU frame time on dense curve scenes (`30_effects_catalog`, complex epicycles) before and after offload.

### 3. In-frame parallel root-subtree evaluation (deferred, trigger-gated)

- **Scope**: `scene_eval/frame.rs` root loop, `EvalCaches`.
- **Current behaviour**: The frame walk is single-threaded. Export is already
  frame-parallel (`render_pipeline` chunks frames across threads), so this
  only matters for interactive scrubbing latency and very large single scenes.
- **Trigger**: a real content scene whose evaluate-only CPU cost (no GPU
  readback — `stage_breakdown`, not `demos_frame_cost`) approaches a few
  milliseconds. At the 2026-10-26 measurements a 60-actor modifier scene
  evaluates in ~15 µs; the frame budget is nowhere near threatened.
- **Blocker**: `EvalCaches` and the per-entity memos are `RefCell`/`Cell`
  (`&self` interior mutability, not `Sync`), so a frame-parallel walk needs a
  thread-safety audit plus per-thread scratch caches. The static-subtree
  cache already proves the required vello pattern (render each subtree into a
  temp scene, append in authored order).

### 4. Slot-table frame environment for arbitrary values (deep IR change)

- **Scope**: `modifier_runtime/ir` lowering, `env/`, `property_engine`.
- **Current behaviour**: The frame env is a `HashMap<String, Value>` (FxHash
  since 2026-10-25) whose keys are written by name each frame and read back
  by name (`CompiledExpr::LoadEnv(String)`). The build-fixed parts of that
  work (injection plan, capacity, hashing) are already precomputed; the
  remaining cost is the name→value indirection itself for arbitrary dynamic
  `Value` reads.
- **Landed (2026-11-01)**: numeric modifier assignments and plot closures now
  use dense `FastScalarExpr::Slot(usize)` arrays through `FastModifierProgram`
  and the plot fast paths, covering the hot numeric/Vec2 case where slot-table
  indexing actually wins. `stage/modifier_exec` dropped 49.9% and
  `reactive_playback_100frames` 13.6% versus `ab3a526e`.
- **Remaining target**: resolve every free variable in general modifier IR to
  a `(entity, PropertyId)` slot at lower time; the frame env becomes a dense
  `Vec<Value>` sampled by index, with names surviving only at the GUI and
  diagnostics boundary. Plot-closure captures (let-scopes, bare param names)
  are the design edge that needs its own resolution scheme. Trigger: a real
  scene whose arbitrary `Value` property reads dominate `stage/eval_total`
  after the numeric fast paths (currently `stage/modifier_exec` is sub-µs).
- **Verification**: `stage/build_frame_env` and `vm_vs_ir` suites; the
  `is_animating` and closure parity tests are the behavior pins.


### 5. Frame-time text overrides do not invalidate the offline glyph cache

- **Scope**: `animatix-render` offscreen frame path / text glyph cache
  invalidation; surfaces in `animatix image` and `animatix storyboard`.
- **Current behaviour**: an `always { actor.text = ... }` (or any frame-time
  `.text` write) renders its DECLARED string in offline renders while
  frame-time geometry/position writes from the same block render correctly.
  Repro: `examples/animation/33_count_up_ticker.amx` shows "0%" at t=1.0;
  `web/demos/gradient/descent.amx` shows its declared loss at t=4.0. The live
  web player and GUI playback are believed correct (the count-up ticker is a
  documented recipe), so the defect is specific to the offscreen cache key not
  including the frame-time text value.
- **Workaround in force**: scenes declare the final/total string and rewrite it
  per-frame in `always` for live playback, so both paths read truthfully
  (web/DESIGN.md §5). Storyboards therefore never show a mid-count value.
- **Remaining target**: include the resolved frame-time text in the glyph cache
  key (or invalidate on text-property write) so offline renders agree with
  playback; then drop the dual-declare workaround from the house style.
- **Verification**: `animatix storyboard examples/animation/33_count_up_ticker.amx`
  shows a mid-count percentage in a middle tile; a new render-path test pins
  `image --time` against the live player's frame text.
