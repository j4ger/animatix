# Primitives

Documents current runtime support. Parser-only and planned features are noted explicitly.

For colorscheme details including `config { colorscheme: ... }`, semantic color aliases, `color: auto`, inline `Colorscheme` with `extends`, and module-based reuse, see [`colorscheme_design.md`](colorscheme_design.md).

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
- `at`: Tuple `(x, y)` or scene-relative percent tuple `(30%, 38%)`
- `anchor`: Scene anchor
- `offset`: Tuple `(x, y)`
- `size`: Optional tuple `(width, height)` — defaults to intrinsic pixel size

**Example:**
```animatix
photo: Image { url: "examples/checker.ppm", at: (640, 360), size: (180, 180) }
```

Note: Missing files report build diagnostics. Source changes are discrete (crossfade requires manual opacity layering).

## Circle
**Status:** Implemented in parser and runtime.

**Properties:**
- `radius`: Number
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width` / `width`: Number
- `fill_opacity`: Number
- `stroke_progress`: Number
- `at`: Tuple `(x, y)`

**Example:**
```animatix
c: Circle, radius: 50, color: red, at: (200, 200)
```

## Dot
**Status:** Implemented in parser and runtime.

Small-radius alias of Circle pipeline. Same properties as Circle.

**Example:**
```animatix
dot: Dot, color: gold, at: (320, 240)
```

## Rect
**Status:** Implemented in parser and runtime.

Uses generic `size` tuple (not separate `width`/`height`).

**Properties:**
- `size`: Tuple `(width, height)`
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width` / `width`: Number
- `fill_opacity`: Number
- `stroke_progress`: Number
- `at`: Tuple `(x, y)`

**Example:**
```animatix
r: Rect, size: (160, 80), color: blue, at: (400, 300)
```

## Square
**Status:** Implemented in parser and runtime.

Equal-side alias of Rect. Supports both `side` shorthand and `size`.

**Properties:**
- `side`: Number
- `size`: Tuple `(width, height)`
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width` / `width`: Number
- `fill_opacity`: Number
- `stroke_progress`: Number
- `at`: Tuple `(x, y)`

**Example:**
```animatix
sq: Square, side: 120, color: blue, at: (400, 300)
```

## Line
**Status:** Implemented in parser and runtime. Stroke-oriented (no fill).

**Properties:**
- `from`: Tuple `(x, y)` in local actor coordinates
- `to`: Tuple `(x, y)` in local actor coordinates
- `stroke` / `stroke_color`: Color
- `stroke_width` / `width`: Number
- `at`: Tuple `(x, y)`

**Example:**
```animatix
axis: Line, from: (-120, 0), to: (120, 0), stroke: white, stroke_width: 4, at: (640, 360)
```

## Arrow
**Status:** Implemented in parser and runtime.

Straight-arrow companion to Line. Generates filled arrowhead at `to` end via vector path pipeline.

**Properties:**
- `from`: Tuple `(x, y)` in local actor coordinates
- `to`: Tuple `(x, y)` in local actor coordinates
- `tip_length`: Number
- `tip_width`: Number
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width` / `width`: Number
- `fill_opacity`: Number
- `at`: Tuple `(x, y)`

**Example:**
```animatix
flow: Arrow, from: (-100, 0), to: (100, 0), tip_length: 28, tip_width: 18, stroke: white, color: gold, at: (640, 360)
```

## Ellipse
**Status:** Implemented in parser and runtime. Axis-aligned only (rotation planned).

**Properties:**
- `radius_x`: Number
- `radius_y`: Number
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width` / `width`: Number
- `fill_opacity`: Number
- `at`: Tuple `(x, y)`

**Example:**
```animatix
halo: Ellipse, radius_x: 90, radius_y: 40, color: cyan, at: (640, 360)
```

## Arc
**Status:** Implemented in parser and runtime. Stroke-only (open arc, not pie slice).

**Properties:**
- `radius_x`: Number
- `radius_y`: Number
- `start_angle`: Number
- `sweep_angle`: Number
- `stroke` / `stroke_color`: Color
- `stroke_width` / `width`: Number
- `at`: Tuple `(x, y)`

**Example:**
```animatix
ring: Arc, radius_x: 160, radius_y: 110, start_angle: -0.5, sweep_angle: 4.0, stroke: gold, width: 5, at: (640, 360)
```

## Polygon
**Status:** Implemented in parser and runtime.

**Properties:**
- `points`: Tuple/list of point tuples
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width` / `width`: Number
- `fill_opacity`: Number
- `at`: Tuple `(x, y)`

**Example:**
```animatix
badge: Polygon, points: {(-80, 0), (0, -70), (90, 0), (0, 80)}, color: cyan, at: (640, 360)
```

## RegularPolygon
**Status:** Implemented in parser and runtime.

Generated-points companion to Polygon. Derives evenly spaced points from `sides` and `radius`.

**Properties:**
- `sides`: Number (minimum 3)
- `radius`: Number
- `points`: Optional explicit point override
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width` / `width`: Number
- `fill_opacity`: Number
- `at`: Tuple `(x, y)`

**Example:**
```animatix
hex: RegularPolygon, sides: 6, radius: 70, color: cyan, at: (640, 360)
```

## Path
**Status:** Implemented in parser and runtime.

Uses structured commands (`move_to(...)`, `line_to(...)`, `quad_to(...)`, `curve_to(...)`, `close()`).

**Properties:**
- `commands`: Tuple/list of path commands
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width` / `width`: Number
- `fill_opacity`: Number
- `at`: Tuple `(x, y)`

**Example:**
```animatix
guide: Path, commands: {
  move_to(-120, 0),
  line_to(-40, -80),
  curve_to(20, -120, 80, 40, 140, -10),
  close()
}, stroke: white, width: 4, at: (640, 360)
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
- `align`: `start | center | end`

## Col
**Status:** Implemented in runtime auto-layout.

**Properties:**
- `gap`: Number
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

## Stack
**Status:** Implemented in runtime auto-layout.

Layered composition. Overlaps layout-managed children around a shared origin.

**Properties:**
- `gap`: Number

Root layout containers can omit `at` and default to `scene.center`. Scene-relative placement via `anchor: scene.*`, `offset`, and percentage-based `at` is supported.

---

# 4. Common Animated Properties

Runtime supports explicit assignment for: `color`, `stroke_width`, `stroke_color`, `stroke_progress`, `fill_opacity`, `size`, `at`/`position`, `radius`, `radius_x`, `radius_y`, `from`, `to`, `start_angle`, `sweep_angle`, `scene.background_color`.

Text/Math/Code use text-path keyframes; shapes use vector-path keyframes.

Nested property targeting via dotted paths works on both sides: `left.badge.color = red` (assignment) and `copy.at = left.badge.at` (read). Component reads like `source.at.x` are supported.

Geometry inputs (`Polygon.points`, `Path.commands`) are declaration-time only, not animated tracks.

Unsupported assignments report build diagnostics rather than silent ignore.

For bracket modifier details (`duration`, `delay`, `ease`, morphing strategies), see [`spec.md`](spec.md).

---

# 5. Planned / Parser-Only

- `Image` / `Svg` source assignment (currently requires re-declaration)
- `Ellipse` rotation
- `strategy: fade` morphing
- High-level multi-strategy morph selection
