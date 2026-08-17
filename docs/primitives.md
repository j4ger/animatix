# Primitives

Quick reference for all Animatix primitives. For the full language specification, see [`spec.md`](spec.md). For the complete property registry, see [`properties.md`](properties.md).

---

# 1. Scene Primitives

## Text

**Properties:**
- `text`: String
- `font_size`: Number
- `color`: Color
- `at`: Tuple `(x, y)`

**Shorthand:** `title: "Hello World"` desugars to `Text { text: "Hello World", ... }`

**Example:**
```animatix
title: Text { text: "Hello World", font_size: 24, at: (640, 120) }
```

Timed `text` assignments cross-fade the source and target glyph sets instead of snapping at the midpoint; `Typst.content` follows the same behavior.

## Typst

Replaces the deprecated `Math` primitive.

**Properties:**
- `content`: String — Typst markup content (accepts `text`, `math`, `code`, `latex` for backward compatibility)
- `font_size`: Number
- `color`: Color
- `at`: Tuple `(x, y)`
- `highlight_color`: Color — color of whole-actor highlight overlay (default `white`)
- `highlight_opacity`: Number — opacity of highlight, 0 = hidden, 1 = full (default `0.0`)
- `highlight_blend`: String — blend mode: `difference`, `exclusion`, `normal`, `multiply`, `screen` (default `"difference"`)
- `highlight_padding`: Number — padding around actor bounding box (default `4.0`)
- `highlight_radius`: Number — corner radius of highlight rectangle (default `2.0`)

**Shorthand:** `$$ ... $$` desugars to a `Typst` actor with the content taken as raw text (unquoted). A label is required. Modifiers are supported.

```animatix
eq: $$ x^2 + y^2 $$                    // desugars to: eq: Typst, content: "x^2 + y^2"
eq: $$ x^2 $$ [2s, ease: bounce]       // with modifiers
```

**Actions:**
- `highlight target [color: C, blend: B, padding: P, radius: R, duration, ease]` — animate `highlight_opacity` from 0 to 1 (whole-actor highlight)
- `unhighlight target [duration, ease]` — animate `highlight_opacity` from current to 0

This is distinct from Equation+Fragment per-segment highlighting — a Typst actor is highlighted as a single unit.

**Example:**
```animatix
eq: Typst, content: "x^2 + 3", font_size: 18, at: (640, 360)

#2s
  highlight eq [color: yellow, blend: multiply, 800ms]

#4s
  unhighlight eq [400ms]
```

## Code

Renders via the text-path pipeline (no syntax highlighting in v1).

**Properties:**
- `code`: String
- `font_size`: Number
- `color`: Color
- `at`: Tuple `(x, y)`
- `anchor`: Scene anchor
- `offset`: Tuple `(x, y)`

**Example:**
```animatix
snippet: Code { code: "let velocity = x + 1", font_size: 28, at: (640, 360) }
```

## Svg

**Properties:**
- `url`: String
- `scale`: Number
- `at`: Tuple `(x, y)` or scene-relative percent tuple `(72%, 38%)`
- `anchor`: Scene anchor
- `offset`: Tuple `(x, y)`

**Example:**
```animatix
icon: Svg { url: "examples/assets/animatix-mark.svg", scale: 1.5, at: (640, 600) }
```

Note: Missing files or invalid SVG contents report build diagnostics. Source changes require re-declaration at a keyframe (assignment not yet supported).

## Image

**Properties:**
- `url`: String
- `at`: Tuple `(x, y)` or percent tuple `(30%, 38%)`
- `anchor`: Scene anchor
- `offset`: Tuple `(x, y)`
- `size`: Optional tuple `(width, height)` — defaults to intrinsic pixel size

**Example:**
```animatix
photo: Image { url: "examples/assets/checker.png", at: (640, 360), size: (180, 180) }
```

Note: Missing files report build diagnostics. Source changes are discrete (crossfade requires manual opacity layering).

## Rect

**Properties:**
- `size`: Tuple `(width, height)` — general rectangle dimensions
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width`: Number
- `fill_opacity`: Number
- `stroke_progress`: Number
- `at`: Tuple `(x, y)`

**Examples:**
```animatix
# Rectangle with explicit size
r: Rect, size: (160, 80), color: blue, at: (400, 300)

# Square
sq: Rect, size: (120, 120), color: green, at: (400, 500)
```

## Ellipse

**Properties:**
- `size`: Tuple `(width, height)` — bounding box dimensions, converted to `radius_x = width / 2`, `radius_y = height / 2`
- `radius_x`: Number — horizontal radius
- `radius_y`: Number — vertical radius
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width`: Number
- `fill_opacity`: Number
- `stroke_progress`: Number
- `at`: Tuple `(x, y)`

**Examples:**
```animatix
# Circle
c: Ellipse, size: (100, 100), color: red, at: (200, 200)

# Ellipse with explicit radii
halo: Ellipse, radius_x: 90, radius_y: 40, color: cyan, at: (640, 360)

# Ellipse via size tuple
halo: Ellipse, size: (180, 80), color: cyan, at: (640, 360)

# Dot
dot: Ellipse, size: (6, 6), color: gold, at: (320, 240)
```

## Line

Stroke-oriented — no fill property.

**Properties:**
- `from`: Tuple `(x, y)` in local actor coordinates
- `to`: Tuple `(x, y)` in local actor coordinates
- `stroke` / `stroke_color`: Color
- `stroke_width`: Number
- `at`: Tuple `(x, y)`

**Examples:**
```animatix
# Plain line
axis: Line, from: (-120, 0), to: (120, 0), stroke: white, stroke_width: 4, at: (640, 360)
```

## Polygon

**Properties:**
- `points`: Tuple/list of point tuples — explicit vertex list
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width`: Number
- `fill_opacity`: Number
- `stroke_progress`: Number
- `at`: Tuple `(x, y)`

**Examples:**
```animatix
# Explicit vertex list
badge: Polygon, points: {(-80, 0), (0, -70), (90, 0), (0, 80)}, color: cyan, at: (640, 360)

# Regular hexagon
hex: Polygon, points: {(-70, 0), (-35, -60), (35, -60), (70, 0), (35, 60), (-35, 60)}, color: cyan, at: (640, 360)
```

## Path

Uses structured commands (`move_to(...)`, `line_to(...)`, `quad_to(...)`, `curve_to(...)`, `close()`).

**Properties:**
- `commands`: Tuple/list of path commands
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width`: Number
- `fill_opacity`: Number
- `stroke_progress`: Number
- `at`: Tuple `(x, y)`

**Example:**
```animatix
guide: Path, commands: {
  move_to(-120, 0),
  line_to(-40, -80),
  curve_to(20, -120, 80, 40, 140, -10),
  close()
}, stroke: white, stroke_width: 4, at: (640, 360)
```

---

# 2. Graph Primitives

## Graph

A coordinate container that maps child actor positions to math coordinates. Use when you need to plot data (curves, vectors, etc.) with automatic coordinate mapping.

**Properties:**
- `x_domain`: Tuple `(min, max)`
- `y_domain`: Tuple `(min, max)`
- `size`: Tuple `(width, height)`
- `at`: Tuple `(x, y)`
- `grid`: Boolean — draw grid lines at regular intervals
- `ticks`: Boolean — draw tick marks on axes
- `tick_labels`: `"auto" | "true" | "false" | "x" | "y" | "both"` — draw numeric labels on axes

## PlotCurve

Single-stroke curve plot. The `kind` property selects the sampling strategy.

**Properties:**
- `kind`: `"cartesian" | "polar" | "parametric" | "implicit"`
- `func`: Closure — signature depends on `kind`
  - `"cartesian"`: `(x) => y`
  - `"polar"`: `(theta) => r`
  - `"parametric"`: `(t) => (x, y)`
  - `"implicit"`: `(x, y) => scalar`
- `x_domain`: Tuple `(min, max)`
- `y_domain`: Tuple `(min, max)`
- `t_domain`: Tuple `(min, max)` — used by `"polar"` and `"parametric"`
- `color` / `stroke`: Color
- `width` / `stroke_width`: Number
- `tolerance`: Number — adaptive subdivision threshold
- `max_depth`: Number — max recursion depth for adaptive sampling
- `resolution`: Number — sampling grid resolution for `"implicit"`

**Function Transitions**

All plot primitives support animated transitions between functions using timed `func` assignments:

```animatix
curve: PlotCurve, kind: "cartesian", func: (x) => sin(x), stroke: accent.primary

#2s
curve.func = (x) => cos(x) [1s, ease: ease-in-out]
```

The default transition blends evaluated outputs at each sample point: `y = lerp(from(x), to(x), progress)`. This works for cartesian, polar, parametric, and implicit `PlotCurve`, plus `VectorField`, `Heatmap`, and `ContourSet` (their scalar/vector fields are blended per sample).

To cross-fade the two rendered plot outputs instead of blending function values, add `blend: opacity` to the assignment:

```animatix
curve.func = (x) => cos(x) [1s, blend: opacity]
```

`blend: output` is the default and can be written explicitly.

**Overlapping transitions:** If a new transition starts before the previous completes, the system freezes the current blend state and chains to the new target:

```animatix
#1s
curve.func = (x) => cos(x) [2s]  // 1s to 3s

#2s
curve.func = (x) => x^2 [1s]  // freezes at 50%, chains to x^2
```

See `examples/data/24_plot_transitions.amx` for demonstrations.

## VectorField

Grid-sampled vector field rendered as arrows.

**Properties:**
- `func`: Closure `(x, y) => (dx, dy)` — returns a vector at each point
- `density`: Number — grid resolution (default 16)
- `x_domain`: Tuple `(min, max)`
- `y_domain`: Tuple `(min, max)`
- `size`: Tuple `(width, height)`
- `color` / `stroke`: Color

## Heatmap

Pixel-level scalar field visualization using colored rectangles.

**Properties:**
- `func`: Closure `(x, y) => scalar`
- `resolution`: Number — grid resolution (default 64)
- `x_domain`: Tuple `(min, max)`
- `y_domain`: Tuple `(min, max)`
- `size`: Tuple `(width, height)`
- `color`: Color — used as the "hot" color (alpha varies by scalar value)

## ContourSet

Multiple level-set curves for a scalar function.

**Properties:**
- `func`: Closure `(x, y) => scalar`
- `levels`: Tuple of numbers — e.g. `(-2, 0, 2)`
- `resolution`: Number — sampling grid resolution (default 96)
- `x_domain`: Tuple `(min, max)`
- `y_domain`: Tuple `(min, max)`
- `size`: Tuple `(width, height)`
- `color` / `stroke`: Color

## NumberPlane

A standalone visual coordinate plane with auto-generated axes, grid lines, and tick marks. Use when you need a visual background grid/axes without hosting child plots.

**Properties:**
- `x_domain`: Tuple `(min, max)` — visible x-axis range
- `y_domain`: Tuple `(min, max)` — visible y-axis range
- `x_range`: Tuple `(min, max, step)` — grid/ticks x placement range and interval
- `y_range`: Tuple `(min, max, step)` — grid/ticks y placement range and interval
- `size`: Tuple `(width, height)`
- `at`: Tuple `(x, y)`
- `stroke` / `stroke_color`: Color — axis and grid color

Grid lines are drawn at each step interval within the specified range. Axes (horizontal at y=0, vertical at x=0) are drawn with thicker strokes. Tick marks appear at each step interval on both axes.

**Example:**
```animatix
plane: NumberPlane,
  x_domain: (-6, 6), y_domain: (-6, 6),
  x_range: (-6, 6, 2), y_range: (-6, 6, 2),
  size: (400, 400), at: (640, 360)
```

**Example:**
```animatix
graph: Graph, x_domain: (-5, 5), y_domain: (-10, 30), size: (400, 400), at: (400, 300), grid: true, ticks: true {
  parabola: PlotCurve, kind: "cartesian", func: (x) => x^2 + 3, color: red, width: 2,
  rose: PlotCurve, kind: "polar", func: (t) => 3 * sin(4 * t), t_domain: (0, 6), stroke: green, width: 2
}

graph: Graph, x_domain: (-2, 2), y_domain: (-2, 2), size: (360, 360), at: (640, 360) {
  lissajous: PlotCurve, kind: "parametric", func: (t) => (sin(2 * t), cos(3 * t)), t_domain: (0, 6.28), stroke: cyan, width: 3
}

graph: Graph, x_domain: (-2, 2), y_domain: (-2, 2), size: (360, 360), at: (640, 360) {
  circle: PlotCurve, kind: "implicit", func: (x, y) => x * x + y * y - 1, resolution: 96, stroke: cyan, width: 3
}
```

---

## BarChart

A bar chart / column chart primitive for data visualization. Produces a set of
rectangular bars whose heights represent data values, with an optional baseline
axis. Supports standalone (pixel coordinates) and `Graph`-child (math coordinates)
modes.

**Properties:**
- `data`: Brace list of `(key, value)` tuples — the bar data
- `size`: Tuple `(width, height)` — chart visual bounds
- `bar_width`: Number or `"auto"` — per-bar width (default auto-distributes)
- `gap`: Number or `"auto"` — spacing between bars (default auto)
- `bar_colors`: `"auto"`, one color, or a brace list of colors — per-bar fill colors; colors may be RGBA tuples or scheme tokens such as `accent.danger`, `accent.success`, and `accent.warning`
- `show_axis`: Bool or string `"true"` | `"false"` — show baseline axis (default `"true"`)
- `show_labels`: Bool or string `"true"` | `"false"` — show bar labels (default `"true"`)
- `direction`: `"vertical"` | `"horizontal"` — bar orientation (default `"vertical"`, horizontal reserved)
- `max_value`: Number or `"auto"` — y-axis scale cap (default auto)
- `color`: Color — fallback bar fill color
- `stroke` / `stroke_color`: Color — bar outline color
- `stroke_width`: Number — bar outline width
- `x_domain`: Tuple `(min, max)` — math x-range (inherited from parent `Graph`)
- `y_domain`: Tuple `(min, max)` — math y-range (inherited from parent `Graph`)
- `at`: Tuple `(x, y)` — chart position
- `opacity`: Number — chart opacity

**Standalone example:**
```animatix
spectrum: BarChart,
  data: {("2 Hz", 1.0), ("5 Hz", 0.55), ("9 Hz", 0.3)},
  size: (600, 260),
  bar_colors: {accent.danger, accent.success, accent.warning},
  show_axis: true,
  show_labels: true,
  at: (640, 420)
```

**Inside a Graph:**
```animatix
graph: Graph, x_domain: (0, 12), y_domain: (0, 1.1), size: (700, 300) {
  spectrum: BarChart,
    data: {(2, 1.0), (5, 0.55), (9, 0.3)},
    bar_width: 0.8,
    show_axis: false,
    show_labels: false,
    bar_colors: {accent.danger, accent.success, accent.warning}

  envelope: PlotCurve,
    kind: "cartesian",
    func: (x) => exp(-0.2 * x),
    color: text.muted
}
```

---

## Equation

Container primitive for typeset equations with individually highlightable fragments.
Children are `Fragment` primitives whose content is concatenated and compiled as a
single Typst equation, preserving correct layout and spacing.

**Properties:**
- `font_size`: Number — font size in points (default 18)
- `color`: Color — default text color for all fragments (default `text.primary`)
- `at`: Tuple `(x, y)` — position (default `(0, 0)`)

**Example:**
```animatix
eq: Equation, font_size: 22, color: text.muted, at: (0, -230) {
  pre: Fragment, content: "x(t) = "
  f1: Fragment, content: "sin(2 pi dot 2t)"
  mid: Fragment, content: " + "
  f2: Fragment, content: "sin(2 pi dot 5t)"
}
```

## Fragment

Child primitive of `Equation`. Represents a named, addressable segment of the equation.
Does not render independently — the parent Equation handles all rendering.

**Properties:**
- `content`: String — Typst math content for this segment (default `""`)
- `highlight_color`: Color — color of highlight overlay rectangle (default `white`)
- `highlight_opacity`: Number — opacity of highlight, 0 = hidden, 1 = full (default `0.0`)
- `highlight_blend`: String — blend mode: `difference`, `exclusion`, `normal`, `multiply`, `screen` (default `"difference"`)
- `highlight_padding`: Number — padding around fragment bounding box (default `4.0`)
- `highlight_radius`: Number — corner radius of highlight rectangle (default `2.0`)

**Actions:**
- `highlight target [color: C, blend: B, padding: P, radius: R, duration, ease]` — animate `highlight_opacity` from 0 to 1
- `unhighlight target [duration, ease]` — animate `highlight_opacity` from current to 0

**Example:**
```animatix
#2s
  highlight eq.f1 [color: white, blend: difference, 800ms]

#4s
  unhighlight eq.f1 [400ms]
```

---

# 3. Containers

Auto-layout-first model with declaration-time measure/place contract. Explicit `at` opts into handcrafted placement.

## Row

**Properties:**
- `gap`: Number
- `padding`: Number
- `align`: `start | center | end`

## Col

**Properties:**
- `gap`: Number
- `padding`: Number
- `align`: `start | center | end`

## Group

Participates in scene graph and transform inheritance. Does not run a layout algorithm.

## Grid

Structured two-dimensional layout with `cols` and `gap` for deterministic declaration-order placement.

**Properties:**
- `cols`: Number
- `gap`: Number
- `padding`: Number

## Stack

Layered composition. Overlaps layout-managed children around a shared origin.

**Properties:**
- `gap`: Number
- `padding`: Number

Root layout containers can omit `at` and default to `scene.center`. Scene-relative placement via `anchor: scene.*`, `offset`, and percentage-based `at` is supported.

---

# 4. Common Animated Properties

Runtime supports explicit assignment for: `color`, `stroke`, `stroke_width`, `stroke_progress`,
`fill_opacity`, `size`, `at`/`position`, `radius_x`, `radius_y`, `from`, `to`, `scene.background_color`.

Text/Typst/Code use text-path keyframes; shapes use vector-path keyframes.

Nested property targeting via dotted paths works on both sides: `left.badge.color = red`
(assignment) and `copy.at = left.badge.at` (read). Component reads like `source.at.x`
are supported.

Geometry inputs (`points`, `commands`) are now fully animated tracks; assignments with
duration trigger path morphing via `vector_paths` interpolation.

Unsupported assignments report build diagnostics rather than silent ignore.

For bracket modifier details (`duration`, `delay`, `ease`, morphing strategies), see
[`spec.md`](spec.md).

---

# 5. Known Media Gaps

- `Svg.url` source assignment now supports timed keyframe transitions like `Image.url`; SVG path sets snap at the midpoint of a timed assignment and update measured size.