# FFT Thought Experiment — Language Design Gap Analysis

**Scenario:** An educational animation explaining the Fast Fourier Transform (FFT).
**File:** `examples/fft_explain.amx`
**Purpose:** Stress-test the current `.amx` language surface by authoring a real
multi-scene data-visualization animation with plotting, equations, and animation.

---

## 1. Scenario Narrative

A 5-act educational animation:

1. **Title Card** — "Understanding the FFT"
2. **Time-Domain Signal** — Show a composite waveform
3. **Decomposition** — Break it into individual sine waves (2 Hz, 5 Hz, 9 Hz)
4. **Frequency Spectrum** — Bar chart showing magnitude per frequency bin
5. **Reconstruction** — Show that the sum of components reproduces the original

---

## 2. Full .amx Draft with Gap Annotations

The draft lives at `examples/fft_explain.amx`. Below is a summary of every
`// MISSING:` comment, organized by severity.

---

## 3. Gap Analysis

### CRITICAL Gaps — Blockers for even a moderately complex educational animation

#### G1. Bar chart / column / histogram primitive — Resolved

A `BarChart` primitive has been implemented (`crates/animatix/src/primitives/bar_chart.rs`).
It accepts `data: ((key, value), ...)` tuples and produces filled-rectangle
bar paths with optional baseline axis, per-bar colors, and auto-distributed
spacing. Supports standalone (pixel coords) and `Graph`-child (math coords) modes.

**Syntax used in the FFT example:**
```animatix
spectrum: BarChart,
  data: (("2 Hz", 1.0), ("5 Hz", 0.55), ("9 Hz", 0.3)),
  size: (600, 260),
  bar_colors: (accent.danger, accent.success, accent.warning),
  show_axis: true,
  at: (640, 420)
```

This replaces ~30 lines of manual `Rect`/`Text` actors.

#### G2. No data-driven / programmatic actor generation

**What's needed:** A way to generate actors from runtime data or compile-time
lists. `for item in items { ... }` exists but only expands compile-time
structure from static values. You cannot write:

```animatix
let frequencies = (2, 5, 9)
let magnitudes = (1.0, 0.55, 0.3)
for i in (0, 1, 2) {
  bar_{i}: Rect, size: (60, magnitudes[i] * 180), at: ...  // not valid
}
```

**Why it's critical:** For any data visualization with more than a handful of
data points, manual actor declarations don't scale. A 32-bin spectrum would
require 32+ actor declarations + 32+ keyframe blocks. This makes the language
unusable for real data visualization.

**Workaround:** Manual repetition for small datasets. File it under "toy usage
only."

#### G3. No incremental / draw animation for curves

**What's needed:** The ability to "draw" a `PlotCurve` incrementally from left
to right (a tracing effect). This is a standard educational animation technique
— watching the curve being traced as time progresses.

**Why it's critical:** The reveal of the time-domain signal is much more
intuitive when you see it being traced from left to right. A `fade-in` of the
whole curve at once loses the temporal dimension that the animation medium is
supposed to exploit.

**Workaround:** Use `wipe-in` or `reveal-in` actions (designed for shapes, not
curves). Or animate the `x_domain` to create a sliding window effect. Neither
is natural.

#### G4. No step-by-step / interactive progression control

**What's needed:** A way to pause the timeline and wait for user interaction
(click to continue), or a declarative "step" construct that segments the
timeline into user-paced chunks.

**Why it's critical:** Educational animations inherently need to let the viewer
absorb each concept before moving on. A purely time-driven linear animation
(player hits "play" and watches 30 seconds) is passive and doesn't support the
learner's need to pause, rewind, and examine.

**Workaround:** Multi-scene composition + manual scene transitions. The user can
click to advance scenes in the GUI. But within a scene, there's no pause
mechanism.

#### G5. Typst equation animation / progressive highlighting — Resolved

An `Equation` container primitive with `Fragment` children has been implemented.
Each Fragment represents a named semantic segment of the equation that can be
independently highlighted via a colored rectangle overlay with blend mode
(Difference/Exclusion). The `highlight`/`unhighlight` actions provide semantic
shortcuts for animating fragment highlighting.

**Syntax used in the FFT example:**
```animatix
equation: Equation, font_size: 22, at: (640, 360) {
  f1: Fragment, content: "sin(2 pi dot 2t)"
  mid: Fragment, content: " + "
  f2: Fragment, content: "sin(2 pi dot 5t)"
}

#2s
  highlight equation.f1 [color: white, blend: difference, 800ms]
```

#### G6. No animation of PlotCurve parameters at runtime

**What's needed:** The ability to animate the `func` closure itself, or animate
parameters captured by the closure. Currently, `func` is set at declaration time
and cannot be changed — only the entire curve can be replaced via
re-declaration.

**Why it's critical:** Many educational animations need to show, for example,
a frequency sweep, or gradually add sine waves to a composite signal. These
require either live parameter changes or multiple curve declarations with
cross-fades.

**Workaround:** One `PlotCurve` per frequency, revealed via fade-in at
staggered keyframe times. Lots of duplication; no smooth morphing of the curve
shape.

---

### NICE-TO-HAVE Gaps — Would significantly improve ergonomics

#### G7. Annotations / callout primitive

**What's needed:** A `Callout` primitive that points an arrow + text label at
a specific actor or coordinate.

```animatix
callout: Callout, target: sine_2hz, text: "2 Hz component",
  color: accent.danger, at: (50, -30)
```

**Why it's nice:** FFT explanations require frequent pointing — "this frequency,"
"this bar," "this term in the equation." Manual arrows + text labels work but
are tedious to position and don't track their targets automatically.

**Workaround:** `Arrow` + `Text` actor pairs, manually positioned and updated.

#### G8. Legend primitive

**What's needed:** An auto-generated legend that maps colors to labels based on
actors in a container or graph.

**Why it's nice:** When showing multiple frequency components in the same graph
(Scene 3), a legend would automatically explain which color is which frequency.
Instead we have to place inline `Text` labels.

**Workaround:** Inline text labels placed at the end of each curve, or a manual
legend block with colored `Rect` swatches + `Text` labels.

#### G9. Grid/axis customization on Graph

**What's needed:** Control over tick marks, tick labels, grid line style,
subdivisions, and axis labels exposed as properties.

**Why it's nice:** The frequency spectrum (Scene 4) needs labeled axes ("Hz"
for x, "Magnitude" for y). The time-domain graph could benefit from labeled
tick marks every 0.5s.

**Workaround:** Manual axis labels via `Text` + `Line` actors.

#### G10. `draw-in` / trace animation for PlotCurve

**What's needed:** A `draw-in` equivalent for curve primitives that traces the
curve from start to end over the animation duration. (Related to G3.)

**Why it's nice:** This is the single most common education animation trope
for math topics. Its absence is noticeable.

**Workaround:** See G3 workaround.

#### G11. Color auto-assignment with semantic cycling

**What's needed:** When multiple actors of the same kind are created, `color: auto`
should cycle through a deterministic set of visually distinct colors (like
Matplotlib's default cycle).

**Why it's nice:** For the three frequency curves, I had to manually choose
`accent.danger`, `accent.success`, `accent.warning`. A color cycle would
automatically assign `auto` → `accent.primary`, `auto` → `accent.secondary`,
`auto` → `accent.tertiary`, etc.

**Workaround:** Manual color choices. The `auto` pool exists but is not a
cycle — it assigns one auto color per primitive type, not one per instance.

#### G12. Screen-space arrow annotations with auto-layout

**What's needed:** An `Arrow` that can reference actor labels or scene
coordinates and auto-positions itself (e.g., "point from this text to this bar").

**Why it's nice:** Connecting a frequency label to its bar in the spectrum
currently requires manual math: figure out where the bar is, figure out where
the label is, compute arrow endpoints. If either moves, the arrow breaks.

**Workaround:** Manual `Arrow` with hardcoded `from`/`to`.

#### G13. Easing on text property changes

**What's needed:** Smooth interpolation for text content changes (font_size,
color). Currently, color changes work; text content is instantaneous.

**Why it's nice:** In an educational setting, gradually changing a label (e.g.,
"2 Hz" fading into "5 Hz") is a nice touch.

**Workaround:** Multiple `Text` actors at the same position with staggered
fade-in/fade-out.

#### G14. Morphing/closing animation for fade transitions

**What's needed:** The ability to animate a scene's actors out (or the next
scene's actors in) using a custom exit action before the transition.

**Why it's nice:** The hard scene cuts between acts in our FFT animation feel
abrupt. A fade-out of the old content + fade-in of the new would be smoother,
but the `play SceneName [fade, 300ms]` transition applies uniformly to the
entire scene — you can't animate individual actors before the transition fires.

**Workaround:** Use `fade-out` actions at the end of each scene, timed to
coincide with the play transition.

#### G15. No function composition / helper abstraction

**What's needed:** The ability to define a named function or constant that can
be reused across closures and expressions.

```animatix
let F = (x) => sin(2 * pi * 2 * x) + 0.55 * sin(2 * pi * 5 * x)
// Then use F(t) in PlotCurve func
```

**Why it's nice:** In Scene 2, the composite signal function is defined inline
in the `PlotCurve` declaration. If we wanted to show the same function in
multiple graphs or reuse it, we'd have to copy-paste the closure.

**Workaround:** Inline duplication of the closure.

#### G16. No way to highlight / emphasize an actor programmatically

**What's needed:** An action or property that temporarily changes an actor's
appearance (e.g., glow, outline pulse, color flash) as a visual cue during
educational step-throughs.

**Why it's nice:** When explaining "this is the 2 Hz component," it would be
ideal to pulse/highlight the corresponding sine curve and its spectrum bar
simultaneously.

**Workaround:** Manual `pulse` action invocations at the right keyframe times.

---

## 4. Summary Table

| # | Gap | Severity | Workaround Viability | Category |
|---|-----|----------|---------------------|----------|
| G1 | Bar chart primitive | Resolved | N/A — implemented | Primitives |
| G2 | Programmatic actor generation | Critical | Low (manual repetition) | Language |
| G3 | Draw-in for curves | Critical | Medium (fade + domain trick) | Animation |
| G4 | Interactive step control | Critical | Medium (GUI scene navigation) | Runtime |
| G5 | Equation highlighting | Resolved | N/A — implemented | Rendering |
| G6 | Runtime curve parameter animation | Critical | Medium (re-declaration) | Animation |
| G7 | Callout/annotation primitive | Nice | High (Arrow + Text) | Primitives |
| G8 | Legend primitive | Nice | High (manual swatches) | Primitives |
| G9 | Grid/axis customization | Nice | High (manual axes) | Plotting |
| G10 | Curve trace animation | Nice | Medium (see G3) | Animation |
| G11 | Auto color cycling | Nice | High (manual colors) | Color |
| G12 | Auto-arrow layout | Nice | Medium (manual arrows) | Layout |
| G13 | Text property easing | Nice | High (multiple actors) | Animation |
| G14 | Per-actor exit before transition | Nice | High (manual fade-out) | Composition |
| G15 | Named function abstraction | Nice | Medium (inline duplication) | Language |
| G16 | Highlight/emphasis action | Nice | High (manual pulse) | Actions |

---

## 5. Key Insight

The FFT scenario reveals that Animatix is **strong for decorative/animated
graphics** (shapes, layouts, morphing, effects) but **weak for data-driven
educational content** (plotting, bar charts, programmatic generation,
step-by-step control). The critical gaps (G1–G6) all involve creating structured
visualizations from computed data, which is the core requirement of any
educational math/science animation tool.

The first gap — **BarChart primitive** — has been resolved. The FFT spectrum
scene now uses a single `BarChart` declaration instead of ~30 lines of manual
`Rect`/`Text` actors. The next most impactful addition would be a **data-driven
component system** where actors can be generated from list data.

---

## 6. Comparison with Manim (the Python library for math animation)

Manim's power comes from:

1. **Programmatic actor generation** — `VGroup(*[Square() for _ in range(10)])`
2. **Coordinate-aware positioning** — `next_to()`, `align_to()`, `shift()`
3. **Animation composition** — `AnimationGroup`, `LaggedStart`, `succession()`
4. **Interactive step control** — `self.wait()`, `self.play()`, `self.next_slide()`

Animatix trades programmatic flexibility for **declarative simplicity and
deterministic playback**. This is excellent for authored animations but creates
a sharp ceiling for educational content. The language design question:
**should Animatix grow toward Manim's expressiveness, or stay declarative and
add higher-level primitives (like a built-in FFT visualization primitive)?**
