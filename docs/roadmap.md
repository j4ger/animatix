# Animatix Roadmap

Canonical source of truth for **remaining** work. Completed, resolved, and
closed work lives in [`history/README.md`](history/README.md) — do not re-add it here.

---

## Backlog & Prioritization

### User-facing delivery plan (2026-11-12)

The following milestones cover four priorities from the product review:
first-run delivery, editing reliability, parameterized production, and video
footage. All are **planned, not scheduled**; the order expresses dependencies,
not delivery dates. Existing capabilities are the starting point, not work to
reimplement. Detailed engine findings elsewhere in this roadmap remain the
evidence backlog; reproduce them against current code before assigning fixes.

| Priority | Milestone | User outcome | Dependency |
|---|---|---|---|
| P0 | First-run project and video delivery | Install, create an independent project, preview, and export a narrated MP4 without Rust or a repository checkout | Cross-platform packaging/encoding investigation |
| P0 | Reliable editing and language behavior | Format, edit in the GUI, and animate documented properties without unexplained source or visual changes | Starts alongside first-run work; blocks broad template rollout |
| P1 | Parameterized templates and batch production | Supply data once, preview variants, and render multiple versions of a multi-scene work | Reliable defaults, source round-trips, and export |
| P2 | Video footage in explanatory animations | Place a recording or clip alongside diagrams and export synchronized results | Stable media/export contract; preview/export parity |

#### P0 — First-run project and video delivery

- **Desktop acceptance.** Exercise the release ZIPs on clean macOS and Windows
  desktops with their normal graphics drivers. Confirm the Windows first-run
  path on a machine without the developer SDK. The hosted Windows proof uses
  an SDK DXC pair for WARP. [Hosted package evidence](history/2026-11.md#发布归档验收2026-11-18)
- **Acceptance:** on a clean supported machine, a user installs the release,
  creates a project outside the repository, previews it, and exports a 30-second
  MP4 with audio without installing Rust. Exercise this path per supported OS.
- **Selectable tracks:** Define delivery formats and player support for embedded
  subtitles and alternate audio tracks. Reuse the existing scene/clip timing to
  derive subtitle cue timestamps and language metadata for the selected container.

#### P0 — Reliable editing and language behavior

- **Source formatting.** Preserve author-selected multiline declarations.
  Keep the workbench source-only. Before adding
  timeline write-back, require comprehensible diffs that preserve comments and
  unrelated source. Inspector property editing is outside this milestone.
- **Property contracts.** Finish the remaining effective defaults below.
  Verify actor-specific diagnostics for indexed arrays with mixed types.
  Record visual compatibility changes to shipped scenes.
- **Behavior regressions.** Reproduce the remaining same-stamp morph, layout
  and composition cases below. Give the authored `play` timestamp a defined
  behavior or a diagnostic.
- **Acceptance:** repeated formatting is stable; GUI save/reopen preserves text
  semantics; supported property writes visibly work and unsupported writes report
  a diagnostic. Representative scenes agree across preview and export. Run the
  applicable CI gates and attach benchmark comparisons for hot-path changes.

#### P1 — Parameterized templates and batch production

- **Phase A: input contract.** Design explicit work-level inputs with types,
  defaults, required/optional status, numeric bounds, enum choices and descriptions.
  Extend existing `--set` / `data-vars` / `setVars()` semantics to multi-scene
  compositions with defined precedence and import/component scoping. Keep one
  validation contract across CLI, GUI and web; do not introduce parallel schemas.
  The engine now has a narrower single-scene `runtime_inputs` allowlist for finite
  numeric read-only frame snapshots; typed work-level defaults, bounds, enums,
  structured values and multi-scene scoping remain part of this phase.
- **Phase B: author and preview.** Author inputs in code and expose read-only
  inspection in the workbench; add JSON/CSV ingestion, data preview and
  field/row-specific diagnostics. Support
  structured inputs needed by a real data-story template. Validate long text,
  missing optional values and different image proportions. Build explicit tested
  landscape, portrait and square layout variants on existing layout primitives.
- **Phase C: local production.** Add a local batch queue with per-item progress,
  cancellation, failure reporting and retry. Save effective parameters, runtime
  version and output metadata with each job so a reviewed variant can be rendered
  again. Cloud rendering is outside this milestone.
- **Acceptance:** one multi-scene template consumes a dataset and produces at
  least 30 variants without source edits; invalid rows identify their fields and
  do not discard successful outputs. GUI/web preview and CLI export use the same
  effective inputs. Verify the three layout variants with visual review.

#### P2 — Video footage in explanatory animations

- **Phase A: media contract and spike.** Specify a video actor's source, source
  in/out points, timeline start, fit/crop, mute/volume and end-of-clip behavior.
  Validate decoding, timestamp mapping and frame seeking before committing to a
  backend. Keep GPU/decoder implementations in the presentation layer and media
  state within the existing ECS contract.
- **Phase B: native vertical slice.** Implement local-file playback, exact-time
  scrubbing and offline export, including the clip's audio mixed with existing
  Audio actors. Exercise overlays, scene transitions and seeks in both directions;
  define the supported formats and VFR timestamp policy explicitly.
- **Phase C: delivery and web boundary.** Integrate media import/inspection and
  missing/unsupported-file diagnostics. Decide and document browser playback
  support after the native slice; until supported, web builds must diagnose video
  actors rather than silently omit them. Network streams and a general-purpose
  nonlinear editor are outside this milestone.
- **Acceptance:** a screen recording can be trimmed, cropped, annotated and
  exported with synchronized audio; sampled frames match preview after random
  seeks and repeated exports. Cover clip boundaries and variable-frame-rate input,
  and measure decode/cache memory plus frame/export costs before rollout.

### User experience feedback follow-ups (2026-11-13)

The correctness-fix evidence and disposition of AMX-01–11 are recorded in
[`history/2026-11.md`](history/2026-11.md). Remaining work, in dependency order:

1. **Audit coverage (AMX-11, P1).** Exercise animated size/overlap around event
   boundaries and midpoints, distinguish layout-container siblings from
   intentional Stack/Group layering, and report sampling caps/skipped render
   rules. Existing overlap findings classify shared layout ancestors as
   explained; check false positives before changing that policy. Design optional
   instance-count/identity and minimum-gap assertions, including errors for empty
   or missing targets. Do not infer an author's expected instance count from
   geometry alone or introduce a second audit framework.
2. **Scientific color scale (P1).** Design explicit fixed/automatic domains,
   sequential/diverging palettes, invalid/out-of-range policy, shared/independent
   scales and a colorbar using the same mapping. The existing per-frame local
   normalization cannot support absolute cross-frame comparison. Build common
   tangent/projection/area helpers as components before adding native primitives.
3. **Independent frame-sequence export (P1).** Add an export entry point that
   does not update regression baselines. Define frame interval/endpoint and
   rounding, naming, overwrite, cancellation, partial-output and retry behavior;
   reuse the existing scene build/evaluate path.
4. **Portable authoring templates (P2).** Extend the existing figure kit with
   tested content budgets, explicit size breakpoints and long-text/multilingual
   fixtures after layout contracts are reliable. GUI changes require the missing
   companion design document; `.amx` remains the timing authority. Reuse `always`
   for shared responsive values.

Config `resolution` currently requires positive numeric literals: `--set W=...`
does not make `resolution: (W, H)` an evaluated expression. Keep this limitation
visible in authoring guidance; design config-expression evaluation and validation
before advertising parameterized canvas dimensions.

### Callable syntax migration validation (active)

- **Callable/plan migration build cost.** The 2026-11-17 CPU quick comparison
  retains 85/85 coverage but still reports
  material regressions in module/component/reactive builds and analyzer updates.
  Preserve the original baseline and adjacent old-source text control; do not
  attribute the text timing drift to the migration without further evidence.
  After ECS repairs, the 85-case comparison against the preceding migration
  build has one material analyzer-update regression and two inconclusive rows;
  the original pre-migration baseline still has eleven material regressions.
  See [`history/2026-11.md`](history/2026-11.md).

### Workbench acceptance

- Verify audible preview, seeking, and narration/caption synchronization with a
  working audio device. Reproduce the relative-document-path failure recorded
  during Linux review and pin a regression comparing relative and absolute
  opens of the same scene with a relative media asset.
- Run the workbench journeys on Windows and macOS and verify keyboard and
  screen-reader use of the icon controls. The November Linux review exercised
  waveform decoding, visual seeking and accessible control names.
- Integrate audit and storyboard evidence into the workbench and carry source
  provenance through generated and component-expanded actors. Keep the
  inspect-and-navigate workflow source-first. See [Workbench](gui-workbench.md).

### Motion-authoring trial follow-ups (2026-11-17)

These additions extend the existing delivery plan and AMX-11, rather than
opening parallel export, template or audit projects. Evidence comes from a
13-second data-story trial on `5a06a88f`: two parameter variants, native
SwiftShader renders, inspected hold/transition frames and 186 consecutive
pixel-checked frames. This is not browser, audio-sync, real-time playback or
hardware-performance evidence. Keep these follow-ups independent of the unified
`fn`/plan surface; this list selects no additional syntax or primitive.

1. **Sampled content and semantic assertions (P1, extends AMX-11).** Design
   machine-readable sampled text/numeric content and stable actor identity
   alongside existing measured geometry, with explicit units and unsupported/
   skipped coverage. The sorting oracle already checks live readouts through
   native engine/SDK access; CLI `check`/structural `audit` are not substitutes
   for that semantic oracle. Reuse the current evaluation/fact pipeline rather
   than introducing another inspector or audit framework.
   **Acceptance:** a small data-story fixture checks label/value/encoded-width
   agreement before, during and after a change; stale labels, wrong identity,
   and a constant final answer must build and then fail the assertions. Compare
   fresh, forward and reverse/random seeks. A checked source expression or
   expected-value JSON alone is not evidence of sampled output.
2. **Coordinate and measurement contracts (P1, extends AMX-11).** Document the
   units/basis of public measurements: authored full size versus internal
   half-extents, local versus scene coordinates, content bounds versus ink,
   and effective frame-time values. Extend the targeted dynamic-bounds/Filter
   ROI regressions with a broader transformed/nested/animated effect-clipping
   matrix and native/browser coverage; do not reopen those targeted fixes as
   new backlog items. **Acceptance:** independent rendered-ink checks agree
   with the documented bounds contract within declared tolerances, or report
   unsupported measurements explicitly. Include a changing-size benchmark:
   a fixed-size layout scrub is not evidence for this dynamic path. Keep the
   existing Graph-coordinate findings separate unless a shared cause is proven.
3. **Author/review/repair recipe (P1, extends parameterized templates).** Turn
   one small data-story and one typography/diagram scene into portable examples
   of compile → effective-parameter capture → hold and transition sampling →
   targeted repair → recheck. Use the existing CLI and frame-export work, not a
   second renderer or general agent framework. The trial needed manual stacked
   segment center arithmetic, a narrower-label variant, and replacement of an
   intentionally rolling Counter with a plain Text readout for precise digits;
   none proves a missing core primitive. Try reusable components/guidance first.
   **Acceptance:** a second parameter set exposes narrow/long labels, keeps
   data encodings truthful, and can be regenerated from saved inputs. Review
   intermediate frames, not only endpoints; preserve the failed and repaired
   evidence and report what was actually inspected.
4. **Narration/caption timing acceptance (P1, within first-run delivery).** Extend
   the existing narrated starter acceptance with one shared cue/time source for
   captions, scene holds and the actual recorded audio. The trial only exercised
   caption cues; it did not validate narration. **Acceptance:** render the actual
   shipped parameter variant, sample within each hold, check audio/caption onset
   and end boundaries plus seeks in both directions, and state tolerance and
   preview/export coverage. Do not label planned cues or a silent video as
   audio-synchronization evidence.
5. **Small motion-coverage corpus (P2, after reliable contracts).** Start with
   data reduction, rank overtaking, kinetic text and an explanatory diagram;
   record native / composed / workaround / unsupported status, authoring effort
   and evidence per case instead of claiming a global coverage percentage.
   Extend the existing sorting fixture with samples inside a swap, then use a
   separate changing-value rank fixture for overtaking: stable identity, tie
   policy, smooth position, labels and value widths must agree at crossing.
   Integer-rank teleportation must fail even when before/after order is correct.
   Keep semantic tests distinct from actual visual continuity/occlusion review;
   native SDK tests do not establish browser parity. Expand the corpus only
   when a new case probes a distinct mechanism or failure mode.

### Structure gate (active)

The large-file/large-function programme is governed by [`ecs.md`](ecs.md) §8:
`tools/quality/structure.sh` fails on a production file >1000 lines (tests >2000), a
production function >200 lines (tests >300), or a stale allowlist row. A split
deletes its row and never raises one. [`ecs.md`](ecs.md) is the normative
description of the timeline storage, not a work list.

### Review workflow follow-ups

- Extend AMX-11 with a seeded good/bad benchmark for existing rules and
  reproducible human review notes tied to source, actor, time and rendered
  frame. Use the existing audit schema and the motion-authoring repair recipe.
  Evaluate project-specific policy loading only if the built-in profiles and
  source markers cannot express the required tolerances.
- The structured external critic, review dataset export and on-device critic
  are deferred proposals. Revisit them after human review produces reusable
  evidence with stable source and frame identities. See
  [AI-assisted animation review](ai_agent_animation_quality.md).

### Extension follow-ups

The [primitive design note](primitive_abstraction.md#3-remaining-gaps-ranked-by-leverage)
records deferred build dispatch, applicability and default ownership,
overlapping taxonomies, shape metadata and closed draw commands. Revisit
these boundaries when a concrete extension or performance requirement
exercises them. Native child-strategy callbacks also need a demonstrated
plugin use case before expanding the ABI.

### Web playback performance (active)

> **Instrument warning, 2026-10-03.** `debug_bench_frames` on a *demo* page is
> not usable for small A/B comparisons: the page's adaptive quality step changes
> the raster scale between runs, and p50 swung 1.7 ms → 55.2 ms across two runs of
> the same build. Pin `set_render_scale` first, and prefer
> `web/dev/perf-probe.html` with `?base=`, which was built for this. A frame-cache
> gating change was measured on the [retired matrix page](https://github.com/j4ger/animatix/tree/fb1aee420f42e86e08f95f3e8e223394d9955056/web/demos/matrix) (four embeds, machine busy)
> and the result — p50 45.2 ms before, 49.4 ms after, at a pinned 0.5 scale — is
> **inconclusive in both directions**, so no perf claim is attached to it.

Method and measurements: `docs/performance_evaluation.md` §3.7. The harness,
display-matched raster/quality work, and measured web/native gap are archived in
[`history/2026-10.md`](history/2026-10.md) ("Roadmap cleanup"). Remaining:

| Item | Scope | Status |
|---|---|---|
| Refresh current-SDK page measurement | Migrate `web/tools/test/web-page-perf.mjs` away from its retired legacy Wasm-file prerequisite. Verify manifest/source identity and separately measure load admission, preparation, first completed draw and natural page cadence. Repeat on current main with fixed raster, actual adapter identity and multiple fresh processes; the [2026-11-15 snapshot](history/2026-11-15-web-performance-baseline.md) is historical evidence, not current FPS/LCP/INP. | Not started; old page attempts did not produce valid page-cadence results |
| Rich-content startup attribution | Re-profile current preparation and capability/font loading before changing concurrency. Compare cancellation-safe independent fetch overlap, rich compilation and font selection using paired startup/long-task measurements; preserve coverage and diagnostics. The historical warm rich build cost does not establish the current bottleneck or absence of same-page caches. | Measurement first; no claimed speedup |
| Revalidate multi-player presentation after later changes | Repeat fixed-raster 1/3/6-player draw/queue measurements and real visible-page cadence on current main, including weaker hardware. Account for the existing shared context, held-frame suppression and submit-scoped presentation bridge rather than proposing them again. Separate CPU, queue completion and actual GPU timestamps; preserve completed-frame semantics. | Current comparative evidence pending |
| Attribute the GPU-side gap | Narrowed by a WebGPU microbenchmark (§3.7 "Where the gap is not"): not fill rate (a full-target colour write is 0.058 ms vs a 3.42 ms frame), not the present blit (0.003 ms), not pass count (pinned vello uses exactly two compute passes). It sits inside the execution of those passes in the browser — Dawn's dispatch/barrier handling, plus a per-frame encoding upload bounded at ~0.22 ms/MB. JS-side compute timing is unusable (it reports CPU submission), so the next step is a GPU capture: RenderDoc against a `--no-sandbox` Chromium, or Dawn tracing via `--enable-dawn-features`. | Not started |
| `wasm-opt` speed pass | Release builds are size-optimised (`-Oz`). The build-profile A/B says the whole wasm CPU slice is small and unmeasurable from JS, so this is unlikely to be where the gap is — but it is untested. | Not started |
| Browser A/B of scale-animated `Filter` scopes | Re-run the browser measurement after the raster-scale work (`FilterBackend::set_raster_scale`, pixel-unit `EffectParamUnit::Pixel` parameters, ramp-width pin); see `docs/performance_evaluation.md` §The filter chain follows the raster. | Needs the browser A/B |
| rAF-cadence sampling in automation | The controller now has a real-rAF mode (`AP_MANUAL=0`, headed) and a deterministic manual-injection mode; the policy itself is validated (`docs/performance_evaluation.md` §3.7). Remaining: measure achieved fps / long tasks on **real GPU hardware**, which no automated environment here provides. | Needs real-GPU runner |
| Fixed per-embed frame cost | The P1 gate (2026-11-05, RTX 4060 WebGPU, `web/dev/perf-probe.html`) measured `gpu_ms` **flat across 16–27× raster area**: attention 0.75 ms fixed + ≤0.02 ms/MP (R² 0.32), plain-fill 1.34 ms fixed + 0.23 ms/MP (R² 0.11); 27× area changed `gpu_ms` by <0.1 ms. This run does not support prioritizing reductions in raster area. Players already share a context and queue. Measure further reductions in per-embed render/submission/present counts with `debug_bench_frames` and `debug_gpu_drain`. | **Not scheduled** — historical run found fixed costs dominated |
| `debug_readback` intermittently traps the wasm | The readback probe is flaky rather than wrong at a particular time, and with `panic = "abort"` a trap takes the whole page's shared engine with it: after one, later probes in that tab fail (`error` states, `Runtime.evaluate` timeouts) until the browser restarts. Evidence from the 2026-10-01 sweep (flake `.#web` Chromium, `--webgpu`): the same call alternates — `tour/scenes/syntax.amx` `debug_readback(1.95)` timed out once and returned 27 distinct colours on the repeat; [scene.amx](https://github.com/j4ger/animatix/blob/fb1aee420f42e86e08f95f3e8e223394d9955056/web/demos/hash/scene.amx) `debug_readback(19.5)` hung twice while 19.0/20.0/20.5 were fine; one tour sweep surfaced `RuntimeError: unreachable` with frames inside `animatix_web_bg.wasm`. Meanwhile `render_frame` at the same time is fine and the transformer page's seven scenes, the hero and the hash/sorting compositions all read back — so the shipped demos render; only the probe path dies. Affects `web/dev/multi-probe.html?readback=1` and the `svg-probe/*` harnesses, which is why they need a fresh browser per probe until this is understood. | Not started |

## Planned Features

| Feature | Notes | Status |
|---|---|---|
| `Code` custom highlighting theme | Token colours now follow the active colorscheme via a role→named-token palette (`ResolvedColorscheme::highlight_palette`, resolved at build). A fully custom palette (arbitrary per-role colours, or a user-supplied `.tmTheme`) is not exposed as an authoring surface; deferred until a real content need. | Not started |
| Re-declaration morphs sharing one keyframe stamp | When two actor re-declarations share a single `#t` stamp, the actors render their **target** geometry on every frame before the morph starts (an `Ellipse`→`Rect` re-declaration shows a square on pre-morph frames; a size-less `Polygon` leaked as a default-size `Ellipse`). One re-declaration per stamp is safe — [morph.amx](https://github.com/j4ger/animatix/blob/fb1aee420f42e86e08f95f3e8e223394d9955056/web/tour/scenes/morph.amx) exercises the full sequence leak-free. Found while frame-auditing [hero.amx](https://github.com/j4ger/animatix/blob/fb1aee420f42e86e08f95f3e8e223394d9955056/web/scenes/hero.amx) (probes in the 2026-10-01 site session); the hero now choreographs its trio with rotations/resizes instead. Fix direction: a morph track's pre-start evaluation must hold the *previous* declaration, not the target. | Not started |

## Dependency & Verification Constraints

| Item | What it is | Status |
|---|---|---|
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

## Web-demo review pass — engine findings (2026-10-02)

Behaviour gaps found while polishing the site's scenes, each worked around
scene-side first. Four of the nine were fixed in the engine and are archived in
[`history/2026-10.md`](history/2026-10.md) ("Easing pass" and "Web-demo review
pass"); what is left is below.

| Item | What it is | Status |
|---|---|---|
| `pulse intensity` is additive | `intensity: 1.04` means 2.04× scale (peak = start × (1 + N)) — three demo diagrams were destroyed at their thesis beat before the semantics were spotted. Now documented in `docs/spec.md`; a gentler authoring story (percentage semantics or a lint for `intensity > 1`) is open. | Documented |
| Pixel A/B has no noise floor yet | The render-either-side-and-`cmp` method this round used for content checks cannot resolve a small change: **the same binary is not byte-stable across processes.** Measured 2026-10-03 on a now-retired text-rendering probe at 1 s (source remains in Git) — three renders of one binary differ from each other by 20, 36 and 46 raw RGBA bytes (of 3 686 400), while that binary's two builds (which really do differ) are 12 and 24 bytes apart. PNG file sizes drift as well, so comparing files is noisier still, and `web/demos/transformer/scenes/pipeline.amx @ 4` came out *identical* between builds but 8 bytes apart between two runs of one build. Text/glyph scenes show it and [scene.amx](https://github.com/j4ger/animatix/blob/fb1aee420f42e86e08f95f3e8e223394d9955056/web/demos/matrix/scene.amx) does not, so the suspect is glyph rasterization or shaping order. Until that is pinned down: any engine A/B claim needs (a) a same-binary control and (b) a positive control proving the change is live — the track-level probe above is the shape to copy. | Not started |

## Easing pass — what else it turned up (2026-10-02)

What follows is everything the pass surfaced but did not fix, sorted by whether
the behaviour is *wrong* or merely *shaped badly*; what it did land is archived in
[`history/2026-10.md`](history/2026-10.md) ("Easing pass — what else it resolved").

### Bugs — the behaviour is wrong

| # | Item | Evidence |
|---|---|---|
| 2 | **`animatix fmt` reflows declarations.** | Comment deletion is fixed (2026-11-03): `Formatter::format_source` reinserts every `//` class from the lossless token stream — file header, free-standing, in-block, trailing, comment-only — with round-trip, idempotence, and scrubbed-AST tests. Remaining: formatting still collapses every multi-line declaration onto one line ([hero.amx](https://github.com/j4ger/animatix/blob/fb1aee420f42e86e08f95f3e8e223394d9955056/web/scenes/hero.amx) 202 → 124 lines), and nested-block indentation is sometimes doubled. A `for` inside a container can also lose indentation on its header and child nodes (`format_core::format_inline_item`, reproduced by `web/content/scenes/ripple-and-cascade.amx`). |
| 3 | **The native-plugin ABI silently flattens most easings to Linear.** | `easing_code` (`extension_native_plugin/`) maps everything outside {EaseIn, EaseOut, EaseInOut} onto code 0, so a plugin reading a `spring`, `bounce`, or `cubic_bezier` track sees a linear one. Pre-dates this pass. Widening the code space is a plugin-API version bump, not an easing change. |
| 5 | **A component actor declared between keyframes renders half-drawn.** | `hash/lookup.amx`'s `q2`/`q3` (`LabeledBox`) appeared as a dim box *without its label* in beats before their own `fade_in`. Worked around scene-side with `opacity: 0.0`, but the engine question stands: "declared inside a keyframe is visible immediately" should not produce a partially drawn component. Narrowed 2026-10-03: the mechanism first blamed for it — a re-declaration sampling its own target because a sibling wrote a keyframe at the same `#t` — was probed on plain shape actors in four orderings and does **not** reproduce (the pre-frame keeps the old shape and colour and the morph still lands on target); `a_redeclaration_starts_from_what_it_replaces_not_from_its_target` pins that. What is left is specific to *component* actors, whose parts are separate sub-actors seeded during expansion, and it still needs a minimal repro outside the demo. |

### Design flaws — the shape invites the wrong thing

| # | Item |
|---|---|
| 8 | **Nothing keeps a coupled ease pair in sync.** The hero's carriage "rides the draw tip" by repeating the `draw_in`'s start, span *and* ease. A mechanical retune of the draw silently separated the two and no test or diagnostic noticed. Either give the follower its own construct, or warn on two segments with identical span and different easing. |
| 9 | **Curve names that promise physics.** An easing is a scalar map over progress between two values; it can only modulate travel *along* that line, so no easing can produce an arc. `bounce` was renamed `bounce_in` and the boundary is now written into `docs/spec.md`, but `elastic` still promises a spring and delivers a progress wobble. |
| 10 | **Page-level animation review.** The current web smoke gate captures visible canvas pixels and exercises scrolling and play controls. Add targeted page-level evidence when a change affects the surrounding interface, such as hover or scroll transitions outside the canvas. |
| 11 | **Nothing keeps the corpus on its motion vocabulary.** The site was 155-of-207 annotations on one quadratic curve, with ~880 timed statements carrying no ease at all (so: Linear). Fixed by convention plus `docs/spec.md`'s reference table, but nothing enforces it — a scene can slide back to monoculture unnoticed. A lint over "which role uses which curve" is the durable version of this pass. |

## Motion-vocabulary round — what it left open (2026-10-05)

The round itself is recorded in [`history/2026-10.md`](history/2026-10.md) ("The Motion-Vocabulary
Round — Landed Work"). The following items remain:

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

## Browser rendering validation

The October browser findings are recorded in [history](history/2026-10.md).
Current smoke checks capture the visible canvas in Chromium with SwiftShader.
The following scenarios still need targeted coverage:

- **`Glass` frost pixels in the web player.** Headless Chromium cannot make a
  readable RGBA texture (`Could not find SharedImageBackingFactory … RGBA_8888 …
  WebgpuRead`) — the readback path loses the device the moment it allocates — and
  `--screenshot` returns byte-identical frames for a looping scene, so canvas
  contents are not in the capture either. What *is* proven in the browser: the
  scene builds under the shipped profile, the engine initialises, and the figure
  renders through the same scale-aware `drain_pending_layers` the native path
  uses. The pixel behaviour of the frost rests on the native measurement
  (`(93,27,32)` frosted vs `(255,0,0)` unfrosted on a 4 px border). A machine with
  working WebGPU adapter can settle it with a fixture that samples the panel
  interior at zero and nonzero blur on a contrasting border. The archived
  [glass.amx](https://github.com/j4ger/animatix/blob/fb1aee420f42e86e08f95f3e8e223394d9955056/web/tour/scenes/glass.amx) is a starting point for that comparison.
- **Mixed capabilities on one page.** The current smoke tests exercise
  individual shapes, text/font and SVG fixtures, and the transaction test
  checks device sharing with two concurrent players. Mount a base-only
  scene, rich text and SVG/media together using the shipped SDK. Record
  capability and font requests, one shared engine/device, and painted output
  for all three players. Include a hardware WebGPU run.
- **Gradient comparison playback, if republished.** Exercise the parameter
  slider and scene replay with a working WebGPU adapter. The archived
  browser check covered controls and layout. See [comparison layout](comparison-layout.md).

## Found by the transformer review, left open on purpose (2026-10-06)

- **Region-scoped baseline comparison is still open.** `animatix baseline`
  compares the whole rendered frame against an approved PNG at full resolution,
  so a beat whose entire change is a 3px line (0.26% of a 1280×720 plate) is
  visible. What is missing is a region-scoped threshold — restricting a
  comparison to one actor's bounds — for baselines that should apply to a
  single region instead of the whole frame.
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

- **`baseline`/`DEAD-BEAT` sampling is meaningless for a multi-scene plate.** The
  per-keyframe sweep samples `#Ns` stamps as absolute times, but in a file with
  `# Scene` sections those stamps are scene-local — `demos/hash/scene.amx` was
  flagged at "10.5s→11.2s" while its q3 shift at that label moves 9 831 px. The
  sweep now restricts per-beat checks to single-scene files and keeps only the
  held-frame check for plates; a plate needs its beats resolved through
  `animatix timeline` before it can be swept beat by beat.

## Graph tick labels collide at the origin (2026-10-06)

[plots.amx](https://github.com/j4ger/animatix/blob/fb1aee420f42e86e08f95f3e8e223394d9955056/web/tour/scenes/plots.amx) draws `0.0` (the y-axis label) and `0` (the x-axis
label) at the crossing of the two axes, with the axis rule running through both.
It is the `Graph` primitive's tick layout, not that scene's authoring — every
`Graph` with both axes labelled and a zero in both domains gets the same
overprint. The fix belongs in the tick-label pass: skip the zero tick on one
axis when both axes are drawn, or offset the pair, which is what most plotting
libraries do. Left alone in this pass because the only content-side workaround is
to move the domains off zero, which changes what the figure teaches.

## Imported palettes and the embed's module path (2026-10-06) — worked around, still open

**Fixed in the former demo sources.** The old [demo collection](https://github.com/j4ger/animatix/tree/fb1aee420f42e86e08f95f3e8e223394d9955056/web/demos)
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

The mechanism is still a bug, and the shape of it is this: `host::prepare` builds
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

These findings came from the October rendering review. Check them against
the current renderer before changing the authored workarounds.

- **Verify rendered Graph-hosted bars.** The engine now builds `BarChart`
  paths in the parent Graph's coordinate frame. The
  `hosted_bar_chart_spans_graph_axis` test checks the baseline and path extents.
  Verify rendered bars against the ticks in a hosted scene, including the
  earlier spectrum alignment case, before removing its standalone workaround.
- **Define the contract for timed `play`.** The composition builder extracts
  the keyframe stamp and the edge builder ignores it. A 500 ms transition from
  a 10 s scene starts at 9.5 s, even with a preceding `#8s`. Define which
  placement forms schedule an edge and diagnose unsupported ones, including
  `play` inside a keyframe. Cover composition timing in the tests.
- **`fade_in` on a stroke-only actor animates `stroke_progress`, so it fights a
  per-frame write to the same property.** An `Ellipse` with `fill_opacity: 0`
  used as an angle arc, revealed by `fade_in` and driven by
  `arc.stroke_progress = …` in `always`, renders as a closed ring and reports
  `always-overrides-keyframes` even after the declaration drops its
  `stroke_progress`. The warning is right that something is overriding, and
  wrong that the `always` block wins. The arc was removed from
  [complexplane.amx](https://github.com/j4ger/animatix/blob/fb1aee420f42e86e08f95f3e8e223394d9955056/web/demos/epicycles/complexplane.amx); a `reveals`-style entrance that never
  touches `stroke_progress`, or a dedicated `show` action, would let it come back.
- **Verify mapped marker alignment in a rendered Graph.** The old half-scale
  note and `graph.map()` call are gone from [descent.amx](https://github.com/j4ger/animatix/blob/fb1aee420f42e86e08f95f3e8e223394d9955056/web/demos/gradient/descent.amx).
  The scene now computes the ball's pixel position directly. The prior numeric
  probe mapped `(1.0, 0.5)` to `(790, 285)` on a graph scaled at 150 px per unit.
  Check rendered marker and curve alignment before replacing the manual mapping.
- **`stroke_progress` overshoots by one tessellation segment, so a marker
  cannot sit on a plot's leading edge.** Measured on the [retired Epicycles walkthrough](https://github.com/j4ger/animatix/tree/fb1aee420f42e86e08f95f3e8e223394d9955056/web/demos/epicycles)
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

## Remaining property defaults and measurement contracts

- Graph-child coordinate mapping has a 500-by-500 parent fallback.
  Verify when it can be reached and align its units with the measured Graph
  geometry and public size readout.
- Sampling floors for `resolution` differ by plot family. Pin their
  intended behavior with rendered evidence before unifying or documenting
  the numeric floors.

## Release validation still requiring live evidence

The original October pipeline assumptions are recorded in
[history](history/2026-10.md). The current release process requires an explicitly
chosen version and tag with finalized release notes named `vX.Y.Z.md` in
[`docs/releases/`](releases/README.md).

- Validate the next authorized versioned release end to end: version/tag identity,
  committed release-note selection, artifact generation and publication. Local helper
  tests do not establish that a hosted release has succeeded.
- Verify downloaded native release artifacts on their target platforms, including
  Linux portability outside Nix. A successful build alone is not runtime evidence.
- Compare the published site and SDK manifests with the release's source identity.
- Verify the Vello GPU packages and Web SDK on the remaining target platforms
  in the [release handoff](handoff_vello_gpu.md). Record post-release startup
  and recovery results and keep the previous immutable builds for rollback.
- Use the archived Compute and Vello GPU results for post-migration SDK and
  export optimization comparisons. Revisit the third-party Vello CPU edges as
  Typst/Hayro and epaint change their dependency graphs.
- Reconcile the direct Pages upload workflow with the journal-backed publisher.
  Verify the hosted workflow variables, credentials and target projects, then
  retire or isolate the direct uploader so only the journal-backed workflow
  can write to the canonical project. Add coverage for that production wiring.
  See [Pages deployment](cloudflare-pages.md#current-state).

## Performance & Pipeline Optimization (Remaining Backlog)

### 1. Remaining text shaping and layout costs

The text compiler already caches compiled layouts, font data and fallback
lookups. Eligible plain text can use Vello glyph runs. Measure cache misses,
dynamic wrapping and reveal/morph work in representative scenes. Compare grouped
rich-text compilation and reuse of Typst context before changing that path. For
stable kinetic text, measure whether precomputing line clips at build time reduces
frame cost. Use the text benchmarks described in
[performance evaluation](performance_evaluation.md).

### 2. GPU-Accelerated Procedural Curves & Chart Strips

- **Scope**: `crates/animatix/src/timeline/plot/` & `crates/animatix-render/src/instanced_line.rs`
- **Current behaviour**: Mathematical curves (`ProceduralPlot`), parametric functions, and bar chart transitions currently resample hundreds of evaluation points and construct Kurbo/Vello vector paths on the CPU every frame, occupying the CPU rendering thread.
- **Target**: Compare GPU curve evaluation and strip generation with CPU sampling
  on a dense animated fixture. The existing `InstancedLinePipeline` draws from
  CPU-generated segment instances. Validate visual parity and the frame-time
  benefit before integrating GPU evaluation into scene rendering.
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

## Found by the template round, left open (2026-11-11)

- **`Graph`-generated sub-actors trip the lifecycle rules.** A `Graph` + `PlotCurve`
  evaluates tick/curve sub-actors (`plot_tick_x_0`, `curve`) whose own opacity
  track is flat zero — the container's `draw_in` drives them. `actor.
  opacity-never-positive` has no way to tell "container-generated fragment"
  from "author-declared actor that never shows": `parent` is `None` for these
  (they are evaluation-time spawns, not hierarchy children) and the label has
  no stable marker like the `for`-loop `__` suffix. The fix is an engine-side
  `generated_by` (or equivalent) on `ActorFact` so rules can skip what the
  container owns. Until then `crates/animatix-cli/tests/fixtures/amx/templates/explain-figure.amx` uses a
  hand-drawn `Path` to stay warning-free.
