# Handoff — the motion-vocabulary round

> **STATUS: IN PROGRESS (2026-10-04, second session). M1 is complete** — every
> item in it is landed (see ["Landed so far"](#landed-so-far)). Of M2, items
> #14/#15/#16 landed and #12 is in flight; the remaining work is #8 (default
> easing) plus all of M3/M4 and the content/docs closeout, listed in
> ["Remaining work, next-session order"](#remaining-work-next-session-order).
> The decision questions at the bottom were resolved by the owner's go-ahead to
> execute the whole handoff ("做完handoff里的所有项目"); only implementation
> order remains open. This is a new track alongside the silent-drops remainder
> ([`handoff_silent_drops.md`](handoff_silent_drops.md) — WP6, WP7/B2, WP8 stay
> open and unaffected). Any property added here follows the WP6 discipline:
> **append** rows to `PROPERTY_DESCRIPTORS`, never reorder; both derivations
> follow; the build fails if they drift.
>
> Feasibility was verified against the code on 2026-10-04 (file:line where it is
> load-bearing). One live defect fell out of the probe round and was fixed
> first — see ["`draw-in` does not draw"](#found-while-probing-draw-in-does-not-draw-on-non-plot-shapes).

## Landed so far

### Batch 2 (2026-10-04, second session) — five commits, local only

| Commit | Item | Evidence it landed |
|---|---|---|
| `26665289` | #1 gradient fills/strokes, SVG ramp import, hex colors | `gradient_paints_are_parsed_stamped_and_interpolated`; three rewritten `svg_import` tests now assert the ramp survives import; rendered probe of all four forms in `dogfood/probe_gradient.amx` |
| `b4d8e0ac` | #7 + #9 `settle-in`, `pop-in`, `[anticipate: …]` | `settle_in_and_pop_in_ramp_scale_onto_the_authored_scale`, `anticipate_inserts_a_counter_move_before_the_travel` |
| `f69650d5` | #14 loop-perfect lint (`config { seamless_loop: true }` → `loop-not-seamless`) | `seamless_loop_lints_values_that_do_not_wrap` (both directions) |
| `adf0fcbc` | #15 CLI `--set NAME=VALUE` | `cli_defines_shadow_the_authored_let_defaults`; end-to-end render of a template with overridden text and color |
| `a05210b7` | #16 `#2b` beat stamps + `config { bpm }` | `beat_stamps_resolve_against_the_declared_tempo` (60/120/absent tempo) |

Notes the next session will want from batch 2:

- **Gradients are keyframable**, not static: two `linear()` values of the same
  kind and stop count interpolate geometry and colors. `gradient_extend:` and
  `gradient_space:` are separate scalar properties (default `pad` / `oklab`)
  applied by the stamp, so they are not per-ramp arguments.
- Ramps ride **outside** the shape-command memo, exactly like dash — an animated
  ramp would otherwise be served stale from a cached encoding.
- **Hex color strings now resolve anywhere a color is accepted**
  (`utils::color_from_text`). They previously fell through to the silent
  `[0.8, 0.8, 0.8, 1.0]` default, which is how the first gradient probe rendered
  gray. `lerp_color_oklab` also accepts color strings (frame-time `Value::Str`).
- **`pop-in`'s overshoot is an explicit intermediate keyframe**, not
  `Easing::Back`: the easing layer clamps a segment curve at its end, so a
  back-eased ramp peaks exactly at its target and never pops. Verified by test.
- **`anticipate:` is declared on the motion verbs' signatures**, not added to the
  universal `TIMING_KEYS` whitelist, so using it on another action reports
  `unsupported-modifier-key` instead of doing nothing silently.
- **`--set` shadows the authored `let`.** The first version re-applied the values
  *after* the statement walk and a `color: tint` fill stayed red — properties
  resolved during the walk never saw it. `process_body` now skips the authored
  assignment for a shadowed name.
- **`seamless_loop` is spelled that way** because `loop` is a reserved keyword.
  The lint is an engine post-build check (alongside `never-revealed`), so
  `check`, the GUI and the LSP all get it; only *keyframed* values are sampled —
  `always`-driven and plot-`func` values are frame-time functions and are the
  documented v1 skip.
- **Beat durations (`[2b]`) are not resolved** — only stamps. The modifier path
  has no scene tempo in scope; it reports `invalid-modifier-value` rather than
  guessing. Every non-build consumer (formatter, outline, analyzer, web marker
  strip, editor keyframe shift) resolves beats at the documented default 120 bpm.
- **Perf for #1** (`scripts/perf-bench.sh compare`, gradient-only tree, 120
  benches): `mixed_scene_evaluate` +5.03%, `reactive_evaluate_100frames` +13.55%,
  `stage__build_frame_env` +12.02% flagged — the same machine-drift families
  batch 1 flagged and cleared under isolation. `VelloPath` grew two
  `Option<GradientSpec>` fields, the plausible real cost. **Not dispositioned by
  an isolation re-run**; do that on a quiet machine before trusting it.
- **Docs debt partially paid**: `docs/properties.md` has the dash/blend rows plus
  a gradients section; `docs/spec.md` has the hex-color contract, the corrected
  "do not use hex strings" checklist line, the entrance-preset and anticipation
  prose, the `seamless_loop` and `bpm` config rows, a
  "Built-in Functions for Motion" table (noise family, `lerp_color_oklab`,
  `format`) and a Recipes section (count-up, seamless loop, light vocabulary).
  Still owed: `docs/effects.md` and `docs/primitives.md` for dash/blend/gradient,
  and the `vivid`/`paper`/`neon-night` scheme descriptions.
- **New examples**: `examples/animation/32_light_vocabulary.amx` (radial
  screen-blend wash, lit gradient bar, swept stroke, marching dashes, `fbm`
  drift, both new presets, anticipated move) and
  `examples/animation/33_count_up_ticker.amx`. Both render-verified. Note the
  trap found while writing them: an `always` block that writes a property the
  primitive *defaults* (e.g. `size` on a `Rect`) trips
  `always-overrides-keyframes` even though the author never animated it.

### Batch 1 (2026-10-04, first session)

Four commits on `main` (local only, unpushed as of 2026-10-04 evening):

| Commit | Item | Evidence it landed |
|---|---|---|
| `385f6e5d` | #6 `draw-in` really trims stroke-only shapes | `draw_in_trims_stroke_only_path_geometry_mid_draw`, `draw_in_cuts_a_straight_line_mid_segment` + 3 `path_progress` unit tests; trim assertions fail with the hook disabled (negative control). Hero probe at `--time 1.75` now draws the underline to the carriage (was full-width dim) |
| `6cb18912` | #4 noise family (`noise`/`noise2`/`seeded_noise`/`seeded_noise2`/`fbm`/`seeded_fbm`) + #10 `lerp_color_oklab` | `noise_family_and_oklab_lerp_agree_between_ir_and_ast` (IR/AST parity + value-level assertions incl. the red→green midpoint brightness property) |
| `049f9da2` | #3 `dash_pattern`/`dash_offset` + #2 per-actor `blend:` | `dash_pattern_stamps_paths_and_dash_offset_animates`, `blend_mode_is_bound_and_sampled`; visual probes: dashed/ants lines at two phases, `screen`-blend ellipse over a rect |
| `ef6a0c00` | #11 `vivid` / `paper` / `neon-night` colorschemes + GUI picker entries | colorscheme test battery passes; rendered probes for all three schemes look right |

### Implementation notes the next session will want

- **The property-addition checklist in practice** (dash + blend touched every
  one of these; use this as the canonical evidence chain): core descriptor row
  (append; pinned count `property_id_order_is_pinned` 97→99→100),
  `raw_property_types()` index-aligned row, `common_property_names()` if the
  row is `Applicable::Everything` (the sync test fails otherwise),
  `ValueType`/`ActorField` variants, `field→default` arm, the
  **name-sorted** `BINDINGS` insertion (`registry_is_sorted` fails if out of
  order — `blend` goes *after* `baseline`, first insertion was wrong),
  `Kind` mapping, `TrackFieldRef/Mut` arms + the three `name => Field` maps,
  `StyleTracks` fields, declaration seeding (`build/actor.rs` locals + prop
  arms + `insert_actor_keyframes`/`insert_end_keyframes` threading), and the
  plugin-ABI `PropertyValue` match in `extension_native_plugin.rs`.
- **DSL list literals use braces**, not brackets: `dash_pattern: {8, 6}` — a
  JSON-style `[8, 6]` is a parse error. Same shape as `points:`.
- **Dash rendering**: `VelloPath` grew `dash_pattern: Option<Vec<f32>>` /
  `dash_offset: f32`; the executor maps them onto kurbo `Stroke`. Dash rides
  *outside* the shape-command memo (an animated offset would be served stale
  from a cached encoding), stamped in `stamp_shape_dash`; a `draw-in` combines
  trim + fixed-length dashes. The static-subtree cache is safe because every
  dash-bearing actor has keyframes and is excluded there anyway.
- **Blend rendering**: node-level `push_layer` with an unbounded rect in
  `evaluate_node`, all 16 CSS mix-mode names parsed, unknown names warn and
  fall back to normal. The per-frame track probe is gated by
  `Timeline::blend_used` (refreshed at build end and in
  `invalidate_frame_cache`), so scenes that never author a blend pay one bool
  read. Known limits, worth revisiting: the layer is full-surface (not the
  node's bounds), `blend` on `Filter`/`Mask` scopes is untested, and there is
  no per-actor `Compose` control (vertical writing modes etc.).
- **F32List crossed the plugin ABI** as a plain `NATIVE_VALUE_LIST` of NUMs —
  no ABI bump needed; if plugins ever need to *author* dash patterns that path
  needs a proper type.
- **Perf dispositions for `049f9da2`** (hot-path rule): full-suite compare
  flagged `env_50`, `reactive_evaluate_100frames`, `text_rebuild` cold/warm;
  isolation re-runs of `env_*` and `reactive_evaluate` **PASS** (7.7–13.1%
  inside dynamic limits). `text_rebuild__mixed_48` was flagged at the same
  magnitude in the *first* full run of the session, before dash/blend touched
  anything text-shaped — machine-drift suspect (the suite's own history notes
  ±40% run-to-run swings on this box), and its warm arm reads 48.8% vs a 46%
  band. Not dispositioned by isolation; re-measure on a quiet machine before
  believing it.
- The `draw-in` fix changed `trim_path_by_progress` semantics from
  segment-count popping to arc-length cutting — plots benefit too (smoother
  curve trace-on). `docs/effects.md`/`docs/primitives.md` have not been
  updated for any of these four commits yet (docs closeout is item #5+#13).

## Why this round exists

The owner's brief: the demos and web pages are competent but *boring and dated*;
make it easier for users to produce lively, visually striking animation — by
adding language capability and/or adjusting defaults and design.

Three read-only surveys (web corpus, `examples/`+`dogfood/`, engine capability
inventory) plus an external study of two AI-animation skill repos converged on
one diagnosis: **the engine already "speaks" lively — the defaults and the corpus
don't use it, and the "light" vocabulary (gradients, blend modes, glow, noise,
camera) is supported by the pinned dependencies but not exposed.**

Evidence, in numbers:

- Web corpus: 652 entrances are `fade-in` + `expo-out`, 457 exits are
  `fade-out` + `ease-in`; `spring` appears 9 times and `bounce` twice across
  40+ scenes. Every loop ends with the same quiet-plate fade-out (the comment
  "Outro: quiet plate before the wrap" recurs verbatim in ~15 files).
- `examples/`+`dogfood/`: of 1,006 timed statements only 285 carry an ease —
  **72% animate linear**; `fade-in` is ~80% of all entrances; 76 of 87
  colorscheme declarations are the same two dark schemes; 2 `font_family`
  declarations in the entire corpus; zero gradients, shadows, glow, grain or
  texture in any `.amx`; the 13 shipped filter effects are used decoratively in
  ~6 files, nearly all of them scenes *about* the filter system.
- What the strong scenes prove (`gallery/epicycles.amx`, `web/demos/matrix`,
  `web/scenes/hero.amx`, `gallery/motion_poster.amx`): `always`-driven procedural
  motion, real `bounce` physics, morphs and stroke traces already produce
  landing-page-quality work. The gap is that nothing *default* points there.

## External reference points (2026-10-04 study)

Neither repo is an engine; both are **craft encoded as AI-agent skills**, which
is exactly what makes them useful to us — a demand list of named techniques and
numeric craft rules.

- **[iart-ai/motion-skills](https://github.com/iart-ai/motion-skills)** — 54
  skills / 17 packs (motion design, web animation, kinetic typography,
  explainer, data animation, maps, WebGL…), rendering via Remotion/manim/HTML,
  each skill carrying a deliver-and-verify loop.
- **[Unclecheng-li/AI-Animation-Skill](https://github.com/Unclecheng-li/AI-Animation-Skill)**
  — a Chinese workflow + 44 HTML templates for explainer decks (26 "Level2"
  templates organized by genre: VS-comparison, hierarchy, steps, code-case,
  warning; 14 flowchart templates). Its pain points are all "does not own the
  runtime" symptoms (cloneNode to replay CSS animations, "≥95% template
  similarity" rules) — our deterministic timeline is the structural advantage;
  the transferable idea is *templates organized by genre with a one-page SUMMARY*.

### The craft tables worth absorbing (from iart-ai `animation-principles` / `motion-background`)

- **Durations**: micro-interactions 100–200 ms; UI transitions 200–400 ms; hero/
  full-screen elements 400–800 ms; camera moves 800–2000 ms. Scale +30–50 % per
  doubling of distance/area ("a 20 px icon and an 800 px panel sharing a
  duration reads weightless") — the corpus's "everything is 500 ms" disease.
- **Easing by role**: enter ease-out `cubic-bezier(0.16,1,0.3,1)` (identical to
  the site CSS `--expo-out` — the taste exists, the language doesn't default to
  it); exit ease-in `(0.7,0,0.84,0)`; on-screen reposition ease-in-out
  `(0.65,0,0.35,1)`; playful overshoot `(0.34,1.56,0.64,1)`; **linear only for
  continuous loops**. "The eye forgives a slow start far less than a slow end."
- **Stagger**: lists 40–80 ms; dense grids 20–40 ms; cap a group reveal at
  ~600–800 ms total; direction follows the eye; ≤ ⅓ of elements in motion at
  once; no element travels > ⅓ of the screen without an intermediate keyframe.
- **The three principles that matter most for motion graphics**: anticipation
  (a 60–120 ms counter-move before the action), follow-through (attached
  elements settle 40–80 ms after the hero), arcs (straight-line travel "looks
  mechanical").
- **Anti-pattern diagnosis table** — reads like our corpus's chart: *stiff* →
  linear/symmetric enter easing; *floaty* → duration too long (cut 30 %);
  *cheap* → everything appears at once, or all elements share one duration;
  *mechanical despite easing* → missing anticipation/follow-through/arcs.
- **Loop craft**: `0%` and `100%` keyframes identical; motion built only from
  `sin`/`cos` of `(t % P)/P · 2τ` (and integer multiples) loops seamlessly; a
  `?t=N` freeze harness for deterministic screenshots.
- **Background recipes**: layered radial gradients with alpha colors over a dark
  base (the cheap one); value-noise + 5-octave fbm aurora mixing two colors via
  `smoothstep`; particle constellation (~80 points, distance-faded links). All
  three are product forms of the "light vocabulary" below.

## Found while probing: `draw-in` does not draw on non-plot shapes

**Fixed 2026-10-04 in `385f6e5d`** — the probe round also forced an upgrade:
the segment-count trim popped whole segments (a single-segment underline drew
all-or-nothing), so the trim is now arc-length aware and cuts inside a segment.
The section below is kept as the record of the defect.

**`draw-in` writes a `stroke_progress` track, but the only render-time consumer
of `stroke_progress` in the engine is the plot primitive.** A stroke-only
`Path` (the signature use) therefore *fades in at full width* — the draw is
fictional.

- Write side: `timeline/actions/reveal.rs:127` (`DrawIn` keys
  `style.stroke_progress` 0→1; the visible effect comes only from
  `lift_hidden_by_default`/`reveal_authored_zero_opacity`, i.e. an opacity fade).
- Read side: `primitives/plot.rs:91` is the only
  `track.style.stroke_progress.get(...)` in the tree; `primitives/path.rs:32-46`
  builds the VelloPath without progress; `primitives/mod.rs` mentions
  `stroke_progress` only in a doc comment (:1175). `trim_path_by_progress`
  (`timeline/path_progress.rs:14`) has exactly one caller.
- Probe (2026-10-04, `nix develop`, lavapipe):
  `cargo run -p animatix-cli -- image web/scenes/hero.amx --time 1.75 -o /tmp/hero_t175.png`
  (draw is 6.7 % in, expo-out ≈ 27 %): the underline renders **full-width and
  dim** — a fade, not a partial stroke. At `--time 1.95` it is full-width at
  full brightness. Re-run these two commands after the fix; the mid-draw frame
  must show a partial line.
- Consequences: the hero's carriage "rides the draw tip" over a tip that does
  not exist; `web/tour` teaches draw-in on plots (where it *does* work), so the
  tour hides the bug. **Fixing this is the prerequisite for the icon-set route**
  below, and it is user-visible on the flagship scene today.

Fix shape: consume `stroke_progress` in the shared shape render path (mirror
`plot.rs:91-99` via `trim_path_by_progress`), open strokes only — closed/filled
shapes must keep the fill-reveal semantics from `8e244595`.

## Item inventory and feasibility

Verdicts: TRIVIAL / SMALL / MEDIUM / LARGE, with the one or two things that make
each non-trivial. Verified 2026-10-04.

### M1 — light/color vocabulary + motion defaults

| # | Item | Verdict | Where the work is | Non-trivial part |
|---|---|---|---|---|
| 1 | Gradient fills/strokes (linear/radial/sweep) | MEDIUM | `RenderCommand::Paths` carries solid colors only (`primitives/mod.rs:968-1030`); peniko 0.6.1 at the pinned rev has full `GradientKind` support; SVG import flattens gradients to averaged solid (`svg_import.rs:63,115`) — un-flattening rides along | Animatable gradient params need a `ValueType` decision (ship static first); text paint excluded (the text pipeline degrades to solid RGBA) |
| 2 | Per-actor/scope `blend:` (16 peniko Mix modes) | ~~SMALL-MEDIUM~~ **DONE** `049f9da2` | see implementation notes | Node-level unbounded layer; scope-scoped blend and `Compose` control deferred |
| 3 | `dash_pattern` / `dash_offset` | ~~SMALL~~ **DONE** `049f9da2` | see implementation notes | Animated offset bypasses the command memo; list literal is brace form |
| 4 | `noise` / `fbm` / `seeded_noise` builtins | ~~TRIVIAL~~ **DONE** `6cb18912` | see implementation notes | Pure leaf functions; IR/AST parity pinned |
| 5 | Count-up tickers | **NOTHING TO BUILD** | `format("{}", …)` already exists (`eval_shared.rs:510-548`, `spec.md:938`) and `always { label.text = … }` recompiles glyphs per frame (`dispatch.rs:449-457`) | Deliverable is a recipe + example. Caveat to document: `geometry.size` is not remeasured from frame-time overrides, so a growing counter does not reflow its box |
| 6 | **Fix: `draw-in` trims non-plot shapes** | ~~SMALL-MEDIUM~~ **DONE** `385f6e5d` (plus the arc-length trim upgrade the first probe forced) | see implementation notes | Closed/filled shapes keep the `8e244595` fill-reveal semantics; hero re-probed |
| 7 | Preset actions + `anticipate:` / settle params | SMALL | New action = trait impl + `get_builtin_actions()` (`actions/mod.rs:367`) + syntax `ACTIONS` row (`builtins.rs:55-77`) + `action_documentation` (`:164-181`); a pinned test enforces runtime==syntax so drift fails the build | Pre-keyframes are established mechanics: `ensure_guard_keyframe` at `t-1` (`entrance.rs:59-62`), the plan-slot fence (`property_engine.rs:527-558`) — anticipation is the same move at `t-offset` (saturating at 0) |
| 8 | Default easing for un-eased statements | SMALL code, LARGE blast radius | the no-ease default site in timing parse/build | Requires the noise-floor discipline (same-binary + positive control; see `handoff_silent_drops.md` §Verification), corpus review, spec/golden updates |
| 9 | `settle` entrance (fade + slight scale) | SMALL | new action (preferred over changing `fade-in` semantics) | — |
| 10 | `lerp_color_oklab` | ~~TRIVIAL~~ **DONE** `6cb18912` (new name; `lerp_color` untouched; morph path deliberately not switched — decide separately if it should be) | — | — |
| 11 | New built-in colorschemes (incl. a vivid set) | ~~TRIVIAL data~~ **DONE** `ef6a0c00` (`vivid`/`paper`/`neon-night`) | — | GUI picker list updated in the same commit |

### M2 — make it visible (content, skin, verification tooling)

| # | Item | Verdict | Where the work is | Non-trivial part |
|---|---|---|---|---|
| 12 | Bundled stroke-icon set (Lucide-derived, ISC) | MEDIUM | **Cheapest route**: a name→`commands` data table expanding to `Path` actors — zero engine/primitive changes, inherits reveal once #6 lands. The new-primitive route costs a CATALOG row (`animatix-std/catalog.rs:188+`) + ShapeKind dispatch (`shapes/mod.rs:390`) + registry (`primitives/registry.rs:57`) | Blocked on #6 (otherwise icons fade, not draw). Subset size + attribution is a decision |
| 13 | Theme/vivid pack, display font, fast-path glyph gaps (ᵀ/ₖ tofu → ASCII math on the tour), site content redo, outro variety | content/web | `docs/ai_agent_animation_quality.md`-adjacent authoring work | Pure content, but the glyph gap is an `animatix-text` fix |
| 14 | Loop-perfect lint (`check` warns when a loop does not wrap) | SMALL | `animatix check` already builds the full Timeline (`main.rs:1383-1465`); scalar/style tracks + plan slots are sampleable at any t (`read_property_plan_slot`, `property_engine.rs:561-566`) | v1 policy: skip `always`/plot-`func`-driven values (document it); define the loop boundary (scene end vs `play` loop point). Shares the facts exporter with `ai_agent_animation_quality.md` |
| 15 | CLI `--set name=value` (template × data batching) | SMALL engine + plumbing | inject `env.set` at the pre-walk seam (`build/entry.rs:399-407`) | ~10 build entry points thread a defines map (or canonicalize one); the analyzer needs the defines or false `unresolved` fires |
| 16 | Beat timestamps `#2b` + `config { bpm }` | SMALL-MEDIUM | lexing site `token.rs:380-387`; **ordering is a non-problem** (config pre-pass precedes stamp resolution, `entry.rs:421-443`; scene configs hoisted, `composition/build.rs:238-240`) | a new `ast::Time::Beats` fans out to exhaustive matches (`walk.rs:317`, analyzer `duplicates.rs:232`, GUI `ast_utils.rs:271-275`, formatter); bpm threads to `time_to_ms` (`utils.rs:839`) via builder state; `config_keys.rs` is test-pinned against spec.md (`:128-163`) so docs move in lockstep |

### M3 — camera, text, particles

| # | Item | Verdict | Where the work is | Non-trivial part |
|---|---|---|---|---|
| 17 | Scene camera (pan/zoom/shake, keyframable) | MEDIUM | the transform threading exists; roots are seeded `Affine::IDENTITY` at exactly two sites (`scene_eval.rs:1931, :1968`), composing in `evaluate_node_transform` (`:96-160`) | Landmines: static-subtree cache key (`scene_eval.rs:1875-1905`), effect regions in world coords (`:1036`, blit translate `:1092-1098`), screen-anchored actors need an un-camerad parent transform (`:143`), `verify.rs` hit regions, background fill stays un-camerad (`:1841`) |
| 18 | Per-letter/word reveal (`draw-in [by: letter]`) | MEDIUM | `RenderCommand::Text` draws glyphs one by one already (`primitives/mod.rs:972-990`) but `TextPath` carries no per-glyph transform (`animatix-text/src/lib.rs:20-28`); `char_progress` truncation (`dispatch.rs:469-476`) and Code per-glyph recolor are the precedents | Per-frame glyph lists are cloned/mutated, not `Arc`ed (`translate_text_glyphs`, `:893-898`) — needs a perf look via `scripts/perf-bench.sh compare` |
| 19 | Particles (`burst` / `ambient`) | MEDIUM | **No state model needed**: with a seed, each particle's position/tint is an analytic function of `(seed_i, spawn, t)` — the same stateless philosophy as `always`, and `bounce` already pre-bakes physics into keyframes (`actions/effects.rs:216-290`) | **Decision 3**: primitive vs builtin-driven declarative actors; determinism must hold across platforms (no `rand()` unseeded) |
| 20 | Motion-along-path (`move [along: …]`) | MEDIUM (unverified) | building blocks: `trim_path_by_progress`, the `always` fallback exists today | Needs a scoping pass (orientation, arc-length parameterization) before committing |

### M4 — glow (the ABI bump) and the parked items

| # | Item | Verdict | Notes |
|---|---|---|---|
| 21 | Second-input-texture ABI bump → Bloom, soft DropShadow, chain Mix | MEDIUM-LARGE, already scoped | `docs/effects.md` §4.1-4.2: bind group gains an input binding so a pass can read the pre-chain original |
| 22 | `glass` / backdrop-blur | LARGE, **parked** | The main vello target is never an input texture (`offscreen.rs:267-294`; the filter backend renders its own sub-scenes, `filter_backend.rs:54-67`). Needs mid-frame scene splitting + rounded-rect regions (`EffectRegion` is a plain rect) + compositing *below* children. Revisit after #21 — shared machinery |
| 23 | BarChart race | MEDIUM-LARGE, parked | `data` is static and deliberately non-keyframeable (`build/plot.rs:969,1119-1134`); PlotCurve's `FuncTransition` side channel (`timeline/plot.rs:99-260`) is the pattern to copy; bar identity-matching is genuinely new logic |
| 24 | Variable-font weight animation | NOT feasible today | slim builds bundle Open Sans **Regular only** (`animatix-text/src/lib.rs:594-657`, `:200-201`); `font_weight` snaps Regular\|Bold (rich-text only). Needs a variable font or 9 static faces — parked |
| 25 | Vello pin lift | external gate | unchanged: upstream #1558, then the `vello_img_probe` matrix |

## Remaining work, next-session order

M1 (#1-#11) is complete. In M2, #14/#15/#16 are done and #12 (icon set) is in
flight. What is left, in the order that makes sense to attempt it:

1. **#8 default easing** — the last M2 item that changes what scenes look like.
   Needs the noise-floor A/B discipline (same-binary + positive control, per
   `handoff_silent_drops.md` §Verification), a corpus review, and spec/golden
   updates, on its own commit. Note that `settle-in`/`pop-in` already give
   authors a one-word way off the flat-fade default; #8 is about moving the
   default itself.
2. **#18 per-letter reveal** (`draw-in [by: letter]`) — `RenderCommand::Text`
   already draws glyphs one by one; `TextPath` carries no per-glyph transform,
   so this needs a glyph-list representation change plus a
   `scripts/perf-bench.sh compare` run.
3. **#17 scene camera** — the transform threading exists and roots are seeded
   `Affine::IDENTITY` at two sites; the cost is the landmine list in the
   inventory table (static-subtree cache key, effect regions in world coords,
   screen-anchored actors, `verify.rs` hit regions, un-camerad background).
4. **#19 particles** (seeded-analytic form, decision 3 approved) and
   **#20 move-along-path** (needs a scoping pass first).
5. **#21 the second-input ABI bump** → bloom/soft shadow; **#22 glass** after
   it (shared machinery); **#23 BarChart race**; **#24 font weights**; **#25
   vello pin lift** (check upstream state first).
6. **#13 closeout, partially done in batch 2.** Still owed: `docs/effects.md`
   and `docs/primitives.md` for dash/blend/gradient, the `vivid`/`paper`/
   `neon-night` scheme descriptions, the theme-pack examples, the fast-path
   glyph-gap fix (`ᵀ`/`ₖ` tofu on the tour → ASCII math), the site content redo
   with the new vocabulary, then pruning this handoff into `docs/history.md`
   when the round closes.
7. **The sub-agent visual review pass** the owner asked for: re-review every
   `web/` page's rendered output with the new vocabulary available, fix what
   it finds.

Milestones as originally proposed: M1 = items 1-11 (**complete**),
M2 = 12-16 (14/15/16 done, 12 in flight, 8 open), M3 = 17-20, M4 = 21-22.

## Relationship to other documents

- `docs/handoff_silent_drops.md` — still the open handoff for WP6/WP7/WP8; this
  round does not touch those items, but property additions here must follow the
  descriptor discipline that round documented.
- `docs/ai_agent_animation_quality.md` — the deterministic scene-facts evaluator
  proposal. The lints in M2 (loop-perfect, motion-vocabulary, the anti-pattern
  table) should be expressed as facts in *that* exporter's schema rather than as
  a second, parallel checker.
- `docs/handoff_web_redesign.md` — kept as the locked design-direction reference
  (palette, chrome spec, review pipeline); the M2 content/skin work builds on it.

## Decisions (resolved 2026-10-04, with the choices each implementation made)

The owner approved executing the whole handoff; the decisions below are
recorded with what was actually chosen so they are not re-litigated:

1. **Default easing** — still to do (#8); implement with the noise-floor A/B
   discipline when its turn comes.
2. **Color interpolation** — new `lerp_color_oklab` name (non-breaking);
   `lerp_color` untouched; the morph path's u8 sRGB lerp deliberately not
   switched yet.
3. **Particles** — seeded-analytic form approved in principle (#19 pending).
4. **Icons** — Lucide-derived data-table route (#12 pending, unblocked by the
   #6 fix).
5. **Beat syntax** — approved (#16 pending).
6. **`--set` semantics** — top-level `let` overrides first (#15 pending).
7. **Loop lint v1** — skip `always`/plot-func-driven values, document the skip
   (#14 pending).
8. **Presets** — all three layers: engine verbs, `examples/lib` genre packs,
   site recipes gallery.
9. **Sequencing** — this round runs after the silent-drops gates each session;
   both tracks stay unpushed per the standing rule until the owner says push.

## Gates

Standard `AGENTS.md` gates (fmt, check/clippy `--workspace --all-targets`,
syntax tests, serial lib tests) inside `nix develop`; `cog commit`; no push
without the owner. Round-specific additions:

- Anything that changes what every scene renders (default easing, OKLab,
  camera, draw-in fix) needs **same-binary + positive controls** per the
  noise-floor rules in `handoff_silent_drops.md` §Verification — no corpus pixel
  claims without them.
- Per-frame-path items (per-letter reveal, particles, dash/trim) need
  `scripts/perf-bench.sh compare` numbers in the commit message.
- The `draw-in` fix re-runs the hero probe (`--time 1.75` must show a partial
  stroke, not a dim full-width line).
