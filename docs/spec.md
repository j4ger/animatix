# Animatix Language Specification

> This document describes the current implemented language surface first. Status callouts distinguish shipped runtime behavior from parser-only or planned syntax.

---

## Language Status Matrix

This matrix is the quick-reference status view for the current language surface.

| Area | Surface | Parser | Runtime | Tests | Docs | Notes |
|---|---|---|---|---|---|---|
| Reactive | `always` | Yes | Runtime-real | Yes | Yes | Shipped stateless reactive model; see `docs/stateless_reactive_design.md` and `examples/reactive_runtime.amx`. |
| Reactive | `for` | Yes | Runtime-real | Yes | Yes | Structural expansion is treated as compile-time/timeline-build behavior. |
| Reactive | `loop` / `yield` / `stop` / `pause` / `resume` | Rejected | Removed | Yes | Yes | Explicitly removed from the shipped model and covered by negative parser tests. |
| Components | imported `pub component` instantiation | Yes | Runtime-real | Yes | Yes | Public imported components expand and instantiate through `module.rs`; see `examples/component_modules_demo.amx`. |
| Components | parameter binding + nested-label isolation | Yes | Runtime-real | Yes | Yes | Covered by module/timeline tests and current component demo. |
| Components | dotted assignment targets / rhs property lookup | Yes | Runtime-real | Yes | Yes | Supports nested-label writes and sampled reads such as `left.badge.color` and `echo.radius = right.badge.radius`. |
| Components | custom component actions | No current parser surface | Planned | No | Yes | `ast.rs` still contains component-action statement shapes, but `parser.rs` currently reserves `action` and rejects the syntax. |
| Expressions | literals / arithmetic / calls / paths / conditionals | Yes | Runtime-real | Yes | Yes | This is the stable expression core exercised by current runtime tests and examples. |
| Expressions | closures | Yes | Runtime-real | Yes | Yes | Used by current plotting and reactive examples; runtime semantics should still be tightened before VM work. |
| Expressions | `Expr::Method` | AST-defined | Explicit error | Yes | Partial | Present in `ast.rs`, but current runtime evaluation rejects method expressions instead of inventing semantics. |
| Expressions | `Expr::Index` | AST-defined | Explicit error | Yes | Partial | Present in `ast.rs`, but current runtime evaluation rejects index expressions instead of inventing semantics. |
| Expressions | `Expr::Construct` | AST-defined | Explicit error | Yes | Partial | Present in `ast.rs`, but current runtime evaluation rejects inline construct expressions instead of inventing semantics. |
| Primitives | `Text`, `Math`, `Svg`, `Image`, `Circle`, `Rect`, `Line`, `Ellipse`, `Arc`, `Polygon`, `Path` | Yes | Runtime-real | Yes | Yes | Covered by current docs and runnable examples such as `showcase.amx`, `arc_polygon_path_demo.amx`, and `image_demo.amx`. |
| Primitives | `Code` | Yes | Runtime-real | Yes | Yes | Shipped as a small v1 primitive rendered through the text-path pipeline; see `examples/code_demo.amx`. |
| Plotting | `Graph`, `CartesianPlot`, `PolarPlot` | Yes | Runtime-real | Yes | Yes | Shipped plotting surface; see `examples/plotting_demo.amx`. |
| Plotting | `ParametricPlot`, `ImplicitPlot` | No current runtime surface | Planned | No | Yes | Future-facing plotting types documented as not yet implemented. |
| Morphing | re-declaration morphing and current path/text interpolation | Yes | Runtime-real | Yes | Yes | Current runtime supports the core morph path via re-declaration and interpolation. |
| Morphing | DSL modifiers `strategy`, `path_arc`, `stretch` | Planned surface only | Planned | No | Yes | Documented as future-facing controls, not wired into the runtime today. |
| Actions | built-ins `fade-in`, `wipe-in`, `fade-out` | Yes | Runtime-real | Yes | Yes | These are the currently registered built-in actions. |
| Actions | broader verb-first action surface | Yes | Partial | Partial | Yes | The language shape exists, but only a small built-in subset is currently implemented. |

### Matrix Conventions

- **Parser = Yes** means the current source surface is accepted by the parser and represented in the AST or statement model.
- **Runtime-real** means the current execution path supports the feature end-to-end in the shipped runtime.
- **Parser-only** means syntax is accepted but the runtime does not execute it.
- **Placeholder** means the current runtime has a stand-in behavior that should not be treated as real semantics.
- **Explicit error** means the parser or AST surface exists, but the runtime intentionally rejects evaluation rather than silently inventing behavior.
- **Planned** means the feature is documented or reserved for future work but is not part of the current executable language surface.
- **Tests = Yes** means the repo has direct automated evidence today, not just a documentation claim.

For executable examples, prefer the curated runnable set in `examples/README.md`. Planned features documented in this spec should not be treated as current runtime guarantees unless they are also backed by runnable examples and tests.

The parser implementation in `crates/animatix/src/parser.rs` is the executable source of truth for accepted syntax. Editor-facing syntax metadata such as a future Tree-sitter grammar should be treated as a synchronized derivative of that parser surface rather than as an independent language authority.

For Tree-sitter work, this means the grammar should cover only parser-accepted `.amx` syntax, using runnable examples and parser tests as the primary corpus. Removed or dead internal surface area should not be exposed as grammar rules, keywords, or highlight queries.

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
- **Hash (`#`)**: Marks a keyframe in the timeline. Plain `#time` is an absolute keyframe and `#+time` is a relative keyframe.  
  *Example:* `#0s`, `#2.5s`, `#+1s`
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
#0s
#2.5s
#500ms
```

**Relative Keyframes**  
Marks a time relative to the previous keyframe.
```animatix
#+1s
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
`Arc`, `Polygon`, `Path`, and `Image` are now implemented runtime primitives.

- `Arc` is a stroke-first primitive using `radius_x`, `radius_y`, `start_angle`, and `sweep_angle`.
- `Polygon` is built around explicit `points` input rather than higher-level helpers like `sides`.
- `Path` is built around structured `commands` such as `move_to(...)`, `line_to(...)`, `quad_to(...)`, `curve_to(...)`, and `close()`.
- `Image` is a file-backed raster primitive using `url`, `at`, and optional `size`.

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

> **Status: Implemented.** The shipped reactive model is stateless `always` evaluation plus compile-time `for` expansion.

**Always Blocks**  
Code inside `always` is evaluated from the requested time and current sampled scene state rather than from hidden prior-frame execution state.
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

Typical uses:
- continuous motion from `t`
- stateless composition from sampled actor properties
- finite or infinite periodic behavior expressed with explicit time math

```animatix
always {
  pulse.size = if (t % 1.0) < 0.5 { (120, 120) } else { (180, 180) }
}
```

**Structural Repetition (`for`)**  
Use `for` for compile-time structural expansion.

```animatix
for item in items {
  // generate repeated structure during timeline build
}
```

`for` is the shipped construct for repeated object creation, component instantiation, and elaboration-style generation.

### Shipped Direction

The language model is:
- `for` for structure
- keyframes for declarative timed animation
- `always` for stateless runtime behavior

Repeated runtime behavior should be expressed with explicit time math inside `always`, not with hidden interpreter state.

---

## 9. Components

> **Status: Partially implemented.** Imported `pub component` definitions can now be instantiated at runtime with parameter binding and instance-prefixed nested labels. Custom component actions remain future-facing and are not part of the current parser surface.

### Definition
The parser accepts `pub component ...` definitions, and the runtime now expands imported public components into ordinary scene statements before timeline build.

```animatix
pub component Button(text: "Click") {
  let x = 1
}
```

### Import
Use the `import` keyword to load external files. Imported `pub component` definitions are visible to the importing file and can be instantiated through normal actor declaration syntax.

```animatix
import "button.actor.amx"

btn: Button, text: "Submit"
```

Current MVP behavior:
- imported components must be declared with `pub`
- instance props bind to component params by name
- nested labels are instance-prefixed to avoid collisions across repeated uses
- nested labels can be targeted with dotted assignment paths such as `card.badge.color = red`
- nested labels can also be queried on the rhs through sampled property paths such as `copy.at = card.badge.at`
- custom component actions remain future-facing and are not currently accepted by the parser

**Custom Actions**  
Custom component actions also remain planned syntax. The current AST still reserves space for them, but `parser.rs` currently rejects `action ...` forms.
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

Current runtime contract:
- closure parameters bind by name at call time
- the closure body is evaluated against a clone of the current call-time environment with those parameter bindings added
- free variables therefore resolve from the environment that exists when the closure is invoked, not from a separately stored lexical snapshot

This is the contract future execution work should preserve unless the runtime model is intentionally changed and re-tested.

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

The current runtime supports dotted assignment targets for nested labels that already exist as runtime actors, including labels generated by imported component expansion.

```animatix
left.badge.color = red
right.frame.radius = 20
```

This is intentionally narrower than a full object/query system:
- assignment targets are interpreted as `label.path.property`
- the final segment is the property name
- the earlier segments resolve to the runtime actor label
- rhs dotted paths are resolved as flat dotted lookup keys in the runtime environment rather than recursive object traversal
- the runtime seeds sampled actor/scene properties under those dotted keys, such as `node.at`, `node.radius`, `node.color`, `scene.background_color`, and vector/color component keys like `node.at.x`, `node.at.y`, `node.color.r`, `node.color.g`, `node.color.b`, `node.color.a`
- rich object-style traversal, index-based access, and method-style query composition remain future-facing

---

## 12. Editor & Hot-Reloading

File watching and hot-reload for realtime evaluation is intentionally **postponed**. The current architecture prioritizes a clean, stable evaluation model over live file watching. This decision keeps the core evaluation semantics simple and avoids the complexity of invalidation cascades that would arise from mid-evaluation file modifications. 

Future versions will introduce a robust UI/Hot-Reload system that assigns a unique `FileId` to each module in a central module graph, preventing circular dependencies and allowing safe, incremental re-evaluation.
