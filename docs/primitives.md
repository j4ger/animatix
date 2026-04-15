# Primitives

This document tracks what the current runtime actually supports. Where the parser or low-level Rust modules expose additional surface area, that is called out explicitly as parser-only or planned.

---

# 1. Runtime-Supported Scene Primitives

## Text
**Status:** Implemented in parser and runtime.

**Properties used by the runtime:**
- `text`: String
- `font_size`: Number
- `color`: Color
- `at`: Tuple `(x, y)`

**Example:**
```animatix
title: Text { text: "Hello World", font_size: 24, at: (640, 120) }
```

## Math
**Status:** Implemented in parser and runtime.

**Properties used by the runtime:**
- `math` or `latex`: String
- `font_size`: Number
- `color`: Color
- `at`: Tuple `(x, y)`

**Example:**
```animatix
eq: Math { math: "x^2 + 3", font_size: 18, at: (640, 360) }
```

## Code
**Status:** Implemented in parser and runtime.

The current v1 `Code` primitive is intentionally small. It renders code content through the same text-path pipeline used by `Text`, which makes it a real scene primitive without committing the runtime to syntax highlighting or editor-like behavior yet.

**Properties used by the runtime:**
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

**Properties used by the runtime:**
- `url`: String
- `scale`: Number
- `at`: Tuple `(x, y)`

**Example:**
```animatix
icon: Svg { url: "examples/vector.svg", scale: 1.5, at: (640, 600) }
```

## Image
**Status:** Implemented in parser and runtime.

**Properties used by the runtime:**
- `url`: String
- `at`: Tuple `(x, y)`
- `size`: Optional tuple `(width, height)`

If `size` is omitted, the runtime uses the image's natural pixel size. The initial implementation keeps the surface intentionally small and file-based.

**Transition note:** Animating `url` currently produces a discrete source swap, not a crossfade between two raster images. If you need a fade today, layer images manually and animate opacity instead.

**Example:**
```animatix
photo: Image { url: "examples/checker.ppm", at: (640, 360), size: (180, 180) }
```

## Circle
**Status:** Implemented in parser and runtime.

**Properties used by the runtime:**
- `radius`: Number
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width` or `width`: Number
- `fill_opacity`: Number
- `stroke_progress`: Number
- `at`: Tuple `(x, y)`

**Example:**
```animatix
c: Circle, radius: 50, color: red, at: (200, 200)
```

## Dot
**Status:** Implemented in parser and runtime.

`Dot` is the small-radius alias of the current circle pipeline. It accepts the same style properties as `Circle`, but its default radius is intentionally much smaller so it behaves like a point marker out of the box.

**Properties used by the runtime:**
- `radius`: Number
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width` or `width`: Number
- `fill_opacity`: Number
- `stroke_progress`: Number
- `at`: Tuple `(x, y)`

**Example:**
```animatix
dot: Dot, color: gold, at: (320, 240)
```

## Rect
**Status:** Implemented in parser and runtime.

The runtime uses the generic actor path with `size` rather than dedicated `width`/`height` fields.

**Properties used by the runtime:**
- `size`: Tuple `(width, height)`
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width` or `width`: Number
- `fill_opacity`: Number
- `stroke_progress`: Number
- `at`: Tuple `(x, y)`

**Example:**
```animatix
r: Rect, size: (160, 80), color: blue, at: (400, 300)
```

## Square
**Status:** Implemented in parser and runtime.

`Square` is the equal-side alias of the current rectangle pipeline. You can still use `size`, but the dedicated `side` property is the clearer shorthand for the common case.

**Properties used by the runtime:**
- `side`: Number
- `size`: Tuple `(width, height)`
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width` or `width`: Number
- `fill_opacity`: Number
- `stroke_progress`: Number
- `at`: Tuple `(x, y)`

**Example:**
```animatix
sq: Square, side: 120, color: blue, at: (400, 300)
```

## Line
**Status:** Implemented in parser and runtime.

**Properties used by the runtime:**
- `from`: Tuple `(x, y)` in local actor coordinates
- `to`: Tuple `(x, y)` in local actor coordinates
- `stroke` / `stroke_color`: Color
- `stroke_width` or `width`: Number
- `at`: Tuple `(x, y)`

`Line` is stroke-oriented in the current runtime. It does not produce a fill path.

**Example:**
```animatix
axis: Line, from: (-120, 0), to: (120, 0), stroke: white, stroke_width: 4, at: (640, 360)
```

## Arrow
**Status:** Implemented in parser and runtime.

`Arrow` is the straight-arrow companion to `Line`. It uses the same local `from` / `to` coordinates for the shaft, then generates a filled arrowhead at the `to` end through the existing vector path pipeline.

**Properties used by the runtime:**
- `from`: Tuple `(x, y)` in local actor coordinates
- `to`: Tuple `(x, y)` in local actor coordinates
- `tip_length`: Number
- `tip_width`: Number
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width` or `width`: Number
- `fill_opacity`: Number
- `at`: Tuple `(x, y)`

**Example:**
```animatix
flow: Arrow, from: (-100, 0), to: (100, 0), tip_length: 28, tip_width: 18, stroke: white, color: gold, at: (640, 360)
```

## Ellipse
**Status:** Implemented in parser and runtime.

**Properties used by the runtime:**
- `radius_x`: Number
- `radius_y`: Number
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width` or `width`: Number
- `fill_opacity`: Number
- `at`: Tuple `(x, y)`

The first runtime pass supports axis-aligned ellipses. Rotation is still future work at the DSL/runtime surface.

**Example:**
```animatix
halo: Ellipse, radius_x: 90, radius_y: 40, color: cyan, at: (640, 360)
```

## Arc
**Status:** Implemented in parser and runtime.

**Intended v1 properties:**
- `radius_x`: Number
- `radius_y`: Number
- `start_angle`: Number
- `sweep_angle`: Number
- `stroke` / `stroke_color`: Color
- `stroke_width` or `width`: Number
- `at`: Tuple `(x, y)`

The current runtime is stroke-first. It renders an open arc path, not a filled pie slice.

**Example:**
```animatix
ring: Arc, radius_x: 160, radius_y: 110, start_angle: -0.5, sweep_angle: 4.0, stroke: gold, width: 5, at: (640, 360)
```

## Polygon
**Status:** Implemented in parser and runtime.

**Intended v1 properties:**
- `points`: Tuple/list of point tuples
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width` or `width`: Number
- `fill_opacity`: Number
- `at`: Tuple `(x, y)`

The current runtime uses explicit points rather than higher-level helpers such as `sides` or `radius`. Geometry changes are expected to come from re-declaration/morphing rather than property-level point animation.

**Example:**
```animatix
badge: Polygon, points: {(-80, 0), (0, -70), (90, 0), (0, 80)}, color: cyan, at: (640, 360)
```

## RegularPolygon
**Status:** Implemented in parser and runtime.

`RegularPolygon` is the generated-points companion to `Polygon`. The runtime derives evenly spaced points from `sides` and `radius`, then routes the result through the same polygon/vector path pipeline.

**Intended v1 properties:**
- `sides`: Number (minimum 3)
- `radius`: Number
- `points`: Optional explicit point override
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width` or `width`: Number
- `fill_opacity`: Number
- `at`: Tuple `(x, y)`

**Example:**
```animatix
hex: RegularPolygon, sides: 6, radius: 70, color: cyan, at: (640, 360)
```

## Path
**Status:** Implemented in parser and runtime.

**Intended v1 properties:**
- `commands`: Tuple/list of path commands using existing call syntax
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width` or `width`: Number
- `fill_opacity`: Number
- `at`: Tuple `(x, y)`

The current runtime uses structured commands such as `move_to(...)`, `line_to(...)`, `quad_to(...)`, `curve_to(...)`, and `close()`. This keeps the implementation aligned with the existing expression/parser model instead of introducing a separate SVG path-string parser.

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

`Graph` is a container that establishes logical plotting domains and renders axes.

**Properties used by the runtime:**
- `x_domain`: Tuple `(min, max)`
- `y_domain`: Tuple `(min, max)`
- `size`: Tuple `(width, height)`
- `at`: Tuple `(x, y)`

## CartesianPlot
**Status:** Implemented in runtime.

**Properties used by the runtime:**
- `func`: Closure `(x) => expression`
- `color` or `stroke`: Color
- `width` / `stroke_width`: Number
- `tolerance`: Number
- `max_depth`: Number

## PolarPlot
**Status:** Implemented in runtime.

**Properties used by the runtime:**
- `func`: Closure `(theta) => expression`
- `t_domain`: Tuple `(min, max)`
- `color` or `stroke`: Color
- `width` / `stroke_width`: Number
- `tolerance`: Number
- `max_depth`: Number

**Example:**
```animatix
graph: Graph, x_domain: (-5, 5), y_domain: (-10, 30), size: (400, 400), at: (400, 300) {
  parabola: CartesianPlot, func: (x) => x^2 + 3, color: red, width: 2,
rose: PolarPlot, func: (t) => 3 * sin(4 * t), t_domain: (0, 6), stroke: green, width: 2
}
```

## ParametricPlot
**Status:** Implemented in runtime.

`ParametricPlot` samples a closure that returns a 2D point `(x, y)` over `t_domain`, then maps those math-space coordinates into the parent `Graph` domain.

**Properties used by the runtime:**
- `func`: Closure `(t) => (x_expr, y_expr)`
- `t_domain`: Tuple `(min, max)`
- `color` or `stroke`: Color
- `width` / `stroke_width`: Number
- `tolerance`: Number
- `max_depth`: Number

**Example:**
```animatix
graph: Graph, x_domain: (-2, 2), y_domain: (-2, 2), size: (360, 360), at: (640, 360) {
  lissajous: ParametricPlot, func: (t) => (sin(2 * t), cos(3 * t)), t_domain: (0, 6.28), stroke: cyan, width: 3
}
```

## ImplicitPlot
**Status:** Planned, not yet implemented.

The docs still reserve `ImplicitPlot` as future work. The current runtime does not sample or render implicit equations yet.

---

# 3. Containers

Animatix is standardizing on an **auto-layout-first** scene model. Containers should become the default composition tool, while explicit `at` remains the opt-in way to do handcrafted placement.

## Row
**Status:** Implemented in runtime auto-layout.

Properties:
- `gap`: Number
- `align`: `start | center | end`

Design direction:
- should remain a primary default authoring primitive
- container placement is now optional for root layout containers via the default `scene.center` binding
- explicit absolute placement on the container should remain supported

## Col
**Status:** Implemented in runtime auto-layout.

Properties:
- `gap`: Number
- `align`: `start | center | end`

Design direction:
- should remain a primary default authoring primitive
- container placement should become optional in the future
- explicit absolute placement on the container should remain supported

## Group
**Status:** Implemented as a grouping container.

`Group` participates in the scene graph and transform inheritance, but does not run a layout algorithm.

Design direction:
- remains the grouping/transform container
- should coexist with layout containers for scenes that mix structured layout and manual composition

## Grid / Stack
**Status:** Implemented in runtime.

`Grid` and `Stack` now participate in the runtime layout system.

Current role:
- `Grid`: structured two-dimensional layout for AI-friendly dashboards, panels, equations, legends, and repeated visual blocks
- `Stack`: layered composition for overlays, badges, callouts, and foreground/background composition without manual coordinate math

Phase 1 semantics implemented today:
- `Grid` supports `cols` and `gap` with deterministic declaration-order placement
- `Stack` overlaps layout-managed children around a shared origin
- root layout containers can omit `at` and default to `scene.center`
- scene-relative placement is supported through `anchor: scene.*`, `offset`, and percentage-based `at`
- manual child `at` remains an explicit opt-out inside layout containers

For current runnable demos, explanatory copy should use standalone `Text` / `Math` statements placed beside containers rather than inline text children.

---

# 4. Common Animated Properties

The current runtime has explicit assignment handling for these actor properties:

- `color`
- `stroke_width`
- `stroke_color`
- `stroke_progress`
- `fill_opacity`
- `size`
- `at` / `position`
- `radius`
- `radius_x`
- `radius_y`
- `from`
- `to`
- `scene.background_color`

Current runtime additions include `start_angle` and `sweep_angle` for `Arc`. `Polygon.points` and `Path.commands` are declaration-time geometry inputs rather than property-level animated tracks.

Assignments can now target nested runtime labels through multi-segment dotted paths. For example, a component-expanded nested actor can be updated with `left.badge.color = red` or `right.frame.radius = 20`. This is label targeting, not a general object-property query system.

The same dotted path surface now works on the rhs for sampled property reads. Common examples are `copy.at = left.badge.at`, `echo.radius = right.badge.radius`, and scalar/vector component reads such as `source.at.x`.

Text and math content are rendered through text-path keyframes; shape actors are rendered through vector-path keyframes.

Absolute positioning is intentionally preserved in the language. The design change is about making layout containers and scene-relative placement the preferred default, not about removing direct coordinate control.

## 4.1 Bracket Modifier Status

Square brackets are parsed through one generic modifier shape, but the current runtime does **not** treat every declaration surface equally.

**Runtime-real today:**
- property assignments support duration shorthand plus named `delay` plus named `ease`
- `Text`, `Math`, and `Code` declarations support duration shorthand plus named `delay` plus named `ease`
- built-in actions (`fade-in`, `wipe-in`, `fade-out`) support duration shorthand plus named `delay` plus named `ease`

**Runtime-real today:**
- actor re-declarations use the same shipped timing subset as the other declaration/action hosts: duration shorthand plus named `delay` plus named `ease`
- timed path-morphing re-declarations can additionally use `strategy: auto|match`, `path_arc`, and `stretch`

**Planned / not yet runtime-real:**
- `strategy: fade`

Duplicate shipped modifier keys are reported deliberately and the runtime uses the last provided value. `ease` without an explicit duration remains an instant change; `delay` can shift that instant application later in time.

Near-term note: the shipped scoped morph controls are `strategy: auto|match`, `path_arc`, and `stretch` on timed path-morphing re-declarations. A true `fade` strategy is deferred for now because it likely needs a different transition/compositing representation than the existing single-track morph path model.

Unsupported modifier keys are reported explicitly during build/timeline construction rather than being treated as silently supported behavior.

The intended long-term design is a typed declarative modifier bag with one universal duration shorthand, a small shared timing vocabulary, and host-specific keys only where a statement kind explicitly supports them.

---

# 5. Morphing Status

**Implemented today:**
- Vector path interpolation for runtime scene tracks
- Low-level path morphing in `timeline/morph.rs`
- Shape-to-path morph helpers in `timeline/kurbo_shapes.rs`

**Not implemented in the DSL/runtime yet:**
- `strategy: fade`
- High-level multi-strategy morph selection described in older drafts

---

# 6. Other Planned Primitives

Some shape-oriented concepts also exist in lower-level Rust modules such as `kurbo_shapes.rs`; `Arc`, `Polygon`, and `Path` are now part of the shipped runtime surface described above.

---

# 7. Rendering Notes

Animatix uses **Vello** as the primary rendering backend. Text, math, SVGs, plots, and the currently supported shape actors are all converted into vector path data for rendering.
