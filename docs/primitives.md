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
- `scene.background_color`

Text and math content are rendered through text-path keyframes; shape actors are rendered through vector-path keyframes.

Absolute positioning is intentionally preserved in the language. The design change is about making layout containers and scene-relative placement the preferred default, not about removing direct coordinate control.

---

# 5. Morphing Status

**Implemented today:**
- Vector path interpolation for runtime scene tracks
- Low-level path morphing in `timeline/morph.rs`
- Shape-to-path morph helpers in `timeline/kurbo_shapes.rs`

**Not implemented in the DSL/runtime yet:**
- `strategy`, `path_arc`, and `stretch` modifiers
- High-level multi-strategy morph selection described in older drafts

---

# 6. Parser-Only or Planned Primitives

The docs previously listed several primitives as if they were runtime-ready. They are not currently wired into the scene runtime:

- `Line`
- `Path`
- `Polygon`
- `Arc`
- `Ellipse`
- `Image`
- `Code`

Some of these concepts exist in lower-level Rust modules such as `kurbo_shapes.rs`, but they are not first-class scene primitives in the current Animatix runtime.

---

# 7. Rendering Notes

Animatix uses **Vello** as the primary rendering backend. Text, math, SVGs, plots, and the currently supported shape actors are all converted into vector path data for rendering.
