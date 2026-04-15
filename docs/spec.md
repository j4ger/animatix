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
| Components | dotted assignment targets / rhs property lookup | Yes | Runtime-real | Yes | Yes | Supports nested-label writes and sampled reads such as `left.badge.color` and `echo.radius = right.badge.radius`; nonexistent nested targets now report diagnostics instead of creating orphaned tracks. |
| Components | custom component actions | No current parser surface | Planned | No | Yes | `ast.rs` still contains component-action statement shapes, but `parser.rs` currently reserves `action` and rejects the syntax. |
| Expressions | literals / arithmetic / calls / paths / conditionals | Yes | Runtime-real | Yes | Yes | This is the stable expression core exercised by current runtime tests and examples. |
| Expressions | closures | Yes | Runtime-real | Yes | Yes | Used by current plotting and reactive examples; runtime semantics should still be tightened before VM work. |
| Expressions | `Expr::Method` | AST-defined | Explicit error | Yes | Partial | Present in `ast.rs`, but current runtime evaluation rejects method expressions instead of inventing semantics. |
| Expressions | `Expr::Index` | AST-defined | Explicit error | Yes | Partial | Present in `ast.rs`, but current runtime evaluation rejects index expressions instead of inventing semantics. |
| Expressions | `Expr::Construct` | AST-defined | Explicit error | Yes | Partial | Present in `ast.rs`, but current runtime evaluation rejects inline construct expressions instead of inventing semantics. |
| Primitives | `Text`, `Math`, `Svg`, `Image`, `Circle`, `Dot`, `Rect`, `Square`, `Line`, `Arrow`, `Ellipse`, `Arc`, `Polygon`, `RegularPolygon`, `Path` | Yes | Runtime-real | Yes | Yes | Covered by current docs and runnable examples such as `showcase.amx`, `arc_polygon_path_demo.amx`, `primitive_breadth_demo.amx`, `arrow_demo.amx`, and `image_demo.amx`. |
| Primitives | `Code` | Yes | Runtime-real | Yes | Yes | Shipped as a small v1 primitive rendered through the text-path pipeline; see `examples/code_demo.amx`. |
| Plotting | `Graph`, `CartesianPlot`, `PolarPlot` | Yes | Runtime-real | Yes | Yes | Shipped plotting surface; see `examples/plotting_demo.amx`. |
| Plotting | `ParametricPlot` | Yes | Runtime-real | Yes | Yes | Shipped parametric plot surface using a tuple-return closure over `t_domain`. |
| Plotting | `ImplicitPlot` | No current runtime surface | Planned | No | Yes | Future-facing implicit plotting remains unimplemented. |
| Morphing | re-declaration morphing and current path/text interpolation | Yes | Runtime-real | Yes | Yes | Current runtime supports the core morph path via re-declaration and interpolation. |
| Morphing | DSL modifiers `strategy:auto\|match`, `path_arc`, `stretch` | Yes (scoped) | Runtime-real on timed path-morphing re-declarations | Yes | Yes | Shipped only for timed path-morphing re-declarations; `strategy:fade` remains deferred. |
| Actions | built-ins `fade-in`, `move`, `shift`, `rotate`, `scale`, `draw-in`, `wipe-in`, `fade-out`, `wipe-out` | Yes | Runtime-real | Yes | Yes | These are the currently registered built-in actions. |
| Actions | broader verb-first action surface | Yes | Partial | Partial | Yes | The language shape exists, but only a small built-in subset is currently implemented. |
| Composition | `sequence { ... }` for actions and assignments | Yes | Runtime-real | Yes | Yes | v1a lowers sequence blocks at build time; nested sequence and declarations inside sequence are deliberately unsupported. |
| Composition | `stagger [150ms] { ... }` for actions and assignments | Yes | Runtime-real | Yes | Yes | v1b offsets each child statement by a shared interval; declarations inside stagger are deliberately unsupported. |

### Matrix Conventions

- **Parser = Yes** means the current source surface is accepted by the parser and represented in the AST or statement model.
- **Runtime-real** means the current execution path supports the feature end-to-end in the shipped runtime.
- **Parser-only** means syntax is accepted but the runtime does not execute it.
- **Placeholder** means the current runtime has a stand-in behavior that should not be treated as real semantics.
- **Explicit error** means the parser or AST surface exists, but the runtime intentionally rejects evaluation rather than silently inventing behavior.
- **Planned** means the feature is documented or reserved for future work but is not part of the current executable language surface.
- **Tests = Yes** means the repo has direct automated evidence today, not just a documentation claim.

For executable examples, prefer the curated runnable set in `examples/README.md`. Planned features documented in this spec should not be treated as current runtime guarantees unless they are also backed by runnable examples and tests.

The parser implementation in `crates/animatix/src/parser.rs` is the executable source of truth for accepted syntax. Editor-facing syntax metadata such as the shipped [`tree-sitter-animatix`](../tree-sitter-animatix/) grammar should be treated as a synchronized derivative of that parser surface rather than as an independent language authority.

For Tree-sitter work, this means the grammar should cover only parser-accepted `.amx` syntax, using runnable examples and parser tests as the primary corpus. Removed or dead internal surface area should not be exposed as grammar rules, keywords, or highlight queries. The package-local maintenance workflow lives in [`tree-sitter-animatix/README.md`](../tree-sitter-animatix/README.md).

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
- **Square Brackets (`[ ]`)**: Used for statement modifiers. The parser accepts a generic bracketed modifier list, while runtime support varies by statement kind.  
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

> **Current runtime note:** Actor re-declaration is shipped and now follows the same timing subset as the other shipped modifier hosts: positional duration shorthand plus named `ease`. Unsupported modifier keys are reported explicitly rather than being treated as runtime-real.

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
move badge [to: (120, -30), 1s]
shift badge [by: (40, -24), 1s]
rotate badge [by: 1.5708, 1s]
scale badge [by: 1.5, 1s]
draw-in badge [1s]
fade-out btn [1s]
wipe-out badge [1s]

sequence {
  fade-in btn [400ms]
  move btn [to: (120, -30), 600ms]
  btn.color = red [200ms]
}

stagger [150ms] {
  fade-in a [300ms]
  fade-in b [300ms]
  fade-in c [300ms]
}
```

**Bracket Modifier Model**  
Square brackets use one parser-level shape everywhere they appear: a comma-separated list of positional and/or named modifiers.

```animatix
[2s]
[2s, ease: ease-in-out]
[ease: bounce]
```

The intended design is a **typed declarative modifier bag** with:

1. **One universal shorthand** — the first bare time literal means duration.
2. **A small shared timing vocabulary** — today that is `duration` (via shorthand), `delay`, and `ease`.
3. **Host-specific extension keys** — only when a statement kind explicitly supports them.

The parser accepts generic modifier syntax broadly, but the shipped runtime is narrower:

| Surface | Current runtime support |
|---|---|
| Built-in actions | positional duration + named `delay` + named `ease` |
| Property assignments | positional duration + named `delay` + named `ease` |
| `Text`, `Math`, `Code` declarations | positional duration + named `delay` + named `ease` |
| Actor re-declarations / morph-triggering declarations | positional duration + named `delay` + named `ease` |
| Morph control keys `strategy: auto|match`, `path_arc`, `stretch` | shipped only on timed path-morphing re-declarations |
| `strategy: fade` | deferred |

**Current shipped modifier examples**
```animatix
[2s]                      // Duration shorthand
[delay: 120ms]            // Delay without changing duration
[2s, ease: ease-in-out]  // Duration + easing
[ease: bounce]           // Easing without duration
[delay: 250ms, 0s]       // Delayed instant change
[strategy: match]        // Morph-only modifier on timed path-morphing re-declarations
[path_arc: 1.57]         // Curved interpolation hint for morphing paths
[stretch: true]          // Bounds-normalized morph interpolation
```

**Planned / deferred modifier examples**
```animatix
[strategy: fade]
```

Duplicate shipped modifier keys are reported deliberately and the runtime uses the last provided value. `ease` without an explicit duration remains an instant change. Morph-only keys are rejected or ignored with diagnostics outside timed path-morphing re-declarations.

Unsupported modifier keys are not part of the shipped contract today. The runtime reports them explicitly during build/timeline construction so CLI and GUI tooling can surface the mismatch without pretending the key had an effect.

**Built-in Actions Registry**  
The runtime currently registers nine built-in actions:
- **Motion**: `move`, `shift`, `rotate`, `scale`
- **Entrance / Reveal**: `fade-in`, `draw-in`, `wipe-in`
- **Exit**: `fade-out`, `wipe-out`

`fade-in` is opacity-based and applies broadly across current renderable targets. `draw-in`, `wipe-in`, and `wipe-out` are intentionally narrower vector-first actions; unsupported targets such as images or text-like actors report diagnostics instead of silently pretending the action had an effect.

`move` is a target-based local motion action. Its required `to` modifier sets the target's local translation offset on top of existing placement.

`shift` is a local motion action. Its required `by` modifier applies a relative translation on top of the target's existing placement rather than changing layout ownership or rebinding `at` directly.

`rotate` is a local motion action. Its required `by` modifier applies a relative rotation in radians around the target's current local origin after placement and local motion offset are resolved.

`scale` is a local motion action. Its required `by` modifier applies a positive uniform scale factor on top of the target's current local transform. In the current runtime this is visual-only: it changes rendering, not layout extents.

The action system exposes action signatures through the Rust registry API, which is enough for editor/LSP integration work. Higher-level editor workflows remain future work.

**Composition v1a: Sequence Blocks**  
`sequence { ... }` is the currently shipped composition helper. It is intentionally narrow in v1a: the body may contain only actions and property assignments, and it lowers at build time by advancing each statement's start time by the previous statement's full span (`delay + duration`).

```animatix
sequence {
  fade-in badge [400ms]
  shift badge [by: (80, 0), 600ms, ease: ease-in-out]
  badge.color = blue [250ms, delay: 100ms]
}
```

Nested `sequence` blocks and declarations inside `sequence` are intentionally rejected with diagnostics in this first slice.

**Composition v1b: Stagger Blocks**  
`stagger [150ms] { ... }` is the current stagger helper. Like `sequence`, it is intentionally narrow: the body may contain only actions and property assignments. Instead of chaining total spans, stagger offsets each item by a shared interval from the parent keyframe time.

```animatix
stagger [150ms] {
  fade-in first [300ms]
  fade-in second [300ms]
  fade-in third [300ms]
}
```

`stagger [each: 150ms] { ... }` is also accepted. Nested `stagger` blocks and declarations inside `stagger` are intentionally rejected with diagnostics in this slice.

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

**Current Morph Modifier Status**  
The runtime morphs vector path data when a supported actor is re-declared. Property assignments and actor re-declarations now share the same shipped timing subset: positional duration shorthand plus named `delay` plus named `ease`. Timed path-morphing re-declarations can also use `strategy: auto|match`, `path_arc`, and `stretch`. Unsupported or out-of-context modifier keys are reported explicitly during build/timeline construction instead of being treated as silently supported morph controls.

**Shipped Morph Strategy Modifiers**  
The currently shipped scoped morph modifiers are:
```animatix
[strategy: auto]          // Engine decides (default)
[strategy: match]         // Force point alignment
[path_arc: 1.57]          // Curved interpolation hint during path morphing
[stretch: true]           // Bounds-normalized morph interpolation
```

These keys are intentionally narrower than parser acceptance: they are runtime-real only for timed path-morphing re-declarations. `strategy: fade` is intentionally deferred for now because it implies overlapping source/target visual states rather than a single path interpolation contract.

**Instant Change**
Use zero duration or property assignment for instant updates.
```animatix
btn: Button, text: "New" [0s]
btn.text = "New"
```

For the current runtime, choose between actor re-declarations and property assignments based on the kind of change you want to author, not because one path has a more complete timing contract than the other.

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

Component bodies may contain actor declarations, assignments, and control flow statements that get inlined into each instance at expansion time.

```animatix
pub component MetricCard(title: "Metric") {
    frame: Rect, size: (240, 120), color: blue
    title_text: Text { text: title, at: (0, -20) }
    badge: Circle, radius: 12, color: gold
}
```

### Import
Use the `import` keyword to load external files. Imported `pub component` definitions are visible to the importing file and can be instantiated through normal actor declaration syntax.

```animatix
import "button.actor.amx"

btn: Button, text: "Submit"
```

**Shipped MVP behavior:**
- Imported components must be declared `pub` to be visible across files
- Instance props bind to component params by name
- Nested labels within a component are instance-prefixed when expanded (e.g., `card.badge.color` inside a `MetricCard` definition becomes `card.badge.color` in the scene after expansion)
- Repeated component instances each get isolated nested labels, preventing collisions
- External dotted assignment targets work against nested labels: `left.badge.color = red` updates the prefixed `left.badge` track
- Rhs dotted property lookup samples from nested labels: `echo.at = right.badge.at` reads from the expanded `right.badge.at`

**What remains future-facing:**
- Custom component actions (the `action ...` syntax is not yet accepted by the parser)
- Richer namespace/export controls beyond simple `pub` visibility
- Parameterized component exports or component-level configuration syntax

**Phase 3C — Authoring Patterns:**

For reusable imported components, follow these recommended patterns:

1. **Parameter-driven configuration** — Use component parameters to configure appearance rather than accessing internals:
```animatix
// Preferred: configure via params
card: MetricCard, title: "Latency"

// Avoid: reaching into nested labels for basic config
```

2. **Property forwarding via rhs lookup** — Read nested properties using dotted paths on the right-hand side:
```animatix
// Copy a nested property from one component to another
echo: Circle, radius: right.badge.radius, color: right.badge.color, at: right.badge.at
```

3. **External dotted assignment** — Update nested labels from outside using multi-segment paths:
```animatix
// Update a nested label's property in an existing instance
left.badge.color = red
left.frame.color = (0.12, 0.28, 0.58, 1.0)
```

4. **Multiple instances with isolated namespaces** — Each instance gets independent nested labels:
```animatix
first: MetricCard, title: "Latency"
second: MetricCard, title: "Throughput"
// first.badge and second.badge are completely independent
```

**Namespace and Reachability Rules:**

The following rules define what is and is not accessible from outside a component instance:

| Rule | Behavior |
|------|----------|
| Instance label | Always reachable: `card`, `left`, `right` |
| Nested actor label | Always reachable when the nested actor exists in the component body: `card.badge`, `left.frame` |
| Non-existent nested label | Creates an orphaned track entry with empty vector/text paths (no runtime error) |
| Property on any track | Assignable without pre-declaration; creates the property track if it does not exist |

**Reachability Example:**
```animatix
pub component MetricCard(title: "Metric") {
    frame: Rect, size: (240, 120), color: blue
    badge: Circle, radius: 12, color: gold
}

card: MetricCard, title: "Latency"

#0s
card.badge.color = red      # OK: badge exists in component
card.nonexistent.color = blue  # Creates orphaned card.nonexistent track
```

```animatix
# NOT YET SUPPORTED — custom component actions
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

The current runtime supports dotted assignment targets for nested labels that already exist as runtime actors, including labels generated by imported component expansion. If a dotted assignment target does not resolve to a declared nested actor, the runtime now reports a build diagnostic and ignores that assignment instead of creating an orphaned track.

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
