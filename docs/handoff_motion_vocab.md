# Handoff — the motion-vocabulary round

> **STATUS: IN PROGRESS (2026-10-04, fifth session).** M1 (#1-#11), M2 (#12-#16)
> and M3 (#17-#20) are complete, and M4's data half is too: #21 (the
> second-input-texture ABI bump, `Bloom`, soft `DropShadow`) and #23 (the BarChart
> race) have both landed. So have the site's recipes gallery, the tour's new
> "Light & camera" section, the authored-bounds camera gap, a delayed
> `camera.zoom` bug the new tests surfaced, and the
> `always-overrides-keyframes` false positive. What remains is #22 (glass), #24
> (variable weights — three changes, not one; see its row), #25 (the vello pin —
> a local probe after a rev bump, not an upstream gate), the camera's two real
> follow-ups, an open +10.7% on one analyzer bench, and the #13 content leftovers.
> See
> ["Landed so far"](#landed-so-far) and
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

### Batch 5 (2026-10-04, fifth session) — eleven commits, local only

| Commit | Item | Evidence it landed |
|---|---|---|
| `889c6149` | A delayed `camera.zoom` no longer zeroes the scene until its stamp | `a_delayed_camera_write_holds_its_identity_until_its_stamp` (IDENTITY at t=0 and t=999, the authored 2× at t=1500); the *general* fix is recorded in the message as not free — seeding `max_height`'s `INFINITY` identity makes the 1 ms preserve segment interpolate to `NaN`, which `test_write_read_roundtrip_max_height` catches |
| `b329b19c` | **Authored `Filter` bounds follow the camera** (the gap batch 4 measured and left open) | `dogfood/probe_camera_scopes.amx`: authored vs the same scene with `bounds:` deleted went 73,512 differing pixels at t=1.6 → **0**, with the 9-pixel padding floor at t=0.2 unchanged; `filter_bounds_follow_the_camera` covers the mapping; the full 120-bench compare is quoted in the message |
| `a8647000` | The analyzer's exemption vocabulary is built once per process, not per keystroke | 111 + 2 analyzer tests unchanged; measured **neutral** on the bench (129.4 → 129.2 µs) and kept on that basis, not as a perf claim |
| `6d4b08c5` | BarChart builder split into `BarChartLayout` + `paths_for`, the precondition for animating `data` | Pixel-identical renders of two bar-chart scenes before and after, plus the existing fifteen `bar_chart` tests |
| this commit | **#23 the BarChart race** — `data = {…}` keyframes, bars matched by label | `a_data_assignment_records_a_transition`, `bar_data_interpolation_matches_by_label`, `bar_data_at_walks_the_transition_list`, `bar_geometry_follows_the_data_transition` (bbox heights at t=0 / mid / after); `examples/data/27_bars_race.amx`; the assignment that used to be an `unsupported-assignment-property` error now checks clean |
| `aee62252` | **`always-overrides-keyframes` no longer reads a declaration seed as animation** (item 4) | `is_property_animated` asks for ≥2 keyframes with differing values; three new source-level tests pin seeded-`dash_offset` silent, constant-declared-`size` silent, real `#1s b.size = (140, 80) [1s]` still warning; `check examples/animation/36_light_pack.amx` no longer mentions `dash_offset` |
| `39bbf79a` | The loop lint samples the camera axes, and a camera-only scene stops measuring zero length | `seamless_loop_lints_camera_axes`; `check` on a camera-only loop scene now names `camera.zoom`, and the same scene with the push returned to 1.0 stays silent |
| `757b5b70` | Docs closeout: the six-scheme table, `architecture.md`'s stale scheme list, `roadmap.md`'s effects section, the plugin ABI's sixth binding | Swept for numbers and lists of vocabulary the tests cannot see; the roadmap now says what landed and why a chain `Mix` is deliberately not on the list |
| `1105234a` | The handoff's claim that `animatix verify` checks ink in scene space was wrong | Settled from the code and from `b329b19c`'s 0-pixel agreement, not a new render |
| `f65b497c` | The tour now teaches the round: new §06 "Light & camera", and §05's effects scene gets its bloom | Both scenes `check`-clean; four frames of the new scene rendered and measured (15% → 29% → 32% → 51% content as the layers land); the bloom beat moves the sphere's two unsaturated channels (255,214,13) → (255,255,28); `never-revealed` caught the first draft's invisible bulb |
| this commit | Docs closeout: six-scheme table, `architecture.md`'s stale scheme list, `roadmap.md`'s effects section, the plugin ABI's binding note, and #24/#25 rescoped | See the notes below |

Notes the next session will want from batch 5:

- **`analyzer_update/small` is a real, unattributed +10.7%.** It has now
  reproduced across four runs against the 2026-10-02 baseline with a *tight*
  within-run spread (129.2 µs ±1.4% vs a 116.8 µs baseline), while
  `analyzer_update/dogfood` and `/large` — same code path, bigger inputs — read
  11-15% *faster*. So it is a fixed-cost effect on the smallest input, not noise,
  and not machine drift. Two candidates were measured out: the per-update
  exemption set (`a8647000` made it static; the number moved 0.2%) and
  `raw_property_types()` (it sits inside a `get_or_init`, so it was never
  per-update). The next step is a profiler on `Analyzer::update` for
  `examples/basics/00_hello.amx`; no profiler was available in this environment
  (`perf`/`valgrind` both absent).
- **The guard's own noise bound on that bench is useless.** Across runs of the
  same tree its limit has ranged 5.0% → 84.8%, because the bound is built from
  whichever `std_dev` that run happened to measure. Treat a flag there as a
  question ("re-run isolated and look at the spread"), never as a verdict.
- **#24 was mis-scoped and #25 was mis-blamed.** Variable-font weights need three
  changes, not an asset swap (see the inventory row); the vello pin's gate is not
  upstream's issue tracker but our own `vello_img_probe`, which needs a rev bump
  and a quiet machine. Both rows now say that.
- **Docs drift is wider than the code you changed.** This round's effect additions
  left stale counts in four places the tests cannot see: the homepage's "13 GPU
  effects"/"19 built-in actions", the tour's "Thirteen ship built in",
  `architecture.md`'s three-scheme list, and `roadmap.md` still describing the ABI
  bump as blocked. `docs/extension_authoring.md` also still told plugin authors the
  binding layout had five entries. Sweep the site and docs for *numbers and lists
  of vocabulary* after any addition — `grep` for the old count is the whole method.

### Batch 4 (2026-10-04, fourth session) — seven commits, local only

| Commit | Item | Evidence it landed |
|---|---|---|
| `b27ca5e5` | **#21 ABI v2** — effect passes get the pre-chain original at `binding: 5` — plus `Bloom` as its first consumer | Built the CLI with and without the bump (`strings` confirms only the new binary carries the `Animatix Filter Original` texture label) and rendered nine filter-bearing frames: every PNG byte-identical, and the 35 pixel-asserting GPU tests in `filter_backend::tests` pass against the v2 layout unchanged. `dogfood/probe_bloom.amx` measures the payoff: the bulb goes (167,142,15) → (255,255,24) and 32 px away the plate lifts (11,16,23) → (21,30,44) |
| `a046cbf6` | `DropShadow.softness` — sixteen golden-angle taps over a disc, hard case kept structurally | `examples/animation/30_effects_catalog.amx` md5-identical across the change (`53af058e0f0013f7ee09f427215693de`); `drop_shadow_softness_spreads_the_silhouette` reads alpha 0 at x=12 with `softness: 0` and 20..200 with `softness: 6` |
| `455f403b` | Eleven builtins the engine answered and the language never declared, plus `LIST_FUNCTIONS` | `git show HEAD~:crates/animatix-syntax/src/builtins.rs` matches none of the thirteen names; `check` on a scene calling all thirteen now reports no `unresolved-variable` at all |
| `e7fa1a1b` | `docs/spec.md`'s built-in math line rewritten to match the declaration table | The docs listed a subset, which is how the undeclared names stayed unnoticed |
| `7d568e64` | The site's recipes gallery — decision 8's third preset layer | `web/recipes/index.html` + seven scenes, all `check`-clean and all rendered at t=2.6 s with content measured (0.97% of frame for a dashed rule, 43.6% for the bloom stage) |
| `d4bfdd25` | `docs/spec.md`'s `format` example used a placeholder form the engine never had | Rendered proof of the grammar: `named {x}` stays literal, `plain {}` yields `12.345`, `{:.0}` yields `12`, `{:.2}` yields `12.35`, `{:,}` stays literal. A grep for the class over `examples/`, `web/`, `docs/`, `dogfood/` found no other use |
| `22b3632f` | The batch-4 record, plus the camera/authored-bounds limit this batch then fixed | The probe's header carries the 9 px / 73,512 px measurement that `b329b19c` closed |

Notes the next session will want from batch 4:

- **The perf guard cannot see the GPU filter path.** Every bench in the suite
  times parse, build and scene evaluation; `render_scene_to_image_gpu_filtered`
  is not in any of them. So an ABI change that adds a whole-region copy per scope
  per frame is invisible to `scripts/perf-bench.sh compare`, and the honest
  statement in that commit is read off the shader. If the filter path ever needs
  a real guard, that bench has to be written first.
- **Two analyzer flags were measurement contention, not regressions.** The full
  120-bench run flagged `analyzer_diagnostics/dogfood` (+7.10%) and
  `analyzer_update/small` (+15.64%); re-run with the machine to themselves the
  family reads 9 compared, 0 regressions (+2.82% and +17.89% against a 44.48%
  bound). Two lessons: never trust a flag from a run that overlapped a build or a
  render, and `analyzer_update/small` measures a 1 KB file so its own noise bound
  has ranged 44-85% — it is a weak guard, not a sensitive one.
- **`EffectParams.values` is index-aligned with `params()`, not name-keyed.**
  Inserting a parameter in the middle of a declared list shifts every later
  `values.get(n)` in `pack`, and `sample_params` hides that for authored scenes
  while a hand-built chain (a test) breaks loudly. The next parameter added to an
  existing effect should be appended, or every `get(n)` in that file re-checked.
- **`format` has two placeholder forms and the spec used a third.** Measured by
  rendering: `{}` substitutes, `{.N}` substitutes at N decimals, and everything
  else — `{x}`, `{:.1f}`, `{:,}` — is emitted *literally* (deliberately, so an
  unsupported spec shows instead of guessing). `docs/spec.md`'s reactive-system
  example was `format("y = {x}", x)`, which has never worked; it now reads
  `{:.2}` and the grammar is written down where a reader will hit it. Worth a
  grep after any doc round: `format("[^"]*{[a-zA-Z_.]` finds the class in one
  pass, and it found nothing else in the corpus.
- **Premultiplication is not a detail in this backend.** The first `Bloom`
  composite raised colour while freezing alpha at the original's, which fringes
  at a scope's semi-transparent edge and caps a glow's coverage at what was
  already there; the shipped version grows `out_alpha` with the glow and clamps
  colour to it. Same trap as `DropShadow`: keep `rgb <= a`.

### Batch 3 (2026-10-04, third session) — four commits, local only

| Commit | Item | Evidence it landed |
|---|---|---|
| `44bdba93` | The review pass's three engine gaps: plot decorations, `always`-block `size` doubling, `always`-block color text | `plot_actor_declarations_seed_dash_and_gradient`, `size_override_is_authored_in_full_extents`, `text_color_override_reaches_shape_style`; rendered probes measured to the pixel — `always { l.stroke = "#30d158" }` now paints (48,209,88), `always { r.size = (100.0, 60.0) }` now paints 100×60, and a `PlotCurve` with `stroke_gradient` shows the ramp it was dropping |
| `a0960f98` | **#17 scene camera** — `camera.at` / `camera.zoom` / `camera.rotation` | Eight tests in `timeline/tests/camera.rs` (identity when untouched, zoom about the center, pan-after-zoom, interpolation across a timed write, `always`-driven engagement, both diagnostic paths, the reserved label); `examples/animation/35_camera_moves.amx` verified by pixel position at t=2.6/4.6/9.0 |
| `6c40c243` | The `web/` visual review pass — 11 scenes re-lit | Rendered and read back: hero at t=4.6s, the tour's word reveal at t=1.3s, the `along` route at t=2.2s |
| `1727d558` | The reactive-frame cost batch 2 flagged and never dispositioned | `reactive_evaluate_100frames` +16.7% → **+5.0%** in isolation; `vello_path_stays_small_enough_to_clone_per_frame` pins the struct size |
| `9f344cdb` | The type table catching up with hex color strings, and two missing analyzer exemptions | `check` is silent on `color: "#ffe9c7"` in `34_particles_analytic.amx`, which it used to warn `type-mismatch` about |
| `2e170adf` | Decision 8's second preset layer — the `examples/lib` genre pack | `examples/lib/light.amx` (`KeyLight`, `CoolLight`, `MarchingRail`, `Ticker`, `Breather`) and `examples/animation/36_light_pack.amx`, rendered at t=2.0s: warm key behind the card, cool fill low-right, the ticker reading its 1284× target, the rail dashed |
| `e7d0b3ac` | Three scenes re-tuned after the `size` fix | The particles example reads as sparks again; `04_motion`'s orbit is centred on the plate rather than the origin (`scene.center.x` is an anchor, not an expression value); the tour's reticle breathes on `size` again |

Notes the next session will want from batch 3:

- **The three gaps were all the same bug shape**: the language declares a
  property as applicable to an actor, and one of the three write paths
  (declaration, keyframe assignment, frame-time override) or one of the two
  read paths (the generic shape renderer, the plot primitive) did not honor
  it. When adding vocabulary, the check to run is not "does my probe scene
  look right" but "does every actor × every write path × every read path
  carry it" — `animatix check` reported nothing for all three.
- **`geometry.size` is half-extents everywhere.** Declarations halve in
  `build/shape.rs`, keyframe assignments in `handle_size_assignment`, and now
  frame overrides in `primitives::override_size`. A new shape that reads
  `geometry.size` must treat it as half, and a new write path must halve.
- **`inject_property_into_env` is the hot path nobody watches.** It walks every
  INJECTABLE registry row for every actor every frame an env is built, and each
  row costs a read, an env write and an `animating_flag` key allocation. Adding
  a property with `F::ASSIGNABLE_AI` therefore costs real frame time on scenes
  that never use it — the reason six of batch 1/2's rows are now
  `ASSIGNABLE_A`. `dash_offset` stays injectable on purpose.
- **Camera semantics, in one line each:** pan is applied *after* zoom (so
  centring a station 400 px off-center at 1.7× takes 680), scene-anchored
  actors move with it, the background does not, and authoring a camera
  bypasses the static-subtree cache.
- **Eleven builtins the engine implemented and the language never declared.**
  Diffing `eval_shared`'s dispatch against `animatix-syntax::builtins` turned up
  `atan2, fract, hypot, ln, pow, rem, round, signum, step, deg_to_rad,
  rad_to_deg` absent from `MATH_FUNCTIONS`, and `list_set`/`list_swap` absent
  from every list. Those names resolve at runtime — `check` on a scene using
  them is silent — but the analyzer's `unresolved-variable` pass exempts only
  what `MATH_FUNCTIONS` declares, so the editor flags working, documented code
  (`ln(x)` and `list_swap(…)` are both in `docs/spec.md`) and completion never
  offers them. All eleven are declared now, `list_set`/`list_swap` in a new
  `LIST_FUNCTIONS` because they return a list and typing them `Num` would break
  the assignment the docs show. The diff is worth re-running after any builtin
  is added; nothing else catches it.
- **Tooling drifts behind a vocabulary addition in three places, not one.** When
  batch 2 made color strings resolve everywhere, `raw_property_types()` still
  said `Color` only, so `check` and the editor warned on source that rendered
  correctly (`9f344cdb`). The property-addition checklist covers the parser and
  the engine; the *type* row is where a widened value grammar has to follow, and
  nothing tests that a warning-free scene stays warning-free.
- **Still open from batch 2's notes**: `docs/effects.md` / `docs/primitives.md`
  now do cover dash/blend/gradient (done in `f00d3770`/`fc4dc879`). The
  `web/demos/posters/*.png` turned out not to need regenerating — nothing
  references them (the hub plays live embeds), so `web/README.md` was wrong
  rather than the images being stale.

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
| 8 | Default easing for un-eased statements | ~~SMALL code, LARGE blast radius~~ **DONE** `90285df1` + `ef3b400b` | the role-based default now lives in `default_easing_for` (`timeline/timing.rs`) | Landed narrower than proposed: entrances get `expo-out`, exits `ease-in`, reposition `ease-in-out`, oscillators stay linear — and **assignments stay linear**, because an action expands into assignments that inherit its timing modifiers and would double-ease a pre-baked curve (`bounce` regressed to scale 1.4375 against a 1.375 baseline). Corpus A/B run with two builds and a positive control |
| 9 | `settle` entrance (fade + slight scale) | SMALL | new action (preferred over changing `fade-in` semantics) | — |
| 10 | `lerp_color_oklab` | ~~TRIVIAL~~ **DONE** `6cb18912` (new name; `lerp_color` untouched; morph path deliberately not switched — decide separately if it should be) | — | — |
| 11 | New built-in colorschemes (incl. a vivid set) | ~~TRIVIAL data~~ **DONE** `ef6a0c00` (`vivid`/`paper`/`neon-night`) | — | GUI picker list updated in the same commit |

### M2 — make it visible (content, skin, verification tooling)

| # | Item | Verdict | Where the work is | Non-trivial part |
|---|---|---|---|---|
| 12 | Bundled stroke-icon set (Lucide-derived, ISC) | ~~MEDIUM~~ **DONE** `719d0c77` | the data-table route: `animatix-core::stroke_icons` (25 Lucide ISC paths) + an `icon:` property that expands into `Path` geometry | Landed with two real parser fixes in `svg_import::parse_svg_path_data` (compact numbers `7-7`/`.53.53`; implicit `line_to` after `M`/`m`) and the authored-`scale:` fix that makes an icon sizeable at all |
| 13 | Theme/vivid pack, display font, fast-path glyph gaps (ᵀ/ₖ tofu → ASCII math on the tour), site content redo, outro variety | content/web — **mostly landed**: schemes `ef6a0c00`, genre pack `2e170adf`, glyph gaps already closed by the `missing-glyph` warning + ASCII formulae | `docs/ai_agent_animation_quality.md`-adjacent authoring work | Pure content, but the glyph gap is an `animatix-text` fix |
| 14 | Loop-perfect lint (`check` warns when a loop does not wrap) | SMALL | `animatix check` already builds the full Timeline (`main.rs:1383-1465`); scalar/style tracks + plan slots are sampleable at any t (`read_property_plan_slot`, `property_engine.rs:561-566`) | v1 policy: skip `always`/plot-`func`-driven values (document it); define the loop boundary (scene end vs `play` loop point). Shares the facts exporter with `ai_agent_animation_quality.md` |
| 15 | CLI `--set name=value` (template × data batching) | SMALL engine + plumbing | inject `env.set` at the pre-walk seam (`build/entry.rs:399-407`) | ~10 build entry points thread a defines map (or canonicalize one); the analyzer needs the defines or false `unresolved` fires |
| 16 | Beat timestamps `#2b` + `config { bpm }` | SMALL-MEDIUM | lexing site `token.rs:380-387`; **ordering is a non-problem** (config pre-pass precedes stamp resolution, `entry.rs:421-443`; scene configs hoisted, `composition/build.rs:238-240`) | a new `ast::Time::Beats` fans out to exhaustive matches (`walk.rs:317`, analyzer `duplicates.rs:232`, GUI `ast_utils.rs:271-275`, formatter); bpm threads to `time_to_ms` (`utils.rs:839`) via builder state; `config_keys.rs` is test-pinned against spec.md (`:128-163`) so docs move in lockstep |

### M3 — camera, text, particles

| # | Item | Verdict | Where the work is | Non-trivial part |
|---|---|---|---|---|
| 17 | Scene camera (pan/zoom/shake, keyframable) | ~~MEDIUM~~ **DONE** (batch 3) | `timeline/camera.rs`: `camera.at` / `camera.zoom` / `camera.rotation`, routed in `assignments` before target resolution and applied as the parent transform of every root node | Landmines resolved as: the static-subtree cache is bypassed while `camera_used` (a cached encoding cannot be re-transformed on append), the background fill deliberately stays un-camerad, hit regions come out in screen space for free because the camera is in the node transform, and **scene-anchored actors do move** — documented as a limit, with no per-actor opt-out yet |
| 18 | Per-letter/word reveal (`draw-in [by: letter]`) | ~~MEDIUM~~ **DONE** at word granularity `bf820fef` | `draw-in [by: word]` reveals a text actor word by word through the existing per-`TextPath` command list | Per-*letter* still needs the `TextPath` per-glyph transform this row named (`animatix-text/src/lib.rs:20-28`); `by: letter` is deliberately rejected rather than silently doing the word thing |
| 19 | Particles (`burst` / `ambient`) | ~~MEDIUM~~ **DONE** `fdbdc5c0` | decision 3 went the analytic way: `examples/animation/34_particles_analytic.amx` is seeded `always` math (`seeded_noise(i, …)` for direction and speed, gravity as `age²`) — no new primitive, no state model | Determinism holds because there is no unseeded `rand()`. Follow-up this exposed: a `range(n)` builtin, so the index list is generated instead of spelled out |
| 20 | Motion-along-path (`move [along: …]`) | ~~MEDIUM (unverified)~~ **DONE** `00441684` | `run_move_along` samples the route with `trim_path_by_progress` (`sample_path_along`) and keys ~48 positions at one per 40 ms | Scoping-pass result: arc-length parameterisation was already there (from the #6 trim upgrade); `orient: true` turns the actor along the local tangent, and the sampler must be seeded from the first `MoveTo` because `trim_path_by_progress(path, 0.0)` yields an empty path with no current position |

### M4 — glow (the ABI bump) and the parked items

| # | Item | Verdict | Notes |
|---|---|---|---|
| 21 | Second-input-texture ABI bump → Bloom, soft DropShadow, chain Mix | **DONE** `b27ca5e5` + `a046cbf6` | ABI v2 binds the pre-chain original at `binding: 5` (`filter_backend.rs`), copied once per scope before pass 0. `Bloom` is its first consumer (`animatix-std/src/effects/bloom.rs`) and `DropShadow.softness` the second. "chain Mix" needed no new effect: `Bloom`'s `keep` parameter *is* a linear mix of the chain result with the original, so a second effect over the same math would be a duplicate |
| 22 | `glass` / backdrop-blur | LARGE, **parked, but now decision-ready** | Re-scoped in batch 6 against the pipeline rather than the summary. ABI v2 does **not** help: binding 5 is the *scope's own* pre-chain sub-scene, and glass needs the main target's pixels *below* the scope in z, which nothing in the frame ever exposes as a texture (`offscreen.rs:267-294` renders the whole scene in one pass; `filter_backend.rs:54-67` renders only sub-scenes into the backend's own targets). And the obvious shortcut — render the frame, sample the region, blit the above-glass content afterwards — is wrong for the common case, because a label sitting *on* a glass card would be swallowed by the composite. The shape that is correct: evaluate once, emit **two** vello scenes pivoted at the glass scope, render below→texA, copy texA's region as the chain's second input, render above→texB with a transparent `base_color`, blit texA then the glass result then texB. Two hard facts to design around: (a) `RendererCore` only ever hands vello a `RenderParams { base_color, .. }` (`core.rs:151-170`), i.e. every render clears its target, so "draw a second scene over the first in the same texture" does not exist here — the split must go to separate textures and be composited; (b) `EffectRegion` is a plain rect, so a rounded glass panel needs either a mask pass or rounded-region support. Cost: N glass scopes = N+1 vello renders per frame, and **the filter path has no bench guard at all** (batch 4's note — every bench in the suite stops at scene evaluation), so that guard has to be written before this lands. Still also owed: compositing *below* children, and a decision on whether glass is one effect or a `Filter` variant |
| 23 | BarChart race | ~~MEDIUM-LARGE~~ **DONE** (batch 5) | Landed along the five steps as scoped — `BarDataTransition` beside `FuncTransition` (`timeline/plot.rs`), label matching in `interpolate_bar_data`, the frame-time sampler `bar_data_at` called from `evaluate_node`, and the layout kept on the track so a rebuild never re-parses properties — with **two corrections to the plan**. Step 4's memo is unnecessary: the rebuild is gated on `bar_data_transitions` being non-empty, so a chart that never animates its data pays one `is_empty()` per frame. And step 5 was a no-op — the `data` entry in `build/plot.rs:1160`'s skip list is the *declaration* walk, not the assignment path; removing it would have broken the builder. The assignment hooks in through `Primitive::handle_assignment`, the extension point that already existed for exactly this (eight primitives use it). Two documented limits: a changed label set **or order** warns, because captions are compiled at build into the declaration's slots, and `max_value: auto` normalises the tallest bar every frame, which hides the race unless the author pins it. A third branch the plan assumed turned out to be **dead**: `build/plot.rs` resolves the layout for every `BarChart` whether or not `data:` was declared, so assigning without a declaration is not an error — it is an empty `from`, bars entering from 0, with its own warning that there were no captions to compile. And it is the render, not the test suite, that proves label matching: bar tops read back from three frames of `examples/data/27_bars_race.amx` land on the interpolated values, with `api` overtaking `web` inside the first window and losing it inside the second.
| 24 | Variable-font weight animation | NOT feasible today — **three changes, not one** | Scoped in batch 5 against the code rather than the summary: (1) the bundle ships four *static* Open Sans faces (`animatix-text/src/lib.rs:600-646` — Regular/Bold/Italic/BoldItalic) plus Noto Sans SC and Fira Math, so a variable face has to be added (asset + licence); (2) `font_weight_to_typst` (`:960`) quantizes the numeric axis to nine *named* CSS weights as a `&'static str`, and typst then picks a face by name — six of those nine names have no face in the bundle and fall back, so the path needs typst's numeric `("family", weight: 640)` form rather than a keyword; (3) weight changes recompile glyphs every frame, which is the `count_up` cost path, so it needs the same frame-time memo question answered. Parked |
| 25 | Vello pin lift | **locally answerable as of batch 6** | Batch 3 and batch 5 both called this an external gate because `gh` is absent, `WebFetch` is quota-blocked (`FORBIDDEN`, not a network fault) and a web search returns no status. Neither matters: the probe *is* the gate, and the revision to probe for is now known. `git ls-remote` (2026-10-04) puts upstream `main` at **`f3000c8d`**, and this box's cargo git cache (`~/.cargo/git/db/vello-*`) tops out at `17166312` "Update wgpu badge", whose parent is `c55a2b5e` "vello: Keep image atlas residency across renders (#1558)" — the change the pin avoids, sitting directly on top of the pinned `d8686d52`. So the work is one command away: set `rev = "f3000c8d"` in the `vello` entries, run `cargo test -p animatix-render --features animatix/svg --test vello_img_probe` on a quiet machine, and read the A–D matrix. Pass → lift the pin everywhere and re-run `animatix video dogfood/projects/effects-wave1/entry.amx` (the symptom is a vanished checker backdrop between non-image renders); fail → the pin's comment gains the rev it was tested against, which is more than this row has ever had |

## Remaining work, next-session order

M1 (#1-#11) and M2 (#12-#16) are complete, as is M3 (#17-#20). What is left, in
the order that makes sense to attempt it:

1. ~~**#21 the second-input-texture ABI bump**~~ — **done** (`b27ca5e5`,
   `a046cbf6`). `web/recipes/scenes/bloom_stage.amx` demonstrates the pair on the
   site, and the content follow-up that sentence named is landed too: the tour's
   §05 `effects` scene now carries a `glow: Bloom` that goes up at 2.3 s and back
   down at 3.2 s (`f65b497c`), so the second input is taught where people learn
   the vocabulary, not only in the gallery.

2. **#22 glass** (needs mid-frame scene splitting that ABI v2 does *not*
   provide — the main vello target is still never an input texture), **#24 font
   weights** (three changes, not the one it was described as: asset, a
   numeric-weight path into typst, per-frame glyph recompile — see its row), and
   **#25 vello pin lift** (a local probe after a rev bump, not an upstream check —
   see its row). ~~#23 BarChart race~~ landed in batch 5. None of the rest is
   a quiet afternoon.
3. **#13 closeout, mostly done.** Landed across the round: the theme/vivid pack
   (`ef6a0c00` schemes, `2e170adf` `examples/lib/light.amx` genre pack), the
   glyph-gap item — which turned out to be closed already: the build warns
   `missing-glyph` naming each uncovered character (`declarations_text.rs:523`,
   pinned by `build_diagnostics.rs:260`) and the tour's formula scenes are
   written in ASCII, so no tofu ships, and the third preset layer —
   `web/recipes/` (`7d568e64`), seven single-idea scenes with the source beside
   each, so `docs/spec.md`'s Recipes section and the site now point at the same
   vocabulary. Still owed: the site content redo beyond the review pass,
   a decision on `web/demos/posters/*.png` (eight 1280×720 stills nothing
   references any more — the hub plays live `data-hoverplay` embeds, and
   `web/README.md` now says so; **deleting them is the owner's call**), and
   pruning this handoff into `docs/history.md` when the round closes.

4. **`always-overrides-keyframes` fires on declaration seeds, not on
   animation.** `examples/animation/36_light_pack.amx` warns that the `always`
   inside `MarchingRail` writes `dash_offset` on actor `rail`, "which also has
   keyframe animation". The guess recorded here when it was filed — that the
   write lands on `rail.line` while the lint looks at `rail` — was wrong: a
   single-actor component's internal actor *is* the instance track, because
   `animatix-syntax/src/module/rewrite.rs` drops the root label when it rewrites
   `self.line.dash_offset`, so the target really is `["rail"]`. The cause is one
   level down: the lint asks `has_keyframes_for`, which is
   `keyframe_count() > 0`, and `insert_end_keyframes` stamps a constant keyframe
   for every declared property — `dash_offset` gets one at the scene end holding
   its default `0.0`. Nothing animates. That is the same root cause batch 2 noted
   for `size` on a `Rect`: a non-zero keyframe count says *declared*, not
   *animated*, and no lint should treat the two as the same question.

   **Fixed in batch 5.** `AnimationTrack::is_property_animated` asks whether the
   track holds at least two keyframes that do not all carry the same value, and
   the lint asks it; `examples/animation/36_light_pack.amx` is now silent about
   `dash_offset`. What is given up: a *single* authored keyframe of a constant
   value can no longer be flagged, because it is bit-for-bit what declaration
   seeding leaves — the seeder writes the **declared** value, not the registry
   default (measured: `size: (100, 60)` yields one keyframe at `(50.0, 30.0)`,
   halved as `geometry.size` requires). Separating them needs keyframe
   provenance, and the storage is a bare `BTreeMap<u64, (T, Easing)>` with nowhere
   to put it. If that case ever matters, add provenance to `add_keyframe`; do not
   add another value heuristic.

   Two related things this surfaced, both **still open**:
   - `check examples/animation/36_light_pack.amx` reports
     `unused-label: Unused binding: 'p'` at `36_light_pack.amx:1:1` — but `p` is
     `let p = clamp(t / 0.9, 0, 1)` inside `Ticker`'s `always` in
     `examples/lib/light.amx:69`, and it is used on the very next line. The check
     is running over the expanded component body while attributing it to the
     importing file, and its usage scan misses the use: wrong subject, wrong
     span, wrong answer.
   - The `property → ActorField` table is duplicated **three** times in
     `timeline/dispatch.rs` (`has_keyframe_at`, `has_keyframes_for`,
     `list_keyframes`) — verified identical but for the fall-through arm. The new
     method avoided a fourth copy by resolving through the runtime
     `property_registry::lookup_property(…).field`; the three copies still want
     folding into one mapper.
5. **The camera's known limits** (follow-ups, not blockers): no per-actor opt-out
   for a HUD that must not move, and the camera is not carried across scenes by
   `persistent`/carry-bag.

   Two more were on this list and are now closed. The loop-perfect lint samples
   the camera axes (`Camera::seam_pairs`, named in the warning as `camera.zoom`);
   wiring it turned up a worse bug sitting next to it — `duration_seconds()` finds
   the content end by walking keyframe times over tracks, background and variable
   tracks, and had never been told the camera is content, so **a scene whose only
   motion is a push-in measured zero length** and the seam check skipped it
   entirely. `Camera::max_keyframe_time_ms` is now folded into that max.

   One follow-up this handoff used to list is **not** a gap: it claimed
   `animatix verify`'s ink checks were "in scene rather than screen space". They
   are not. `verify::bounds_map` reads `SceneProgram::precise_bounds`, which is
   documented and used as *world*-space, and its own doc line requires the
   observable renderer "so the bounds and the pixels come from the same
   evaluation" — so bounds and pixels are the same, camera-included space, and an
   actor the camera pushes off-frame clamps to an empty region and reads
   invisible, which is the right answer. The independent proof is the bounds work
   below: the derived filter region is built from those same subtree bounds, and
   it agreed with a camera-mapped authored rectangle to **0 pixels** at t=1.6.

   A `Filter` scope is correct under all three axes now, authored bounds
   included. A derived region never was a problem: the sub-scene renders with the
   node's `global_transform`, the region comes from the subtree bounds that
   transform recorded, and the composite blits back at that same
   `region.origin`. Authored `bounds: (x, y, w, h)` are scene coordinates, so
   `effect_scope_region` maps them through the camera affine before the support
   padding. Measured on `dogfood/probe_camera_scopes.amx`, authored against the
   same scene with `bounds:` deleted: **73,512 differing pixels at t=1.6 → 0**,
   with the 9-pixel padding floor at t=0.2 unchanged.

   Two things about that fix are worth keeping. It was scoped here as needing a
   field on `RenderFrame` plus its five construction sites, and that was wrong —
   `RenderFrame` already carries the frame's `overrides`, so the affine the root
   subtrees were wrapped in can be recomputed at the scope from the same inputs.
   And the first attempt passed the scope's own `global_transform`, which
   double-counted the scope's `at` and slid the region by (320, 180): 42,126
   differing pixels at t=0.2 where the answer is 9. The affine that belongs here
   is the camera's, not the node's.

   A delayed `camera.zoom` also used to zero the scene until its stamp arrived:
   the write helpers preserve the pre-stamp value, and for a track with no
   declaration behind it that fallback is `f32::default()` — `0.0` for zoom.
   Fixed in `889c6149`, where the *general* version of that fix is recorded as
   not free: `max_height`'s identity is `INFINITY` and the 1 ms preserve segment
   then interpolates to `NaN`.
6. **The residual reactive-frame cost is closed** (`1727d558`): the mechanism
   was `inject_property_into_env` walking every INJECTABLE row of every actor
   every frame, and six of batch 1/2's rows are now `ASSIGNABLE_A`. What is
   left is `dash_offset` — deliberately still injectable, because
   `(r.dash_offset + 0.4) % 17` is how marching ants are written — and the ~5%
   that remains on that bench, which is inside the run-to-run spread of this
   box. If it ever matters again, the structural fix is to inject lazily (only
   the properties a modifier program actually reads) rather than to keep
   demoting rows one at a time.

Milestones as originally proposed: M1 = items 1-11 (**complete**), M2 = 12-16
(**complete**), M3 = 17-20 (**complete**), M4 = 21-25 — #21 and #23 landed, #22
(glass), #24 (variable weights) and #25 (the vello pin) are open.

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

1. **Default easing** — done (`90285df1` + `ef3b400b`), narrower than proposed:
   entrances `expo-out`, exits `ease-in`, reposition `ease-in-out`, oscillators
   and *assignments* stay linear, because an action expands into assignments that
   inherit its timing modifiers and would double-ease a pre-baked curve. Measured
   with the noise-floor A/B discipline (two builds, a positive control, a corpus
   run).
2. **Color interpolation** — new `lerp_color_oklab` name (non-breaking);
   `lerp_color` untouched; the morph path's u8 sRGB lerp deliberately not
   switched yet.
3. **Particles** — done (`fdbdc5c0`) in the seeded-analytic form: no new
   primitive, no state model, deterministic under scrubbing in both directions.
4. **Icons** — done (`719d0c77`) on the Lucide-derived data-table route,
   unblocked by the #6 `draw-in` fix (`385f6e5d`).
5. **Beat syntax** — done (`a05210b7`): `#2b` stamps and `config { bpm }`.
   Durations (`[2b]`) are deliberately *not* resolved — the modifier path has no
   scene tempo in scope and reports rather than guesses.
6. **`--set` semantics** — done (`adf0fcbc`): CLI defines shadow the authored
   `let` defaults by *skipping* the authored assignment during the walk, which is
   what the first attempt got wrong (a `color: tint` fill stayed red).
7. **Loop lint v1** — done (`f69650d5`), with the `always`/plot-`func` skip
   documented as designed; batch 5 added the scene camera's three axes
   (`39bbf79a`), which the track walk could not see.
8. **Presets** — all three layers: engine verbs (`b4d8e0ac`), `examples/lib`
   genre packs (`examples/lib/light.amx` + `examples/animation/36_light_pack.amx`,
   batch 3), site recipes gallery (`web/recipes/`, batch 4). All three layers
   exist; the tour's Recipes section in `docs/spec.md` and the page now name the
   same moves.
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
