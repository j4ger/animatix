# Demo Gallery Plan

> Status: approved plan (2026-08-21). Implementation happens on branch
> `feat/demo-gallery` in the `animatix-gallery` worktree, phased per §8.
> This document is the working contract for the new demo suite; update it as
> phases land, and remove it once the suite is fully absorbed into
> `examples/README.md`.

## 1. Audit Findings (why a redesign)

Census over `examples/` (44 `.amx`, ~2900 lines) plus spec/roadmap review:

| # | Problem | Evidence |
|---|---------|----------|
| P1 | "Flagship" showcases are single-scene files with no transitions or narrative arc | `animation/16_showcase.amx` (58 lines), `composition/20_feature_reel.amx` (67 lines): a row of shapes + fade-in + sine breathing |
| P2 | Feature-checklist demos, not works: elements fade in, nothing to remember | `data/07_plots.amx`: four panels side by side, fade-in + resize |
| P3 | Headline capabilities barely exposed (files using them): Callout 2, Legend 1, Mask 1, NumberPlane 1, Filter 1, persist 1, wipe-in 1, reveal-in 1, `strategy:` 1, `stroke_progress` 1; only `fade`/`wipe-left` transitions used of six | grep census |
| P4 | Hand-written repetition instead of generation | `generation/17_audio_reactive.amx` hand-writes b0..b7 + eight `always` lines |
| P5 | Layout system underused: nearly everything is `anchor` + hand-computed `offset`; percentage/fill/auto sizing, min/max constraints, Grid, baseline, `dynamic_layout` swap/reorder absent from mainline demos | scan |
| P6 | `lib/` design system exists but is not reused; every demo authors its own title card | only 5 files import anything |
| P7 | No render-level regression: `scripts/check_examples.sh` only parses/checks | script content |

Conclusion: current examples are a fine syntax tutorial but do not demonstrate
what the system can produce.

## 2. Design Principles

1. **Works, not checklists.** Each gallery demo is a 30–90s story with an
   opening, development, and payoff.
2. **Full capability coverage.** The suite together must cover every
   Runtime-real surface in `docs/spec.md` (matrix in §5).
3. **Shared design system.** All gallery demos consume a rewritten `lib/`;
   the lib itself dogfoods modules/components/slots/types.
4. **Respect engine red lines** (§9) while writing.
5. **Verifiable.** Every demo passes `check` + multi-frame render smoke;
   README documents one-line export commands.

## 3. Copy Language Decision

Gallery and tutorial copy is **bilingual (Chinese + English)**, e.g.
main title Chinese + English kicker ("排序剧场 · Sorting Theatre").

### Spike outcome (Phase 1 prerequisite — DONE)

The bilingual probe (`examples/gallery/spike_bilingual.amx`, kept until
absorbed into `theme_studio`) exposed three engine defects, all fixed on this
branch before any demo work:

| Defect | Root cause | Fix |
|---|---|---|
| CJK text rendered nothing | Non-Latin content always takes the Typst path, whose world bundled only mock Open Sans + Fira Math — no glyph coverage, glyphs silently dropped | Renderer collects system faces covering uncovered characters (cached per char/face) and appends them to the Typst world (`f9c4cf94`) |
| Any line wider than ~453px silently wrapped | No `#set page` rule → Typst default A4 geometry; `compile_text` ignored max_width/align/overflow entirely | Explicit page sizing on all compile paths; `compile_text` honors the wrapping params (`f9c4cf94`) |
| `text_max_width` property did nothing | Declaration parser only matched legacy `max_width`; spec/registry/track say `text_max_width` | Both names accepted (`a8f626e6`) |

Also aligned the analyzer schema: `font_weight` accepts Num \| Str per runtime
(`parse_font_weight`), previously flagged every `"bold"` as a type error
(`96e19620`).

Known remaining nit: bold CJK may fall back to a regular face when the chosen
fallback family has no bold sibling in the world (one representative face per
family is loaded). Re-check during gallery work; not blocking.

### Phase 1 additional engine discoveries (while building `lib/` + `theme_studio`)

| Defect | Symptom | Workaround used |
|---|---|---|
| Component instances ignore `anchor`/`offset`/`at` | `warning: unknown-component-property` and the instance stays at the default origin | Wrap every positioned component instance in a `Group` and put the transform on the Group |
| `text_max_width` on Text inside a Col is auto-overridden with a collapsed width for CJK | Chinese labels wrap after 2–3 characters even with explicit `text_max_width` | Wrap each Text in a `Group` before placing it in a Col; this blocks the auto-propagation |
| Cross-file `@slot` fills are silently ignored | Imported `Card` with `@header`/`@body` always shows its fallback children | Keep slot-based components for same-file use only; build composed mockups with local hard-coded components when cross-file reuse is needed |
| Cross-file custom component actions are not resolved | `error[build:unknown-action]` for `pop_in`/`rise_in` defined in `lib/ui.amx` | Remove custom `fn` actions from `lib/ui.amx`; rely on built-in actions (`fade-in`, `pulse`, `shift`) for imported components |
| Multi-scene files clamp playback when a scene has zero inferred duration | A scene with only actor declarations got duration 0; when targeted by `play` the composition global duration collapsed to the outgoing play time, cutting off prior-scene actions | **Fixed 2026-08-22**: inferred scene durations are now floored to `max(incoming transition duration, 1/60s)`; multi-scene demos can proceed |
| `Path` actors do not render in `animatix image` export | Any `Path` with `stroke:`/`stroke_width:` produces a blank shape | Use `Rect`/`Polygon` bars or pre-rendered shapes instead of `Path` |
| `BarChart` `size:`/`at:` ignored in `animatix image` export | Chart renders as a tiny cluster regardless of explicit size | Build charts from `Rect` bars inside a `Row` |
| Transparent `Rect` overlays render opaque | `color: (0,0,0,0.6)` covers the screen with solid black | Avoid dim overlays; layer sharp cards directly on top, or use `Filter` where supported |

Style conventions live in `lib/theme.amx` comments: Chinese headline +
English kicker, keep technical terms in English, fixed-width number
formatting for count-ups.

## 4. Target Layout

```
examples/
├── gallery/                      ← NEW flagship layer
│   ├── epicycles.amx             Fourier series draws a square wave
│   ├── sorting_theatre.amx       algorithm theatre (sorting)
│   ├── dashboard_story.amx       one-screen data story
│   ├── motion_poster.amx         kinetic type poster (morph/mask/type)
│   ├── theme_studio.amx          theming & component system tour
│   └── brand_reel/               capstone title reel (multi-file)
│       ├── main.amx              + scenes/*.amx cross-file scene modules
├── lib/                          ← rewritten shared design system (§6)
├── basics|layout|animation|…     tutorial track kept, fully refurbished (§7)
└── projects/                     unchanged role: real-content dogfood
```

## 5. Flagship Demo Specs

### G1 `epicycles.amx` — "Drawing a Square Wave with Circles" (~60s, 5 scenes)
Beats: ① goal shot: square wave draw-in; ② one circle rotating at constant
speed (`always` analytic trajectory), tracing a sine via `stroke_progress`;
③ add circles one by one, the curve grows into a square wave while
`Equation`+`Fragment` highlights each harmonic term; ④ `BarChart` of 1/k
amplitude decay with targeted `Callout` on odd harmonics; ⑤ complex-plane
view (`NumberPlane` + rotating e^{iθ} vector), Typst formula finale.
Capabilities: `Graph.map/map_inverse`, actor anchors, analytic `always`
motion, `stroke_progress`, plot function transitions, `NumberPlane`,
Equation/Fragment highlight, targeted Callout, BarChart, Typst.
Hero moment: scene ③ — the wave "grows" into a square in front of you.

### G2 `sorting_theatre.amx` — "Sorting Theatre" (~45s, 3–4 scenes)
Beats: ① `for`-generated random bars in a `dynamic_layout` Row; ② build-time
precomputed sort (`[step:]` + `list_swap` + `if`) drives `swap` actions from
an event table; comparison pointer highlighted via runtime-index targets;
③ comparison/swap counters count up; ④ finish sweep (stagger pulse) + Typst
complexity footnote.
Capabilities: `for`/array actors/`[step:]`/`list_swap`/build-time branching,
`dynamic_layout`+`swap`, runtime-index `always`, `match`.
Hero moment: the whole sort runs with zero hand-written keyframes.
Division of labor vs `dogfood/projects/sorting-visualizer`: dogfood stays the
single-pass grammar probe; the gallery version is the polished multi-scene piece.

### G3 `dashboard_story.amx` — "One-Screen Data Story" (~50s, 5 scenes)
**Status: implemented.** Five scenes render cleanly; see `docs/handoff_phase2.md`
for the exact smoke times and engine workarounds used.

Beats: ① KPI row of `MetricCard` instances popping in with count-up text
override; ② weekly bar chart built from `Rect` bars + coordinate `Callout` on
the peak + `LegendItem`; ③ ranking change with `swap` and `reorder`; ④ focus
scene with insight card and takeaway `TitleCard`; ⑤ end card.
Capabilities: components, count-up text override, `Row`/dynamic layout,
`Callout`, `LegendItem`, `swap`/`reorder`, `play` transitions.
Workarounds: `Path`/`BarChart`/`Filter` transparent overlays are not usable in
`animatix image` export, so the chart is manual bars and the focus effect is a
simple layered card.

### G4 `motion_poster.amx` — "Motion Poster" (~30s, pure type & shape)
Beats: ① per-character staggered entrance (reveal-in/wipe-in + Mask);
② slogan morph via timed text cross-fade + `font_weight`/`letter_spacing`
animation; ③ three copies side by side morphing Path↔star↔circle under
`strategy: match` / `path_arc` / `stretch` — a direct strategy comparison;
④ background Image ken-burns inside a Mask + Filter blur/brightness breathing;
⑤ easing family showcase finale (bounce/elastic/back/expo).
Capabilities: full morph system, Mask, typography props, animatable Filter,
Image/Svg, all easing names.

### G5 `theme_studio.amx` — "Theme & Component Studio" (~40s, 4 scenes)
Beats: ① same UI mockup (login card) presented under editorial-dark /
custom `gallery` scheme (per-scene config + fade transitions); ② exploded
view of the Card component (slot contents fly out and back); ③ `strict_types`
scene showing typed component instantiation; ④ `color: auto` pool carousel.
The zero-duration-scene bug that blocked multi-scene component demos was
fixed in 2026-08-22, so the original 4-scene plan is viable again.
Capabilities: Colorscheme definition/inheritance, per-scene config, slots,
`fn`, type aliases, `strict_types`, `color: auto`. Doubles as the acceptance
demo for `lib/ui.amx` + `lib/charts.amx`.

### G6 `brand_reel/` — Capstone Title Reel (~75s, 5–6 scenes, multi-file)
Beats: ① logo draw-in + Filter sheen sweep; ② kinetic type slogan morphs;
③ six-capability Grid card wall (each with its own always micro-animation),
`reorder` promotes the featured card; ④ KPI flash montage; ⑤ mascot
`persist`s across scenes (position/color continuity), closed by `remove`;
⑥ finale: staggered regroup + Audio-beat-synced pulse, fade back to logo.
Capabilities: nearly everything — and each of the six play transitions must
appear at least once, plus cross-file scenes (`import as` + `play
alias.Scene`) and Audio mixing. The directory itself demonstrates multi-file
project organization.

### Capability Coverage Matrix (summary)

| Capability domain | Primary demo(s) |
|---|---|
| Layout (Grid/%/fill/constraints/reorder) | G3, G6, G2 |
| Actions/easings/effects | G4, G6 |
| Morph (text/path/strategy) | G4, G6 |
| Reactive (always/anchors/map/_animating_) | G1, G2, G3 |
| Plots (six plot kinds + function transitions + stroke_progress) | G1, G3 |
| Annotations (Callout/Legend/Equation highlight) | G1, G3 |
| Multi-scene (6 transitions/persist/cross-file/per-scene config) | G5, G6 |
| Components/slots/fn/type system | G5 + lib itself |
| Generation (for/arrays/[step:]/build-time algorithms) | G2 |
| Media (Image/Svg/Mask/Filter/Audio/typography) | G4, G6 |

## 6. Shared Design System (`lib/` rewrite)

| Module | Contents | Dogfoods |
|---|---|---|
| `theme.amx` | custom `Colorscheme { extends: "editorial-dark" }` + spacing/type-scale/rhythm `pub let` tokens + bilingual copy conventions | Colorscheme construct, namespace imports |
| `ui.amx` | `TitleCard` (kicker/title/sub), `SectionHeader`, `MetricCard` (KPI), `Card` (@slot), `Chip`, `Button` | `pub component`, typed params, `@slot` (same-file only) |
| `charts.amx` | `ChartPanel` (titled Graph card scaffold with `@graph` slot), `LegendItem` | nested components, Graph container, `@slot` |

## 7. Tutorial Track Refurbishment (full scope — decided)

All numbered files adopt the new lib: title cards → `TitleCard`, colors →
`theme.amx`, motion vocabulary → `motion.amx`. Focused rewrites:

- `generation/17_audio_reactive.amx`: b0..b7 → `for`-generated bars +
  runtime-index `always` (~50 → ~15 lines).
- `data/07_plots.amx`: four side-by-side panels → a micro-story ("one
  question, three views").
- `animation/16_showcase.amx`, `composition/20_feature_reel.amx`: **deleted**
  once `brand_reel` lands (decided); README points to gallery.

Constraint: teaching files stay single-purpose; polish must not bloat them.

> Status (2026-08-25): the full tutorial track has adopted the new lib —
> `examples/basics/`, `layout/`, `animation/`, `data/`, `components/`, and
> `generation/` every numbered file carries the dual `theme.amx` import
> (`import "../lib/theme.amx"` + `as theme`); `00_hello` swaps its hand-rolled
> card for the shared `TitleCard` (also importing `ui.amx`). Teaching-focused
> rewrites landed per §7: `data/07_plots` became a micro-story ("one question,
> three views" — a scalar field seen as a curve, gradient field, and heatmap,
> revealed sequentially); `generation/17_audio_reactive` replaced its hand-written
> `b0..b7` bars + 8-line `always` block with a `for`-generated `bar[i]` array and
> a runtime-indexed `always` (≈62 → 36 lines). These files already used semantic
> colorscheme tokens, so no raw colors were replaced; raw literals kept as the
> teaching point are `22_expressions`' `rgb`/`rgba`, `08_effects`' translucent
> `(0.1, 0.1, 0.12, 0.85)` card overlay (no matching opaque token),
> `26_data_math`'s per-bar `bar_colors` (incl. a non-token purple), and
> `29_strict_types`' `rgb(...)` typed-argument values. No `TitleCard` was added
> outside basics: every other opener is a single title heading or an annotation
> header over a richer demo, not a standalone card, and the 1040×230 component
> would overlap the content (bloat). `components/10_modules` keeps only the
> unaliased import (its `theme` alias is already bound to `reexport.amx`).

## 8. Phases

| Phase | Contents | Acceptance |
|---|---|---|
| 1 | rewrite `lib/` + bilingual render spike + `theme_studio.amx` | ✅ clean check + PNG smoke |
| 2 | `motion_poster.amx` + `dashboard_story.amx` | ✅ both (2026-08-24, on `feat/demo-gallery-p2`; see `docs/handoff_phase2.md`) |
| 3 | `epicycles.amx` + `sorting_theatre.amx` | ✅ both (2026-08-25; epicycles wave-reveal polish pending) |
| 4 | `brand_reel/` capstone → delete 16/20 → README points to gallery | ✅ WIP complete (2026-08-25): all six transitions, persist chain, Audio, cross-file scenes; polish pending |
| 5 | full tutorial-track refurbishment (§7) + README matrix + smoke-script extension | `scripts/check_examples.sh` green; render smoke covers all examples |

## 9. Authoring Red-Line Checklist (verify per demo)

- `always` is stateless: write every motion as an analytic function of `t`.
- reveal/draw actions are leaf-only; operate on leaves, fade containers.
- persist containers, not layout-managed children; no `persist` in last scene.
- `always` text overrides do not reflow layout → fixed-width strategies for
  count-ups.
- BarChart data has no animated transition → ranking changes use Rect arrays
  + swap.
- One `play` per scene.
- gif/video export needs the `video` feature (nix develop); CI defaults to
  PNG image smoke only.

## 10. Quality Gates

1. Every demo: `animatix check` with zero non-whitelisted warnings.
2. Render smoke: extend `scripts/check_examples.sh` (or add
   `scripts/render_gallery.sh`) to export 3 PNG frames (t=0/50%/100%) per
   gallery demo; GIF/MP4 commands documented in README, verified locally
   under `nix develop`.
3. `examples/README.md` gains a capability × demo matrix plus per-demo
   duration/scene-count/blurb rows.
4. `docs/spec.md` LLM Generation Checklist references gallery demos as
   canonical best-practice corpus.
