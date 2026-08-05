# Rewrite Plan: `examples/fft_explain.amx`

## Goal
Rewrite `examples/fft_explain.amx` into a standalone, runnable 5-scene FFT
explainer that fixes all critical/moderate audit issues, uses idiomatic
multi-scene patterns from `examples/14_multiscene.amx`, and keeps the
educational content focused — without over-engineering.

## Verified ground truth (from source, not just audit)
These facts were confirmed against `crates/animatix/` and `docs/` and change
how some audit items are treated:

- **`play` is required for multi-scene.** `Stmt::Play { scene_name, transition }`
  (`crates/animatix/src/ast.rs:697`, processed in `composition.rs:588`).
  Missing edges → `OrphanScene` diagnostic; the composition never leaves scene 1.
  → Audit Critical #1 is real.
- **`bar_colors` accepts ONLY RGBA number tuples in braces.**
  `build/plot.rs:1652-1679` matches `Expr::List` (braces `{}`) of `Expr::Tuple`
  of `Expr::Num`. Scheme tokens (`accent.danger` → `Expr::Path`) are silently
  dropped, so bars fall back to default `color`.
  NOTE: `docs/primitives.md` shows `bar_colors: (accent.danger, …)` — that doc
  example is itself wrong (parens → `Expr::Tuple` outer, never enters the
  `Expr::List` branch; and scheme tokens aren't `Expr::Tuple` either).
  → Audit Critical #2 is real. Fix uses RGBA tuples, not scheme tokens.
- **Editorial-dark accent values** (`timeline/colorscheme.rs`, `EditorialDark`):
  `accent.danger` = `(1.0, 0.46, 0.54, 1.0)`,
  `accent.success` = `(0.35, 0.86, 0.63, 1.0)`,
  `accent.warning` = `(0.98, 0.83, 0.44, 1.0)`.
  Use these in `bar_colors` so bars match the Decomposition curve colors.
- **`at` is absolute (origin = top-left; center = (640, 360)).** `anchor`+`at`
  together → `ConflictingPositionBinding` warning, **anchor wins, `at` dropped**
  (`position.rs:67-86`). Bare `at: (0, -300)` → absolute off-screen pixel.
  The original's step titles / labels / equations are therefore genuinely
  mispositioned, not merely "inconsistent." → Audit Moderate #3 understates it.
- **`Equation` container `color` IS valid.** `scene_eval.rs:734-736` reads the
  Equation track's `style.color` as the text color for the whole equation.
  → Audit Moderate #4 is WRONG. Keep `color: text.muted` on the `Equation`;
  do NOT move it to `Fragment` children (Fragments have no `color` property).
- **`func` is `BuildTimeOnly`, non-assignable** (`property_registry.rs:633`).
  A curve's function cannot change per-frame. "Gradual reconstruction" must use
  stacked partial-sum curves + opacity crossfade (not a mutating `func`).
- **Graph/Equation children are addressable.** Labeled inline items become
  real tracks (`build/container.rs` `process_inline_items`). Equation fragments
  use dot-path (`eq.f1`); Graph PlotCurve children use bare label (`sine_2hz`).
  `fade-in sine_2hz` and `highlight decomp_eq.f1 […]` are valid.
- **`always`+`t`** supports `let`, assignments, `if`, `%`, tuple construction
  (see `examples/06_reactive.amx`). A *scrolling/reactive waveform* is NOT
  feasible (`func`/`x_domain` frozen); only assignable props (at/opacity/size/
  color/stroke_progress) can react.
- **`Typst` uses `content:`** (spec.md:543,561) — original is correct.
- **`highlight` sig:** `highlight target [color: C, blend: B, padding: P,
  radius: R, duration, ease]` (spec.md:267). `color: white, blend: difference`
  is valid.

## Plan (numbered, fixer-sized tasks)

### Task 1 — Header + global config; strip design-note comments
Remove all `// MISSING: …` and `// Workaround: …` comments (they belong in
issues/roadmap, per task requirements). Keep a 3-line educational header.
Keep one top-level `config`.
```amx
// FFT Explanation Animation
// A visual walkthrough of the Fast Fourier Transform:
// time-domain signal → sine decomposition → frequency spectrum → reconstruction.

config { colorscheme: "editorial-dark", resolution: (1280, 720) }
```

### Task 2 — Scene 1 `TitleCard` (fix positioning, add `play`)
Use `anchor`+`offset` (idiomatic). Drop the conflicting `at`. End with `play`.
```amx
# TitleCard
config { colorscheme: "editorial-dark" }

title: Text, text: "Understanding the FFT", font_size: 64,
  color: text.primary, anchor: scene.center, offset: (0, -40)

subtitle: Text, text: "From time domain to frequency domain", font_size: 28,
  color: text.secondary, anchor: scene.center, offset: (0, 40)

#0.5s
fade-in title [800ms, ease: ease-out]

#1.5s
fade-in subtitle [600ms, ease: ease-out]

play TimeDomain [fade, 400ms]
```

### Task 3 — Scene 2 `TimeDomain` (fix positioning; add optional `always` playhead)
Graphs keep absolute `at` (centered). Text uses `anchor`+`offset`. Replace the
`// Workaround` note with the real pattern (fade the whole graph; curves have no
draw-in). Add ONE small reactive element: a sweeping sampling playhead driven by
`t` in `always` — reinforces "FFT samples in time." (See Q2 rationale.)
```amx
# TimeDomain
config { colorscheme: "editorial-dark" }

step_title: Text, text: "1. A Complex Waveform", font_size: 40,
  color: text.primary, anchor: scene.top, offset: (0, 60)

signal_graph: Graph, at: (640, 380), size: (900, 300),
  x_domain: (0, 4), y_domain: (-3, 3) {
  composite: PlotCurve, kind: "cartesian",
    func: (t) => sin(2 * pi * 2 * t) + 0.55 * sin(2 * pi * 5 * t) + 0.3 * sin(2 * pi * 9 * t),
    color: accent.primary, stroke_width: 3
}

signal_label: Text, text: "x(t) = sin(2π·2t) + 0.55·sin(2π·5t) + 0.3·sin(2π·9t)",
  font_size: 16, color: text.muted, anchor: scene.center, offset: (0, 250)

// Sampling playhead: sweeps one full window (x_domain 0..4) every 4s, looping.
playhead: Rect, size: (2, 300), color: accent.warning, opacity: 0.7, at: (190, 380)

always {
  let x = 190 + (t % 4.0) / 4.0 * 900
  playhead.at = (x, 380)
}

#0.5s
fade-in step_title [500ms, ease: ease-out]

#1.2s
fade-in signal_graph [1s, ease: ease-out]

#2.5s
fade-in signal_label [400ms]
fade-in playhead [400ms]

play Decomposition [fade, 400ms]
```
> Coord math: graph left edge x = 640 - 900/2 = 190; width 900 maps x_domain
> 0..4. `playhead` y=380 matches graph center; height 300 matches graph height.

### Task 4 — Scene 3 `Decomposition` (fix positioning; keep highlight sync)
Fix all `at:` → `anchor`+`offset` for text/equation. Keep the Equation container
`color: text.muted` (it is valid — see ground truth). Keep `highlight
decomp_eq.f1 [color: white, blend: difference, 800ms]` syncing with each curve
reveal. Drop `// MISSING`/`// Workaround` comments.
```amx
# Decomposition
config { colorscheme: "editorial-dark" }

step2_title: Text, text: "2. Decompose Into Sine Waves", font_size: 40,
  color: text.primary, anchor: scene.top, offset: (0, 60)

decomp_graph: Graph, at: (640, 400), size: (900, 320),
  x_domain: (0, 4), y_domain: (-4, 4) {
  ref_signal: PlotCurve, kind: "cartesian",
    func: (t) => sin(2 * pi * 2 * t) + 0.55 * sin(2 * pi * 5 * t) + 0.3 * sin(2 * pi * 9 * t),
    color: accent.primary, opacity: 0.3, stroke_width: 2
  sine_2hz: PlotCurve, kind: "cartesian", func: (t) => sin(2 * pi * 2 * t),
    color: accent.danger, stroke_width: 3
  sine_5hz: PlotCurve, kind: "cartesian", func: (t) => 0.55 * sin(2 * pi * 5 * t),
    color: accent.success, stroke_width: 3
  sine_9hz: PlotCurve, kind: "cartesian", func: (t) => 0.3 * sin(2 * pi * 9 * t),
    color: accent.warning, stroke_width: 3
}

decomp_eq: Equation, font_size: 22, color: text.muted,
  anchor: scene.center, offset: (0, -200) {
  pre: Fragment, content: "x(t) = "
  f1: Fragment, content: "sin(2 pi dot 2t)"
  mid1: Fragment, content: " + 0.55 dot "
  f2: Fragment, content: "sin(2 pi dot 5t)"
  mid2: Fragment, content: " + 0.3 dot "
  f3: Fragment, content: "sin(2 pi dot 9t)"
}

#0.5s
fade-in step2_title [500ms, ease: ease-out]

#1s
fade-in decomp_graph [500ms]
fade-in decomp_eq [500ms]

#2s
fade-in ref_signal [300ms]

#3s
highlight decomp_eq.f1 [color: white, blend: difference, 800ms]
fade-in sine_2hz [800ms]

#5s
unhighlight decomp_eq.f1 [400ms]
highlight decomp_eq.f2 [color: white, blend: difference, 800ms]
fade-in sine_5hz [800ms]

#7s
unhighlight decomp_eq.f2 [400ms]
highlight decomp_eq.f3 [color: white, blend: difference, 800ms]
fade-in sine_9hz [800ms]

play Spectrum [wipe-left, 400ms]
```
> Removed the per-curve `f1_label`/`f2_label`/`f3_label` Text children: the
> colors + synced highlight already identify each component, and the equation
> fragments carry the Hz values. Less clutter, no fragile absolute label
> positions (addresses Audit Moderate #6 partially).

### Task 5 — Scene 4 `Spectrum` (fix `bar_colors` to RGBA; fix positioning)
This is the Critical #2 fix. Use RGBA tuples matching editorial-dark
accent.danger/success/warning. Use `show_labels: true` and string keys so the
BarChart renders its own labels — delete the three hardcoded `bar_label_*`
Text actors (Audit Moderate #6). Fix `at`→absolute for the chart, `anchor`+
`offset` for text.
```amx
# Spectrum
config { colorscheme: "editorial-dark" }

step3_title: Text, text: "3. Frequency Spectrum", font_size: 40,
  color: text.primary, anchor: scene.top, offset: (0, 60)

equation: Typst, content: "$x(t) = sum_(k) A_k sin(2 pi f_k t)$",
  font_size: 24, color: accent.primary, anchor: scene.center, offset: (0, -200)

spectrum: BarChart,
  data: {("2 Hz", 1.0), ("5 Hz", 0.55), ("9 Hz", 0.3)},
  size: (600, 260),
  bar_colors: {(1.0, 0.46, 0.54, 1.0), (0.35, 0.86, 0.63, 1.0), (0.98, 0.83, 0.44, 1.0)},
  show_axis: true,
  show_labels: true,
  at: (640, 420)

#0.5s
fade-in step3_title [500ms, ease: ease-out]

#1.2s
fade-in equation [500ms]

#2s
fade-in spectrum [800ms, ease: ease-out]

play Reconstruction [fade, 400ms]
```
> `bar_colors` MUST be braces `{ }` of 4-tuples of numbers — the only form the
> parser accepts. Scheme tokens / parens are silently ignored.

### Task 6 — Scene 5 `Reconstruction` (gradual build-up via partial sums)
Fix Critical-ish #7: `reconstructed` was identical to `original`. Since `func`
is frozen, show gradual reconstruction with three stacked partial-sum curves,
crossfading opacity so each replaces the previous while the faint original
stays underneath as the target. This is the standard "build the sum harmonic by
harmonic" pedagogy.
```amx
# Reconstruction
config { colorscheme: "editorial-dark" }

step4_title: Text, text: "4. Reconstruction: Sum = Original", font_size: 40,
  color: text.primary, anchor: scene.top, offset: (0, 60)

eqn2: Typst, content: "$hat(x)(t) = sum_(k=0)^(N-1) X_k e^((2 pi i k t) / N)$",
  font_size: 22, color: accent.primary, anchor: scene.center, offset: (0, -200)

reconstruct_graph: Graph, at: (640, 400), size: (900, 300),
  x_domain: (0, 4), y_domain: (-3, 3) {
  original: PlotCurve, kind: "cartesian",
    func: (t) => sin(2 * pi * 2 * t) + 0.55 * sin(2 * pi * 5 * t) + 0.3 * sin(2 * pi * 9 * t),
    color: text.muted, opacity: 0.5, stroke_width: 2
  sum_1: PlotCurve, kind: "cartesian", func: (t) => sin(2 * pi * 2 * t),
    color: accent.danger, stroke_width: 3, opacity: 0.0
  sum_2: PlotCurve, kind: "cartesian",
    func: (t) => sin(2 * pi * 2 * t) + 0.55 * sin(2 * pi * 5 * t),
    color: accent.warning, stroke_width: 3, opacity: 0.0
  sum_3: PlotCurve, kind: "cartesian",
    func: (t) => sin(2 * pi * 2 * t) + 0.55 * sin(2 * pi * 5 * t) + 0.3 * sin(2 * pi * 9 * t),
    color: accent.success, stroke_width: 3, opacity: 0.0
}

reconstruct_label: Text,
  text: "Adding harmonics one by one recovers the original signal exactly.",
  font_size: 18, color: text.secondary, anchor: scene.center, offset: (0, 250)

#0.5s
fade-in step4_title [500ms, ease: ease-out]

#1.2s
fade-in eqn2 [500ms]

#2s
fade-in reconstruct_graph [800ms, ease: ease-out]

#3.2s
sum_1.opacity = 1.0 [600ms, ease: ease-out]

#4.6s
sum_1.opacity = 0.0 [300ms]
sum_2.opacity = 1.0 [600ms, ease: ease-out]

#6.0s
sum_2.opacity = 0.0 [300ms]
sum_3.opacity = 1.0 [600ms, ease: ease-out]

#7.4s
fade-in reconstruct_label [500ms]
```
> `sum_3` ends overlapping the faint `original` exactly — visually proving
> "sum = original." `opacity` is assignable/animated on PlotCurve (SizedActor),
> so the crossfade works despite `func` being frozen.
> Alternative (not used): `stroke_progress` (assignable on stroke paths) to
> "draw" each partial sum in. Opacity crossfade is simpler and clearer here.

### Task 7 — Remove the trailing "Summary annotations" comment block
Delete the final `// MISSING: No callout/annotation primitive …` block entirely
(requirement: remove design-note comments). Do NOT add `Callout` actors — the
5 scenes already convey the concept; adding callouts would over-engineer.
(The `Callout` primitive exists, but isn't needed for clarity here.)

### Task 8 — Validate
- `cargo check -p animatix` → 0 errors.
- `cargo test -p animatix --no-fail-fast` → all green (the existing
  `equation_container_builds_with_fragment_children` and BarChart tests should
  still pass; no source changes, only an example file).
- Load `examples/fft_explain.amx` in the GUI / run the renderer: confirm all 5
  scenes advance (no `OrphanScene`), bars are red/green/amber (not default
  white), titles are on-screen, equation text is muted, reconstruction shows
  the stepwise build-up, and the TimeDomain playhead sweeps.
- Check the build produces zero warnings of type `conflicting-position-binding`,
  `ignored-offset`, `orphan-scene`, `play-target-not-found`.

## Files to touch
- `examples/fft_explain.amx` — full rewrite (the only file changed).

## Optional doc follow-ups (NOT part of this plan; file separate issues)
- `docs/primitives.md` BarChart example shows `bar_colors: (accent.danger, …)`
  which is silently ignored by the parser. Should read
  `bar_colors: {(1.0,0.46,0.54,1.0), …}` (or the parser should accept scheme
  tokens — a code change, out of scope here).
- `docs/spec.md:780` references `examples/fft_explain.amx` as the BarChart
  exemplar; keep that pointer valid (it still is after rewrite).

## Risks
- **Playhead coord math (Task 3):** the `always` expression assumes the graph's
  pixel bounds are exactly `[190, 1090]` × `[230, 530]` (center 640,380; size
  900×300). If the renderer applies padding/insets inside the Graph, the
  playhead may drift slightly from the curve. Mitigation: it's an auxiliary
  visual; slight drift is acceptable. If it looks wrong in preview, drop the
  `always` block + `playhead` actor (the scene still works without it).
- **`sum_1/2/3` initial `opacity: 0.0` in a declaration:** confirm the engine
  treats declaration-time `opacity` as the t=0 value (it does — declarations
  seed the track baseline). The explicit `sum_1.opacity = 1.0 [600ms]` at
  #3.2s animates from that 0.0 baseline. If a declaration `opacity: 0.0` is
  instead treated as "always invisible," fall back to `fade-in sum_1 [600ms]`
  / `fade-out` actions (which animate opacity 0↔1) instead of bare assignments.
- **Highlight persistence:** `highlight` animates `highlight_opacity` 0→1 and
  stays at 1. The plan pairs each new highlight with `unhighlight` on the
  previous fragment so only one fragment is lit at a time. Verify `unhighlight`
  is registered (it is — `actions/highlight.rs` `Unhighlight`).
- **Scene timing totals:** TitleCard ~2.9s, TimeDomain ~3.3s, Decomposition
  ~8.4s, Spectrum ~3.2s, Reconstruction ~8.4s. These are scene-local `t`
  windows; `play [fade, 400ms]` bridges them. Total ~26s. Acceptable for an
  educational reel; trim Decomposition/Reconstruction holds if too long.
- **`bar_colors` braces vs parens:** any slip back to `(...)` or scheme tokens
  silently reverts to default-color bars with no error. Re-verify visually.

## Answers to the four questions

**1. Exact sequence of changes?**
Tasks 1–8 above: strip design-note comments → fix TitleCard positioning + add
`play` → TimeDomain positioning + optional playhead → Decomposition positioning
(keep Equation color + highlight sync) → Spectrum `bar_colors`→RGBA + delete
hardcoded labels → Reconstruction partial-sum crossfade → delete trailing
comment block → validate with `cargo check`/`test` + visual run.

**2. Add `always` blocks for reactive waveforms? Where?**
A *reactive waveform* (scrolling/phase-rotating curve) is **not feasible** —
`func` and `x_domain` are `BuildTimeOnly`/non-assignable, so a `PlotCurve`'s
shape is frozen at build time. The only feasible reactive uses touch assignable
props (at/opacity/size/color). Recommend exactly ONE small `always` block, in
`TimeDomain` only: a sweeping sampling playhead (`Rect`) whose `at` x is driven
by `t % 4.0`, reinforcing "FFT samples in time." Keep it optional/low-risk; if
preview alignment is off, drop it. Do not add `always` to the other scenes —
their content is inherently static (fixed curves, fixed bar data) and adding
pulses would be noise, not pedagogy.

**3. How to fix the reconstructed curve (gradual reconstruction)?**
Because `func` can't change per-frame, use **three stacked partial-sum
`PlotCurve`s** with distinct fixed funcs (`sum_1` = 2Hz only; `sum_2` = 2Hz+5Hz;
`sum_3` = all three = original) and crossfade `opacity` between them in
sequence, while the faint full `original` stays underneath as the target.
`sum_3` ends exactly overlapping `original` → visual proof of invertibility.
This is the standard manim-style "build the sum harmonic by harmonic" and needs
no language changes. (Alternative: `stroke_progress` draw-in; rejected as less
clear than the crossfade.)

**4. What `play` transitions/timing make sense pedagogically?**
- `TitleCard → TimeDomain [fade, 400ms]` — calm handoff into the topic.
- `TimeDomain → Decomposition [fade, 400ms]` — same signal, new view; fade
  preserves continuity.
- `Decomposition → Spectrum [wipe-left, 400ms]` — a *wipe* signals a representational
  shift (time domain → frequency domain), which is the conceptual hinge of FFT.
- `Spectrum → Reconstruction [fade, 400ms]` — back to a time-domain view; fade
  keeps the comparison readable.
Durations are uniform 400ms for rhythm; only the transition *type* varies to
mark the one big representational leap. Don't use `cut` (too jarring for
teaching) or long fades (slows the pace).
