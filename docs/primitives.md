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

**Absorbs:** `Square` — use `side` property for equal-sided squares. `side` takes precedence over
`size` when both are provided.

**Properties:**
- `size`: Tuple `(width, height)` — general rectangle dimensions
- `side`: Number — shorthand for equal width/height (square mode)
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width`: Number
- `fill_opacity`: Number
- `stroke_progress`: Number
- `at`: Tuple `(x, y)`

**Special modes:**
- Providing `side` (and omitting `size`) activates square mode — renders as equal-sided rect.
  Equivalent to `size: (side, side)`.

**Examples:**
```animatix
# Rectangle with explicit size
r: Rect, size: (160, 80), color: blue, at: (400, 300)

# Square via side shorthand
sq: Rect, side: 120, color: green, at: (400, 500)
```

## Ellipse
**Status:** Implemented in parser and runtime.

**Absorbs:** `Circle`, `Dot`, `Arc`.

- `Circle` — use `radius` or `size` for uniform radii.
- `Dot` — use `radius` with a small value.
- `Arc` — add `start_angle` and `sweep_angle` to render an open arc instead of a full ellipse.

**Properties:**
- `size`: Tuple `(width, height)` — bounding box dimensions, converted to `radius_x = width / 2`, `radius_y = height / 2`
- `radius`: Number — shorthand for equal `radius_x` and `radius_y` (circle mode)
- `radius_x`: Number — horizontal radius
- `radius_y`: vertical radius
- `start_angle`: Number — arc start in radians (default: `0`)
- `sweep_angle`: Number — arc sweep in radians (default: `2π`, full ellipse)
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width`: Number
- `fill_opacity`: Number
- `stroke_progress`: Number
- `at`: Tuple `(x, y)`

**Special modes:**
- Providing `radius` (without `radius_x`/`radius_y`) activates circle mode —
  renders a uniform-radius circle.
- Providing `start_angle` and/or `sweep_angle` (when sweeping from `2π`) activates arc mode —
  renders an open arc (stroke-only, not a pie slice). In arc mode `color` and `fill_opacity`
  are ignored; only `stroke`/`stroke_width` apply.
- A small `radius` (e.g. `≤ 3`) renders a dot — use `color` for fill, omit stroke for a clean dot.

**Examples:**
```animatix
# Circle via radius shorthand
c: Ellipse, radius: 50, color: red, at: (200, 200)

# Ellipse with explicit radii
halo: Ellipse, radius_x: 90, radius_y: 40, color: cyan, at: (640, 360)

# Ellipse via size tuple
halo: Ellipse, size: (180, 80), color: cyan, at: (640, 360)

# Dot (small-radius circle)
dot: Ellipse, radius: 3, color: gold, at: (320, 240)

# Arc (open stroke-only arc)
ring: Ellipse, radius_x: 160, radius_y: 110, start_angle: -0.5, sweep_angle: 4.0, stroke: gold, stroke_width: 5, at: (640, 360)
```

## Line
**Status:** Implemented in parser and runtime.

**Absorbs:** `Arrow` — add `tip_length` and `tip_width` to draw an arrowhead at the `to` endpoint.

Stroke-oriented — no fill property.

**Properties:**
- `from`: Tuple `(x, y)` in local actor coordinates
- `to`: Tuple `(x, y)` in local actor coordinates
- `tip_length`: Number — arrowhead length along the shaft (default: `0`, no arrowhead)
- `tip_width`: Number — arrowhead width perpendicular to the shaft
- `stroke` / `stroke_color`: Color
- `stroke_width`: Number
- `at`: Tuple `(x, y)`

**Special modes:**
- Providing `tip_length` and `tip_width` (both > 0) activates arrowhead mode —
  renders a filled arrowhead at the `to` endpoint via the vector path pipeline. In arrowhead mode
  the `stroke` color is used for the shaft and the arrowhead fill matches `stroke`.

**Examples:**
```animatix
# Plain line (no arrowhead)
axis: Line, from: (-120, 0), to: (120, 0), stroke: white, stroke_width: 4, at: (640, 360)

# Arrow line
flow: Line, from: (-100, 0), to: (100, 0), tip_length: 28, tip_width: 18, stroke: white, at: (640, 360)
```

## Polygon
**Status:** Implemented in parser and runtime.

**Absorbs:** `RegularPolygon` — use `sides` and `radius` to generate evenly spaced points automatically.

**Properties:**
- `points`: Tuple/list of point tuples — explicit vertex list
- `sides`: Number (≥ 3) — number of sides for automatic regular polygon generation
- `radius`: Number — circumradius used with `sides` for point generation
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width`: Number
- `fill_opacity`: Number
- `stroke_progress`: Number
- `at`: Tuple `(x, y)`

**Special modes:**
- Providing `sides ≥ 3` (with or without explicit `points`) activates regular polygon
  generation — points are derived automatically from `sides` and `radius`.
- Providing explicit `points` overrides regular polygon generation and renders an arbitrary
  polygon from the given vertices.

**Examples:**
```animatix
# Explicit vertex list
badge: Polygon, points: {(-80, 0), (0, -70), (90, 0), (0, 80)}, color: cyan, at: (640, 360)

# Regular polygon via sides/radius
hex: Polygon, sides: 6, radius: 70, color: cyan, at: (640, 360)
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

Container establishing logical plotting domains and rendering axes.

**Properties:**
- `x_domain`: Tuple `(min, max)`
- `y_domain`: Tuple `(min, max)`
- `size`: Tuple `(width, height)`
- `at`: Tuple `(x, y)`

## CartesianPlot
**Status:** Implemented in runtime.

**Properties:**
- `func`: Closure `(x) => expression`
- `color` / `stroke`: Color
- `width` / `stroke_width`: Number
- `tolerance`: Number
- `max_depth`: Number

## PolarPlot
**Status:** Implemented in runtime.

**Properties:**
- `func`: Closure `(theta) => expression`
- `t_domain`: Tuple `(min, max)`
- `color` / `stroke`: Color
- `width` / `stroke_width`: Number
- `tolerance`: Number
- `max_depth`: Number

## ParametricPlot
**Status:** Implemented in runtime.

Samples closure returning `(x, y)` over `t_domain`, maps into parent Graph domain.

**Properties:**
- `func`: Closure `(t) => (x_expr, y_expr)`
- `t_domain`: Tuple `(min, max)`
- `color` / `stroke`: Color
- `width` / `stroke_width`: Number
- `tolerance`: Number
- `max_depth`: Number

## ImplicitPlot
**Status:** Implemented in runtime.

Samples scalar field closure `(x, y) => expr`, extracts zero contour via marching squares.

**Properties:**
- `func`: Closure `(x, y) => scalar_expr`
- `resolution`: Number of sampling cells along longer graph axis
- `color` / `stroke`: Color
- `width` / `stroke_width`: Number

**Constraints:** Stroke-only, zero contour only (`func(x,y) = 0`), quality depends on resolution and sampled grid.

**Example:**
```animatix
graph: Graph, x_domain: (-5, 5), y_domain: (-10, 30), size: (400, 400), at: (400, 300) {
  parabola: CartesianPlot, func: (x) => x^2 + 3, color: red, width: 2,
  rose: PolarPlot, func: (t) => 3 * sin(4 * t), t_domain: (0, 6), stroke: green, width: 2
}

graph: Graph, x_domain: (-2, 2), y_domain: (-2, 2), size: (360, 360), at: (640, 360) {
  lissajous: ParametricPlot, func: (t) => (sin(2 * t), cos(3 * t)), t_domain: (0, 6.28), stroke: cyan, width: 3
}

graph: Graph, x_domain: (-2, 2), y_domain: (-2, 2), size: (360, 360), at: (640, 360) {
  circle: ImplicitPlot, func: (x, y) => x * x + y * y - 1, resolution: 96, stroke: cyan, width: 3
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
`fill_opacity`, `size`, `side`, `at`/`position`, `radius`, `radius_x`, `radius_y`, `from`,
`to`, `tip_length`, `tip_width`, `start_angle`, `sweep_angle`, `scene.background_color`.

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