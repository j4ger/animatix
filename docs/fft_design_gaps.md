# FFT Explain — Language Design Gap Analysis

## Scenario

A visual explanation of the Fast Fourier Transform in Animatix, demonstrating:
1. **Time Domain** — composite signal as sum of sine waves (3 Hz, 7 Hz, 11 Hz)
2. **DFT Correlation** — probing the signal with reference frequencies to detect components
3. **Frequency Spectrum** — resulting magnitude spectrum showing peaks at component frequencies

The full demo is at `examples/24_fft_explain.amx`. The following gaps were identified during the design exercise, categorized as **critical** (blocks the outcome) or **nice-to-have** (adds polish).

---

## Critical Gaps

### 1. No Primitive for Bar/Histogram Charts

Without a dedicated `BarChart` or `Histogram` primitive, the frequency spectrum (Scene 3) must be built from individual `Rect` actors — one per frequency bin. This is:

- **Verbose:** 8 frequency bins × 4 actors each (bar + label + decoration) = 32+ manual declarations
- **Brittle:** Adding a bin means copy-pasting 4 blocks and adjusting coordinates
- **Not data-driven:** You cannot say `let data = (0.0, 1.0, 0.1, 0.7, ...)` and bind it to bars

**Wanted:** A `BarChart` / `Histogram` primitive (or generic `Chart` with mark bindings):

```animatix
spectrum: BarChart, data: (0.0, 1.0, 0.1, 0.7, 0.05, 0.4, 0.03, 0.02),
  color: accent.primary, bar_width: 30, gap: 12, at: (640, 470)
```

Even better: a data-to-visual binding system where you declare a dataset and a mark mapping (inspired by Vega-Lite / Grammar of Graphics):

```animatix
let freqs = List { (1, 0.0), (3, 1.0), (5, 0.1), (7, 0.7), (9, 0.05), (11, 0.4), (13, 0.03), (15, 0.02) }
chart: Chart, data: freqs, x: col.0, y: col.1, mark: "bar", color: accent.primary
```

### 2. No Runtime Actor Generation from Data

The `for` loop provides compile-time structural expansion, but it cannot generate actor declarations with computed labels at compile time. Every spectrum bar and its label must be declared manually:

```animatix
// What doesn't work:
for f in (1, 3, 5, 7, 9, 11, 13, 15) {
  bar{f}: Rect, size: (36, heights[f]), at: base_x + f * step
}
```

**Wanted:** Compile-time `for` that can generate actor declarations with interpolated labels:

```animatix
for (i, freq) in enumerate((1, 3, 5, 7, 9, 11, 13, 15)) {
  bar_{i}: Rect, size: (36, magnitudes[freq]), at: (310 + i * 90, 610)
}
```

This would make data-driven visualizations (bar charts, scatter plots, heatmap grids) concise instead of requiring manual unrolling.

### 3. No Data Array/Table Display

The DFT outputs arrays of complex numbers. There is no way to display:

- A table of (frequency, magnitude, phase) triples
- A matrix (for the butterfly diagram)
- A scrolling list of values

**Wanted:** A `Table` or `DataGrid` primitive that maps a list of tuples to a formatted grid:

```animatix
let dft_result = ((1.0, 0.0, 0.0), (3.0, 1.0, 0.0), (5.0, 0.1, 0.5), ...)
dft_table: Table, data: dft_result, columns: ("Freq", "Mag", "Phase"),
  header_color: text.primary, cell_color: text.secondary
```

### 4. No Interactive Controls

An FFT explanation begs for interactivity:

- A slider to change which frequency probes the signal
- A toggle to show/hide individual sine components
- A slider to adjust the number of samples (N-point DFT vs FFT)
- A play/pause on the sweeping probe animation

**Wanted:** Interactive primitives (`Slider`, `Toggle`, `Button`) that feed values into `always` blocks:

```animatix
probe_freq: Slider, range: (1.0, 20.0), default: 3.0, at: (100, 100)

always {
  corr_bar.size = (40.0, correlation(signal, probe_freq.value))
}
```

Without this, the demo is a fixed animation — the viewer cannot explore the concept interactively.

### 5. Complex Number Type & Visualization Helpers

The FFT fundamentally deals with complex numbers (real + imaginary parts, magnitude, phase, Euler's formula). Animatix has no `Complex` type, nor any built-in helpers for:

- Displaying `a + bi` notation
- Computing magnitude/phase from (re, im)
- Visualizing the complex plane (Argand diagram with rotating phasors)
- Euler's formula: `e^(iθ) = cos(θ) + i·sin(θ)`

This forced the demo to avoid complex numbers entirely, showing only the sine correlation (real part) aspect of the DFT.

**Wanted:**

```animatix
let c = Complex(0.707, 0.707)
let mag = c.magnitude()  // 1.0
let phase = c.phase()    // 0.785 rad
```

And a primitive `ComplexPlane` / `ArgandDiagram` for visualizations:

```animatix
plane: ComplexPlane, x_domain: (-2, 2), y_domain: (-2, 2), size: (300, 300)
  // Shows real axis (Re), imaginary axis (Im)
  // Points, vectors, and rotating phasors
```

---

## Nice-to-Have Gaps

### 6. Text/Formula Recompilation at Render Time

The spec notes: "Property assignment for `text`/`latex`/`math`/`code` stores the value but does not trigger re-compilation of text paths at render time." This means:

- `corr_freq_val.text = format("f = {sweep_freq} Hz", sweep_freq)` stores the string but may not re-render the glyph
- Animated formula evolution is unreliable — the DFT math equation (`∫ f(t)·e^(-iωt) dt`) cannot smoothly transition between states
- Workaround: pre-declare all text states and fade between them (works for ~2-3 states, impractical for many)

**Wanted:** Full render-time text recompilation so `always` blocks can update text content per frame.

### 7. Formula/Equation Display (Typst with Animated Parameters)

The `Typst` primitive is supported, but its content is also subject to the recompilation limit. For an FFT explanation, being able to show the DFT formula with animated highlights would be powerful:

```animatix
dft_formula: Typst, content: "X[k] = sum_(n=0)^(N-1) x[n] e^(-i 2 pi k n / N)",
  at: (640, 100), font_size: 24
```

But animating `content` at runtime won't re-typeset. And highlighting individual terms (like highlighting `x[n]` or `e^(-i 2 pi k n / N)`) with different colors over time is not possible.

### 8. Data-Driven Color Scales

For spectrum bars, you'd ideally want a color gradient (blue → cyan → green → yellow → red) proportional to magnitude. Currently:

- Each bar must have a hardcoded `color:` at declaration time
- There's no color scale / interpolate function that maps `[0, 1]` to a gradient
- `lerp(a, b, t)` exists but works on numbers, not colors

**Wanted:** Color interpolation:

```animatix
let scale = ColorScale("viridis")
bar.color = scale(magnitude)
// Or: color_lerp(blue, red, t)
```

### 9. Animated Number / Digit Display

Displaying a changing numeric value (like `0.72` for the correlation strength) without stuttering or flashing. Currently:

- `corr_val_text.text = format("{strength}", strength)` writes the whole string
- There's no number tweening — values jump
- No fixed-width digit display for odometer-style counting

**Wanted:** A `DigitCounter` or number display with smooth transitions:

```animatix
corr_value: Number, value: correlation_signal, format: "0.00",
  color: text.primary, at: (940, 540)
```

### 10. Animated Vector/Arrow for Probes and Signals

The DFT correlation (Scene 2) uses a moving vertical line as a "probe". A more intuitive visualization would be:

- An arrow pointing from the signal to the spectrum
- A connecting line that traces the correlation value over time
- Visual "ripples" or "energy" flowing from time domain to frequency domain

**Wanted:** Better animation primitives for connections between visual elements:

- `Connect(line, from: actorA, to: actorB)` — a line that follows animated positions
- `Trace(path, source: PlotCurve, t: time)` — animated point along curve
- `Flow(particles, path: func, count: N)` — particle system along a path

### 11. Graph Child Coordinate Mapping for Non-Plot Primitives

Currently, only children of `Graph` with `at`/`position`/`from`/`to` get math-coordinate mapping. But you cannot place a `Rect`, `Text`, or `Ellipse` inside a `Graph` with mapped coordinates. This means:

- You can't add labeled data points (e.g., a dot + label at a specific (x, y) on the sine wave)
- Annotation arrows inside the graph need manual pixel coordinates
- Visual callouts like "peak at x=1.57" require compensating for the graph's transform

**Wanted:** All primitives inside a `Graph` should support coordinate mapping:

```animatix
graph: Graph, x_domain: (0, 6.283), y_domain: (-3, 3), size: (500, 300) {
  wave: PlotCurve, kind: "cartesian", func: (x) => sin(x)
  peak_dot: Ellipse, at: (1.5708, 1.0), size: (8, 8), color: red  // mapped!
  peak_label: Text, text: "peak", at: (1.5708, 1.2), color: text.secondary  // mapped!
}
```

### 12. Multi-Scene Shared State / Cross-Scene `always`

Each scene has its own `t` that resets to 0. There is no way to share state across scenes. For the FFT demo, this means:

- The correlation scene cannot "remember" the probe frequency when transitioning to the spectrum scene
- There's no global `t` or shared `always` block that persists across scene boundaries
- Workaround: duplicate logic per scene or use pre-calculation

**Wanted:** Optional cross-scene state via `export` on `always` variables, or a composition-level `always` block.

### 13. Axis Labels and Tick Marks

The `Graph` and `NumberPlane` primitives display the coordinate frame, but:

- No native axis labels ("Time (s)", "Amplitude", "Frequency (Hz)")
- No tick mark customization (value formatting, rotation, density)
- Must manually place `Text` actors for labels, which break if the graph is repositioned

### 14. Smooth Color Animation via `always`

Colors set via `always` jump instantly — they don't animate smoothly between frames. For the correlation bar (Scene 2), the color snaps between `accent.primary`, `accent.success`, `text.muted` as the probe sweeps. A smooth color transition:

```animatix
always {
  corr_bar.color = color_lerp(text.muted, accent.primary, strength)
}
```

...would look significantly more polished.

---

## Summary

| # | Gap | Category | Impact |
|---|---|---|---|
| 1 | No bar chart primitive | Critical | Makes spectrum visualization ~10× more verbose |
| 2 | No runtime actor generation | Critical | Prevents data-driven visualizations entirely |
| 3 | No data array/table display | Critical | Can't show DFT output as numbers |
| 4 | No interactive controls | Critical | Animation is fixed, no exploration possible |
| 5 | No complex number type | Critical | Can't show Euler's formula or complex plane |
| 6 | Text recompilation limits | Critical | Animated labels/values may not render |
| 7 | Formula display (Typst) | Nice-to-have | Would make equations beautiful, not just possible |
| 8 | Data-driven color scales | Nice-to-have | Adds polish to bar charts and heatmaps |
| 9 | Animated number display | Nice-to-have | Smooth number transitions for correlation display |
| 10 | Connection/trace primitives | Nice-to-have | Better visual linking between time/frequency domains |
| 11 | Graph child coordinate mapping | Nice-to-have | Labels and annotations inside graph space |
| 12 | Cross-scene shared state | Nice-to-have | Avoid re-calculating per scene |
| 13 | Axis labels and ticks | Nice-to-have | Less manual text placement |
| 14 | Smooth color animation | Nice-to-have | Polished transitions in reactive blocks |

The top priorities for an educational/animation DSL are **#1** (data-driven charts), **#2** (runtime actor generation), and **#4** (interactivity) — without these, the language is limited to pre-authored, non-interactive animations, which underserves the educational use case.
