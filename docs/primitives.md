# Primitives

Documents current runtime support. Parser-only and planned features are noted explicitly.

For colorscheme details, see [`architecture.md`](architecture.md) §Colorscheme System.

---

# 1. Scene Primitives

## Text
**Status:** Implemented in parser and runtime.

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

## Math
**Status:** Implemented in parser and runtime.

**Properties:**
- `math` / `latex`: String
- `font_size`: Number
- `color`: Color
- `at`: Tuple `(x, y)`

**Example:**
```animatix
eq: Math { math: "x^2 + 3", font_size: 18, at: (640, 360) }
```

## Code
**Status:** Implemented in parser and runtime.

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
**Status:** Implemented in parser and runtime.

**Properties:**
- `url`: String
- `scale`: Number
- `at`: Tuple `(x, y)` or scene-relative percent tuple `(72%, 38%)`
- `anchor`: Scene anchor
- `offset`: Tuple `(x, y)`

**Example:**
```animatix
icon: Svg { url: "examples/vector.svg", scale: 1.5, at: (640, 600) }
```

Note: Missing files or invalid SVG contents report build diagnostics. Source changes require re-declaration at a keyframe (assignment not yet supported).

## Image
**Status:** Implemented in parser and runtime.

**Properties:**
- `url`: String
- `at`: Tuple `(x, y)` or percent tuple `(30%, 38%)`
- `anchor`: Scene anchor
- `offset`: Tuple `(x, y)`
- `size`: Optional tuple `(width, height)` — defaults to intrinsic pixel size

**Example:**
```animatix
photo: Image { url: "examples/checker.ppm", at: (640, 360), size: (180, 180) }
```

Note: Missing files report build diagnostics. Source changes are discrete (crossfade requires manual opacity layering).

## Rect
**Status:** Implemented in parser and runtime.

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
**Status:** Implemented in parser and runtime.

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
**Status:** Implemented in parser and runtime.

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
**Status:** Implemented in parser and runtime.

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
**Status:** Implemented in parser and runtime.

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
**Status:** Implemented in runtime.

Container establishing logical plotting domains and rendering axes. Supports optional grid lines and ticks.

**Properties:**
- `x_domain`: Tuple `(min, max)`
- `y_domain`: Tuple `(min, max)`
- `size`: Tuple `(width, height)`
- `at`: Tuple `(x, y)`
- `grid`: Boolean — draw grid lines at regular intervals
- `ticks`: Boolean — draw tick marks on axes
- `tick_labels`: Boolean — draw numeric labels at ticks (not yet implemented)

## PlotCurve
**Status:** Implemented in runtime.

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

## VectorField
**Status:** Implemented in runtime.

Grid-sampled vector field rendered as arrows.

**Properties:**
- `func`: Closure `(x, y) => (dx, dy)` — returns a vector at each point
- `density`: Number — grid resolution (default 16)
- `x_domain`: Tuple `(min, max)`
- `y_domain`: Tuple `(min, max)`
- `size`: Tuple `(width, height)`
- `color` / `stroke`: Color

## Heatmap
**Status:** Implemented in runtime.

Pixel-level scalar field visualization using colored rectangles.

**Properties:**
- `func`: Closure `(x, y) => scalar`
- `resolution`: Number — grid resolution (default 64)
- `x_domain`: Tuple `(min, max)`
- `y_domain`: Tuple `(min, max)`
- `size`: Tuple `(width, height)`
- `color`: Color — used as the "hot" color (alpha varies by scalar value)

## ContourSet
**Status:** Implemented in runtime.

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
**Status:** Implemented in runtime.

Math coordinate system that auto-generates axes, grid lines, and tick marks. Unlike `Graph`, it does not host child plots — it is a standalone visual coordinate plane.

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

# 3. Containers

Auto-layout-first model with declaration-time measure/place contract. Explicit `at` opts into handcrafted placement.

## Row
**Status:** Implemented in runtime auto-layout.

**Properties:**
- `gap`: Number
- `padding`: Number
- `align`: `start | center | end`

## Col
**Status:** Implemented in runtime auto-layout.

**Properties:**
- `gap`: Number
- `padding`: Number
- `align`: `start | center | end`

## Group
**Status:** Implemented as grouping container.

Participates in scene graph and transform inheritance. Does not run a layout algorithm.

## Grid
**Status:** Implemented in runtime auto-layout.

Structured two-dimensional layout. Phase 1 supports `cols` and `gap` with deterministic declaration-order placement.

**Properties:**
- `cols`: Number
- `gap`: Number
- `padding`: Number

## Stack
**Status:** Implemented in runtime auto-layout.

Layered composition. Overlaps layout-managed children around a shared origin.

**Properties:**
- `gap`: Number
- `padding`: Number

Root layout containers can omit `at` and default to `scene.center`. Scene-relative placement via `anchor: scene.*`, `offset`, and percentage-based `at` is supported.

---

# 4. Common Animated Properties

Runtime supports explicit assignment for: `color`, `stroke`, `stroke_width`, `stroke_progress`,
`fill_opacity`, `size`, `at`/`position`, `radius_x`, `radius_y`, `from`, `to`, `scene.background_color`.

Text/Math/Code use text-path keyframes; shapes use vector-path keyframes.

Nested property targeting via dotted paths works on both sides: `left.badge.color = red`
(assignment) and `copy.at = left.badge.at` (read). Component reads like `source.at.x`
are supported.

Geometry inputs (`points`, `commands`) are now fully animated tracks; assignments with
duration trigger path morphing via `vector_paths` interpolation.

Unsupported assignments report build diagnostics rather than silent ignore.

For bracket modifier details (`duration`, `delay`, `ease`, morphing strategies), see
[`spec.md`](spec.md).

---

# 5. Planned / Parser-Only

- `Image` / `Svg` source assignment (currently requires re-declaration)
- ~~`Ellipse` rotation~~ (unified into Ellipse)
- ~~`strategy: fade` morphing~~ (implemented)
- High-level multi-strategy morph selection