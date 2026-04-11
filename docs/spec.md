# Animatix Language Specification

> This document describes the current implemented language surface first. Status callouts distinguish shipped runtime behavior from parser-only or planned syntax.

---

## 1. File Types

- **Animatix Files (`.amx`)**: Main source files loaded by the runtime through `import "..."`.
- Naming conventions such as `.actor.amx` or `.lib.amx` may still appear in examples, but they are conventions rather than distinct runtime file kinds today.

---

## 2. Core Syntax Symbols

- **Let (`let`)**: Used for declaring objects, variables and actors (non-rendered values).  
  *Example:* `let x = 0`
- **Colon (`:`)**: Shorthand for binding actors to a label, and put it to scene.  
  *Example:* `btn: Button, text: "OK"`
- **Hash (`#`)**: Marks a keyframe in the timeline.  
  *Example:* `#0s`, `#2.5s`, `#@10s` (absolute timestamp)
- **Curly Brackets (`{ }`)**: Used for container children, arrays, and block scopes.  
  *Example:* `Row { Item1, Item2 }`
- **Square Brackets (`[ ]`)**: Used for action modifiers (duration, easing).  
  *Example:* `[2s, ease: bounce]`
- **Equals (`=`)**: Used for property assignment (instant change or animated) or variable binding.
  *Example:* `btn.color = red` or `morpher.size = (100, 100) [2s]`
- **Comma (`,`)**: Separates object properties.  
  *Example:* `Type, prop: val, prop: val`
- **Space (` `)**: Separates action verbs from arguments.  
  *Example:* `fade-in btn [1s]`
- **Dot (`.`)**: Used for nested access (namespacing).  
  *Example:* `container.child`

---

## 3. Declarations

**Actor Declaration**  
Actors are rendered objects. They must be declared with a label and a type.
```animatix
label: Type, property: value
```

Absolute positioning via `at: (x, y)` remains fully supported, but it should be understood as one placement mode rather than the only composition model.

**Variable Declaration**  
Variables are computed values. They are not rendered directly.
```animatix
let name = expression
```

**Re-Declaration (Morph Trigger)**  
If an existing label is declared again at a later keyframe, the engine morphs the existing actor to the new definition.
```animatix
#0s
btn: Button, text: "OK"

#2s
btn: Button, text: "Submit" [2s]

#10s
btn: another_button // morph into another pre-defined object
```

**Implicit Objects**  
The engine provides an implicit `scene` object representing the global environment. Its properties, such as `background_color` (defaulting to `black`), can be assigned and animated like any other property.
```animatix
#0s
scene.background_color = black // Default

#2s
scene.background_color = white [2s]
```

---

## 4. Timeline & Keyframes

**Timeline Structure**  
The timeline maintains a hierarchical `scene_graph` mapping parent containers to their children. During evaluation, transforms and opacities cascade down this tree via depth-first search, accumulating parent values into child values.

**Absolute Keyframes**  
Marks a specific time in seconds or milliseconds.
```animatix
#@0s
#@2.5s
#@500ms
```

**Relative Keyframes**  
Marks a time relative to the previous keyframe.
```animatix
#1s
```

**Parallel Actions**  
Actions listed under the same keyframe execute simultaneously.
```animatix
#2s
fade-in A [1s]
color B to red [2s]
```

---

## 5. Actions & Modifiers

**Action Invocation**  
Actions use verb-first syntax with space-separated arguments.
```animatix
fade-in btn [1s]
fade-out btn [1s]
```

**Modifiers**  
Modifiers are enclosed in square brackets immediately following the action.
```animatix
[duration]
[ease: function]
[delay: time]
```

**Common Modifiers**
```animatix
[2s]                      // Duration
[ease: ease-in-out]       // Easing curve
```

**Built-in Actions Registry**  
The runtime currently registers three built-in actions:
- **Entrance**: `fade-in`, `wipe-in`
- **Exit**: `fade-out`

The action system exposes action signatures through the Rust registry API, which is enough for editor/LSP integration work. Higher-level editor workflows remain future work.

---

## 6. Morphing System

**Automatic Morph**  
Re-declaring an actor at a new keyframe triggers a morph transition.
```animatix
#0s
circle: Circle, at: (0, 0)

#2s
circle: Circle, at: (100, 100) [2s]
```

**Morph Strategies**  
The runtime morphs vector path data when a supported actor is re-declared, but advanced strategy modifiers are not wired into the runtime yet. The following syntax is still planned rather than implemented:
```animatix
[strategy: auto]          // Engine decides (default)
[strategy: match]         // Force point alignment
[strategy: fade]          // Cross-fade (ReplacementTransform)
[path_arc: 1.57]          // Planned curved interpolation hint
[stretch: false]          // Planned bounds-fitting control
```

**Instant Change**
Use zero duration or property assignment for instant updates.
```animatix
btn: Button, text: "New" [0s]
btn.text = "New"
```

**Property-Level State Tracking**
Assignments can now take modifiers, allowing individual properties to be animated independently of the entire actor morph.
```animatix
morpher.size = (100, 100) [2s, ease: ease-out]
```

**Additional Runtime Primitives**
`Arc`, `Polygon`, and `Path` are now implemented runtime primitives.

- `Arc` is a stroke-first primitive using `radius_x`, `radius_y`, `start_angle`, and `sweep_angle`.
- `Polygon` is built around explicit `points` input rather than higher-level helpers like `sides`.
- `Path` is built around structured `commands` such as `move_to(...)`, `line_to(...)`, `quad_to(...)`, `curve_to(...)`, and `close()`.

This `Path` surface intentionally reuses existing call-expression syntax rather than introducing a separate SVG-style path-string grammar.

---

## 7. Containers & Layout

> **Status: `Row`, `Col`, `Grid`, `Stack`, and `Group` are implemented. Layout containers support explicit manual-vs-layout placement semantics for children, root layout containers may omit `at` and default to `scene.center`, and scene-relative placement is available through `anchor: scene.*`, `offset`, and percentage-based `at`. Authored `at` values — including `(0, 0)` — are preserved instead of being treated as an unset sentinel.**

**Container Types**

- `Row`: Horizontal layout container. Supports `gap` (number, spacing between children) and `align` ("start", "center", "end" for vertical alignment). (Implemented)
- `Col`: Vertical layout container. Supports `gap` (number, spacing between children) and `align` ("start", "center", "end" for horizontal alignment). (Implemented)
- `Grid`: Two-dimensional layout container with `cols` and `gap` support (Implemented)
- `Stack`: Layered layout container that overlaps layout-managed children around a shared origin (Implemented)
- `Group`: Generic container for grouping and transform inheritance (implemented, but without auto-layout)

**Design Direction**

Animatix is moving toward a container-first layout model:

- Layout containers should be the default way to compose scenes
- Absolute positioning should remain available for precise motion-graphics-style work
- Parent containers should own placement when layout semantics are active
- The language should prefer deterministic layout over fully general constraints

**Layout Properties**

The `gap` property sets uniform spacing between children. The `align` property controls perpendicular alignment:
- For `Row`: aligns children vertically ("start" = top, "center" = middle, "end" = bottom)
- For `Col`: aligns children horizontally ("start" = left, "center" = middle, "end" = right)

```animatix
row: Row, gap: 12, align: "center" {
  Rect, color: red
  Circle, color: blue
}
```

Layout containers may omit explicit absolute placement and rely on a deterministic container default. The current default for root layout containers is `scene.center`. Explicit `at` on the container remains valid.

**Absolute Positioning**

Absolute placement remains part of the language and should continue to work for both root actors and layout children when a scene intentionally needs hand-tuned coordinates.

Within layout containers, an authored child `at` opts that child into manual placement. Children without `at` remain layout-managed.

```animatix
badge: Circle, radius: 24, at: (1180, 80)
```

```animatix
row: Row, at: (640, 360), gap: 16 {
  pinned: Circle, radius: 20, at: (0, 0)
  auto: Circle, radius: 20
}
```

The design intent is not to remove absolute positioning, but to stop forcing it as the default composition strategy.

**Scene-relative Placement**

Scene-relative placement is now available through anchors, offsets, and percentage coordinates:

```animatix
title: Text { text: "Layout", anchor: scene.top, offset: (0, 80) }
badge: Stack, at: (82%, 76%) {
  Rect, size: (220, 80)
  Circle, radius: 18
}
```

Supported scene anchors are: `scene.top_left`, `scene.top`, `scene.top_right`, `scene.left`, `scene.center`, `scene.right`, `scene.bottom_left`, `scene.bottom`, and `scene.bottom_right`.

**Phase 1 Layout Surface**

The current layout-related runtime surface is:

- `Row`, `Col`, `Grid`, and `Stack` runtime behavior
- root layout-container defaults to `scene.center` when `at` is omitted
- scene-relative anchors and percentage-based placement
- explicit child opt-out / manual-placement semantics inside layout containers
- `Group` as the non-layout grouping/transform container

General-purpose constraint solving is intentionally deferred. The preferred model is predictable parent-driven layout with explicit escape hatches.

The concrete precedence rules and rollout slices for this direction are documented in [`layout_design.md`](layout_design.md).

**Children Declaration**  
Children are declared inline within curly brackets. Children may be **labeled** (explicit name) or **anonymous** (no name).

```animatix
row: Row, gap: 10 {
  left: Rect, color: red
  right: Circle, color: blue
}
```

Current runnable demos should use standalone `Text { ... }` and `Math { ... }` statements for explanatory labels. Inline container children are processed through the generic actor-declaration path, so user-facing captions are clearer and safer as sibling text nodes today.

**Anonymous Children and Auto-UID**

Children without a label receive an auto-generated UID internally. This is used by the runtime to build the scene graph and lay out inline items.

```animatix
col: Col {
  Rect, color: red        // anonymous: auto-UID assigned
  Circle, color: blue     // anonymous: different auto-UID
}

#1s
// direct index-based keyframing is planned API surface, not current runtime behavior
```

**Nesting Containers**

Containers can nest arbitrarily deep, with transforms accumulating down the hierarchy:

```animatix
scene: Group {
  inner: Group {
    leaf: Circle, at: (50, 0)
  }
}
```

The `leaf` circle inherits transforms from both `inner` and `scene`. Rotating `scene` 90 degrees causes `leaf` to orbit accordingly.

---

## 8. Reactive System

> **Status: Implemented.** `always`, `loop`, labeled loops, `yield`, loop control, inline conditionals, and compile-time `for` expansion all run in the current runtime.

**Always Blocks**  
Code inside always blocks evaluates every frame. Useful for physics, live data, and continuous motion.
```animatix
always {
  let x = slider_value
  ball.position = (x, x^2)
  label.text = format("y = {x}", x)
}
```

**Conditionals**  
Inline conditionals work within expressions.
```animatix
color = if value > 0 { green } else { red }
```

**Loops**  
Loops can be infinite, time-bounded, or count-bounded.
```animatix
loop { ... }
loop 5s { ... }
loop 3 times { ... }
```

**Loop Control**  
Labeled loops can be stopped.
```animatix
job: loop 5 times { ... }
stop job
```

---

## 9. Components

> **Status: Parser/AST only.** `ComponentDef` and related AST nodes exist, but component instantiation/runtime behavior is not implemented yet.

### Definition
The parser currently accepts `pub component ...` definitions. Runtime instantiation is still pending.

```animatix
pub component Button(text: "Click") {
  let x = 1
}
```

### Import
Use the `import` keyword to load external files. Module loading is implemented, but component instantiation from imported component definitions is not.

```animatix
import "button.actor.amx"

btn: Button, text: "Submit"
```

**Lifecycle Hooks**  
Components can define automatic behaviors.
```animatix
on appear { ... }
on disappear { ... }
```

**Custom Actions**  
Components can define callable actions.
```animatix
action collapse(param1: Number) { ... }
collapse btn1
```

---

## 10. Math & Graphs

> **Status: Implemented for `Graph`, `CartesianPlot`, and `PolarPlot`. Parametric and implicit plot types are still future work.**

### Graph Container

The `Graph` primitive is a container that maps logical mathematical domains to physical screen bounds. It does not render directly but establishes the coordinate system for its child plots.

**Properties:**
- `x_domain`: Tuple (min, max) defining the logical x-domain
- `y_domain`: Tuple (min, max) defining the logical y-domain
- `size`: Tuple `(width, height)` defining the physical graph bounds

```animatix
graph: Graph, x_domain: (-5, 5), y_domain: (-10, 30), size: (400, 400)
```

### CartesianPlot

A child plot that renders a function in Cartesian coordinates. The function is defined as a closure using arrow syntax `(args) => expression`.

**Properties:**
- `func`: Closure `(x) => expression` defining the mathematical function to plot
- `color`: Color (optional, defaults to white)
- `width`: Number (optional, stroke width)

```animatix
parabola: CartesianPlot, func: (x) => x^2 + 3, color: red
```

### PolarPlot

A child plot that renders a function in polar coordinates `r(theta)`.

**Properties:**
- `func`: Closure `(theta) => expression` defining the radius as a function of angle
- `t_domain`: Tuple (min, max) defining the sampled angle range
- `color`: Color (optional, defaults to white)
- `width`: Number (optional, stroke width)

```animatix
spiral: PolarPlot, func: (t) => t, color: blue
```

### Closure Syntax

Functions are defined using closure syntax that is parsed in the AST and evaluated by the current runtime:

```animatix
(x) => x^2              // Single parameter
(x, y) => x + y          // Multiple parameters
(t) => sin(t) * cos(t)   // Mathematical expressions
```

Closures can reference values in the current evaluation environment.

### Math Functions

Built-in support for standard math and helpers (implemented):
- `sin(x)`, `cos(x)`
- `lerp(a, b, t)`
- `rand()`
- `format("template {}", value, ...)`

**Text Formatting**  
Dynamic text using format strings.
```animatix
format("Value: {val:.2f}")
```

---

## 11. Namespacing & Access

**Imports and Modules**  
The current module system resolves `import "..."` statements, flattens imported files, and detects cycles.
```animatix
import "./shared.amx"
```

Nested/path/index access exists in the expression AST, but the runtime currently targets labeled actors directly for property assignment and action execution. Rich query syntax remains future-facing.

---

## 12. Editor & Hot-Reloading

File watching and hot-reload for realtime evaluation is intentionally **postponed**. The current architecture prioritizes a clean, stable evaluation model over live file watching. This decision keeps the core evaluation semantics simple and avoids the complexity of invalidation cascades that would arise from mid-evaluation file modifications. 

Future versions will introduce a robust UI/Hot-Reload system that assigns a unique `FileId` to each module in a central module graph, preventing circular dependencies and allowing safe, incremental re-evaluation.
