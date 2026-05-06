# Animatix Language Specification

> This document describes the implemented language surface. Status callouts distinguish shipped runtime behavior from parser-only or planned syntax. Runnable examples are in `examples/README.md`.

---

## Language Status Matrix

| Area | Surface | Parser | Runtime | Tests | Docs | Notes |
|---|---|---|---|---|---|---|
| Reactive | `always` | Yes | Runtime-real | Yes | Yes | Shipped stateless reactive model |
| Reactive | `for` | Yes | Runtime-real | Yes | Yes | Compile-time structural expansion |
| Reactive | `loop` / `yield` / `stop` / `pause` / `resume` | Rejected | Removed | Yes | Yes | Explicitly removed |
| Components | `pub component` instantiation | Yes | Runtime-real | Yes | Yes | Via `module.rs`; see `examples/component_modules_demo.amx` |
| Components | parameter binding + nested-label isolation | Yes | Runtime-real | Yes | Yes | Module/timeline tests |
| Components | dotted assignment targets / rhs property lookup | Yes | Runtime-real | Yes | Yes | Nested-label writes; nonexistent targets report diagnostics |
| Components | custom component actions | No | Planned | No | Yes | Reserved but rejected by parser |
| Modules | `pub let` exports | Yes | Runtime-real | Yes | Yes | Exported values from `.amx` files; see `examples/module_reuse_demo.amx`. |
| Modules | `import ... as` namespaced imports | Yes | Runtime-real | Yes | Yes | Aliased imports create namespaces for qualified access (`theme.accent`). |
| Expressions | literals / arithmetic / calls / paths / conditionals | Yes | Runtime-real | Yes | Yes | Stable expression core |
| Expressions | closures | Yes | Runtime-real | Yes | Yes | Used by plotting/reactive examples |
| Expressions | `Expr::Method` | AST-defined | Explicit error | Yes | Partial | Rejected at runtime |
| Expressions | `Expr::Index` | AST-defined | Explicit error | Yes | Partial | Rejected at runtime |
| Expressions | `Expr::Construct` | AST-defined | Explicit error | Yes | Partial | Rejected at runtime |
| Primitives | All shapes (`Text`, `Math`, `Svg`, `Image`, `Circle`, `Rect`, `Line`, `Arrow`, `Ellipse`, `Arc`, `Polygon`, `Path`, etc.) | Yes | Runtime-real | Yes | Yes | See `showcase.amx`, `arc_polygon_path_demo.amx`, `primitive_breadth_demo.amx`, `arrow_demo.amx`, `image_demo.amx` |
| Primitives | `Code` | Yes | Runtime-real | Yes | Yes | See `examples/code_demo.amx` |
| Plotting | `Graph`, `CartesianPlot`, `PolarPlot` | Yes | Runtime-real | Yes | Yes | See `examples/plotting_demo.amx` |
| Plotting | `ParametricPlot`, `ImplicitPlot` | Yes | Runtime-real | Yes | Yes | Parametric uses tuple-return closure; implicit uses `(x, y) => scalar` |
| Morphing | re-declaration morphing + path/text interpolation | Yes | Runtime-real | Yes | Yes | Core morph path via re-declaration |
| Morphing | `strategy:auto\|match`, `path_arc`, `stretch` | Yes (scoped) | Runtime-real on timed path-morphing | Yes | Yes | `strategy:fade` deferred |
| Actions | Entrance: `fade-in`, `draw-in`, `wipe-in`; Motion: `move`, `shift`, `rotate`, `scale`; Exit: `fade-out`, `wipe-out`, `reveal-out`, `draw-out`; Effects: `shake`, `pulse`, `bounce`; Reorder: `swap` | Yes | Runtime-real | Yes | Yes | Currently registered built-ins |
| Actions | broader verb-first surface | Yes | Partial | Partial | Yes | Shape exists; small subset implemented |
| Composition | `sequence { ... }` | Yes | Runtime-real | Yes | Yes | v1a lowers at build time; nested seq/decls unsupported |
| Composition | `stagger [150ms] { ... }` | Yes | Runtime-real | Yes | Yes | v1b offsets by shared interval; nested stagger/decls unsupported |
| Colorscheme | `Colorscheme "name" { ... }` | Yes | Runtime-real | Yes | Yes | Native AMX primitive with `extends` |
| Components | `@slot` markers with named slot fills | Yes | Runtime-real | Yes | Yes | Component-internal containers with `@slot`; instantiation via `@slotname { items }` |

**Key:** Parser=Yes (syntax accepted), Runtime-real (end-to-end execution), Tests=Yes (automated evidence exists).

---

## 1. File Types

- **`.amx`**: Main source files loaded via `import "..."`. Suffix conventions (`.actor.amx`, `.lib.amx`) are stylistic, not distinct runtime kinds.

---

## 2. Core Syntax Symbols

| Symbol | Use | Example |
|--------|-----|---------|
| `let` | Variable/actor declaration | `let x = 0` |
| `:` | Actor binding + scene placement | `btn: Button, text: "OK"` |
| `#` | Keyframe (absolute `#0s` or relative `#+1s`) | `#2.5s`, `#+1s` |
| `{ }` | Container children, arrays, block scopes | `Row { Item1, Item2 }` |
| `@` | Slot marker / slot fill prefix | `@slot` (definition), `@header { ... }` (fill) |
| `[ ]` | Statement modifiers (duration, delay, ease) | `[2s, ease: bounce]` |
| `=` | Property assignment (instant or animated) | `btn.color = red` |
| `,` | Separates object properties | `Type, prop: val` |
| ` ` | Separates action verb from arguments | `fade-in btn [1s]` |
| `.` | Nested access / namespacing | `container.child` |

---

## 3. Declarations

**Actor Declaration** (rendered objects):
```animatix
label: Type, property: value
```

**Variable Declaration** (computed, non-rendered):
```animatix
let name = expression
```

**Re-Declaration (Morph Trigger):**
Re-declaring an existing label at a later keyframe triggers morph transition.
```animatix
#0s
btn: Button, text: "OK"

#2s
btn: Button, text: "Submit" [2s]
```
> **Runtime note:** Re-declaration follows the same timing subset as other modifiers: positional duration shorthand + named `ease`. Unsupported modifier keys are reported explicitly.

**Implicit Objects:**
```animatix
#0s
scene.background_color = black  // default

#2s
scene.background_color = white [2s]
```

---

## 4. Timeline & Keyframes

**Absolute:** `#0s`, `#2.5s`, `#500ms`  
**Relative:** `#+1s`

Actions under the same keyframe execute **simultaneously**:
```animatix
#2s
fade-in A [1s]
color B to red [2s]
```

The timeline maintains a hierarchical `scene_graph`; transforms/opacities cascade via depth-first search.

---

## 5. Color System

Colorscheme v1 surface:
- `config { colorscheme: "default-dark" | "default-light" | "editorial-dark" }`
- Aliases via `color:` on text/code/math/actor declarations, `stroke:` on actor declarations
- `color: auto` for deterministic automatic colorscheme assignment
- Primitive-type defaults when no explicit color/stroke provided

**Color precedence (lowest to highest):**
1. Runtime hardcoded default (white)
2. Colorscheme primitive-type defaults
3. Alias-based declaration defaults (`color: text.primary`)
4. `color: auto` from scheme auto pool
5. Explicit declaration values
6. Later timed assignments
7. Frame-local `always` overrides

**Primitive-type defaults:** Text-like (`Text`, `Math`, `Code`) → `text.primary`; shape fills → `surface.primary`; shape strokes (`Line`, `Arrow`, `Arc`) → `stroke.default`; plot curves → `accent.primary`.

```animatix
config { colorscheme: "editorial-dark" }
title: Text, text: "Hello"           // color: text.primary
panel: Rect, size: (200, 100)       // color: surface.primary
badge: Circle, radius: 20, color: auto
```

`Colorscheme` primitive with `extends` inheritance is supported. See [`colorscheme_design.md`](colorscheme_design.md).

---

## 6. Actions & Modifiers

**Modifier Syntax:** Square brackets accept a comma-separated list of positional and/or named modifiers.

- **Universal shorthand:** first bare time literal = duration
- **Shared timing vocabulary:** `duration` (shorthand), `delay`, `ease`
- **Host-specific keys:** when a statement kind explicitly supports them

**Shipped modifier examples:**
```animatix
[2s]                      // Duration shorthand
[2s, ease: ease-in-out]  // Duration + easing
[ease: bounce]           // Easing only (instant change)
[delay: 250ms, 0s]       // Delayed instant change
[path_arc: 1.57]         // Morph control (path-morphing only)
[stretch: true]           // Bounds-normalized morph
```

**Runtime support by statement kind:**

| Surface | Support |
|---------|---------|
| Built-in actions | positional duration + `delay` + `ease` |
| Property assignments | positional duration + `delay` + `ease` |
| Actor re-declarations | positional duration + `delay` + `ease` |
| `Text`/`Math`/`Code` declarations | positional duration + `delay` + `ease` |
| Morph keys (`strategy`, `path_arc`, `stretch`) | timed path-morphing re-declarations only |

Duplicate modifier keys: last value wins. `ease` without duration = instant change.

**Built-in Actions:**
- **Motion:** `move`, `shift`, `rotate`, `scale`
- **Entrance:** `fade-in`, `draw-in`, `wipe-in`
- **Exit:** `fade-out`, `wipe-out`, `reveal-out`, `draw-out`
- **Effects:** `shake`, `pulse`, `bounce`
- **Reorder:** `swap`

**Rotation:** Two ways to rotate:
- `rotate item [by: angle, duration]` - Visual transform (applies to actor)
- `item.angle = value [duration]` - Geometry rotation (path generation)

Vector reveal actions (`draw-in`, `wipe-in`, `wipe-out`, `reveal-out`, `draw-out`) are **leaf-only**; containers/groups report diagnostics.

**Effects actions** add emphasis and attention animations:
- `shake [intensity: N]` - Rapid oscillating horizontal motion
- `pulse [intensity: N]` - Scale up then return to normal
- `bounce [intensity: N]` - Elastic bounce motion

```animatix
fade-in btn [1s]
move badge [to: (120, -30), 1s]
shift badge [by: (40, -24), 1s]
rotate badge [by: 1.5708, 1s]
scale badge [by: 1.5, 1s]
draw-in badge [1s]
fade-out btn [1s]
wipe-out badge [1s]
```

Effects examples:
```animatix
shake badge [intensity: 2]
pulse btn [intensity: 1.5]
bounce badge [intensity: 3]
```

**Reorder:**

`swap childA, childB [duration]` — Swaps the layout positions of two children in their parent container. Both targets must share a common parent and be `LayoutManaged`. Requires `dynamic_layout: true`.

```animatix
config { dynamic_layout: true }

row: Row, gap: 8 {
  a: Rect, size: (30, 40)
  b: Rect, size: (30, 80)
  c: Rect, size: (30, 60)
}

#1s
swap a, b [500ms]

#2s
swap b, c [500ms]
```

Overlapping swaps on the same container are disallowed and emit a diagnostic.

**How it works:** The `swap` action writes a keyframe to a per-container `child_orders` track at `time + duration`. During evaluation, if the current time falls between two child-order keyframes, layout positions are computed for both orders and interpolated with easing. This produces smooth sliding motion without `motion_offset` hacks.

**Future: `reorder`**

A planned `reorder` action would allow explicit full-order specification (e.g. reversing a container), independent of swap history. Unlike `swap`, it could support overlapping transitions by capturing a snapshot of the current order at action start time.

```animatix
# Reverse a row
reorder row [c, b, a] [500ms]
```

**Composition:**

`sequence { ... }` — chains statements by cumulative span:
```animatix
sequence {
  fade-in badge [400ms]
  shift badge [by: (80, 0), 600ms]
  badge.color = blue [250ms, delay: 100ms]
}
```

`stagger [150ms] { ... }` — offsets each child by shared interval:
```animatix
stagger [150ms] {
  fade-in a [300ms]
  fade-in b [300ms]
  fade-in c [300ms]
}
```

Nested `sequence`/`stagger` and declarations inside either block are rejected.

---

## 7. Morphing System

**Automatic Morph:** Re-declaration at a new keyframe triggers path interpolation.
```animatix
#0s
circle: Circle, at: (0, 0)

#2s
circle: Circle, at: (100, 100) [2s]
```

**Shipped morph modifiers** (timed path-morphing only):
```animatix
[2s, strategy: auto]     // Engine decides (default)
[2s, strategy: match]   // Force point alignment
[2s, path_arc: 1.57]    // Curved interpolation hint
[2s, stretch: true]     // Bounds-normalized interpolation
```
`strategy: fade` deferred (implies overlapping states, not single path interpolation).

**Instant Change:** Zero duration or property assignment.
```animatix
btn: Button, text: "New" [0s]
btn.text = "New"
```

**Property-Level Animation:**
```animatix
morpher.size = (100, 100) [2s, ease: ease-out]
```

---

## 8. Containers & Layout

> **See [`layout_design.md`](layout_design.md) for full details.**

Implemented: `Row`, `Col`, `Grid`, `Stack`, `Group`.

- **Row/Col:** `gap` (spacing), `align` ("start" | "center" | "end")
- **Grid:** `cols` + `gap`
- **Stack:** Overlapping children around shared origin
- **Group:** Non-layout grouping/transform inheritance

**Declaration-time measure/place contract:** Layout containers consume each child's `size` track at timeline build; children with explicit `at` opt into manual placement instead.

```animatix
row: Row, gap: 12, align: "center" {
  Rect, color: red
  Circle, color: blue
}
```

**Scene anchors:** `scene.top_left`, `scene.top`, `scene.center`, etc.  
**Percentage placement:** `at: (82%, 76%)`

---

## 9. Primitive Types

**Shapes:** `Circle`, `Dot`, `Rect`, `Square`, `Line`, `Arrow`, `Ellipse`, `Arc`, `Polygon`, `RegularPolygon`, `Path`

**Text-like:** `Text`, `Math`, `Code`, `Svg`, `Image`

**Path commands:** `move_to(...)`, `line_to(...)`, `quad_to(...)`, `curve_to(...)`, `close()`

```animatix
arc: Arc, radius_x: 50, radius_y: 50, start_angle: 0, sweep_angle: 3.14
poly: Polygon, points: [(0,0), (100,0), (50,100)]
path: Path, commands: [move_to(0, 0), line_to(100, 100), close()]
img: Image, url: "photo.png", at: (100, 100), size: (200, 150)
```

**Text shorthand:**
```animatix
title: "Hello"                    // desugars to: title: Text, text: "Hello"
title: "Hello" [2s, ease: bounce] // with modifiers
```

---

## 10. Reactive System

**`always`**: Evaluated from requested time + current sampled scene state.
```animatix
always {
  let x = slider_value
  ball.position = (x, x^2)
  label.text = format("y = {x}", x)
}
```

**`for`**: Compile-time structural expansion.
```animatix
for item in items {
  // generate repeated structure
}
```

**Conditionals** in expressions:
```animatix
color = if value > 0 { green } else { red }
pulse.size = if (t % 1.0) < 0.5 { (120, 120) } else { (180, 180) }
```

Model: `for` for structure, keyframes for declarative timed animation, `always` for stateless runtime behavior.

---

## 11. Imports, Modules & Namespaces

**Non-aliased imports:** `import "path"` flattens the imported file's statements into the current scene. This is the backward-compatible behavior.

**Aliased imports:** `import "path" as name` loads the file but does NOT flatten its statements. Instead, it collects `pub let` exports and makes them available as `name.export_name`.

```animatix
import "./theme.amx" as theme

panel: Rect, size: (200, 100), color: theme.accent
```

**Exporting values:** Use `pub let` to export named values from a module:

```animatix
pub let accent = (0.38, 0.78, 1.0, 1.0)
pub let background = (0.04, 0.06, 0.09, 1.0)
```

Non-`pub` `let` declarations remain local to the file.

**Current limitations:**
- Namespace access is one level: `alias.export_name`
- Re-exports (a module exporting values from its own imports) are not supported
- Export values are evaluated at build time in the importing scene's environment
- Property assignment for `text`/`latex`/`math`/`code` stores the value but does not trigger re-compilation of text paths at render time; infrastructure is in place but render-time font compilation is deferred.

---

## 12. Components

**Definition:**
```animatix
pub component MetricCard(title: "Metric") {
    frame: Rect, size: (240, 120), color: blue
    title_text: Text { text: title, at: (0, -20) }
    badge: Circle, radius: 12, color: gold
}
```

**Import and instantiation:**
```animatix
import "button.actor.amx"
btn: Button, text: "Submit"
```

**Shipped behavior:**
- `pub` required for cross-file visibility
- Instance props bind to component params by name
- Nested labels are instance-prefixed (isolated per instance)
- External dotted assignment: `left.badge.color = red`
- RHS dotted lookup: `echo.at = right.badge.at`

**Non-existent nested targets** report `UnknownTargetPath` diagnostics and are ignored (no orphaned tracks).

**Future-facing:** Custom component actions (`action ...` syntax), richer namespace controls.

### 12.1 Slots

Slots allow component authors to declare fillable regions that can be customized at instantiation time.

**Declaring slots:** Place the `@slot` marker inside a container's children block.

```animatix
pub component SlideLayout {
  config { colorscheme: "editorial-dark", resolution: (1280, 720) }
  #0s
  backdrop: Rect, size: fill, color: scene.background, anchor: scene.center

  // Required slot — error if not filled:
  header: Col, anchor: scene.top {
    @slot
  }

  // Optional slot — non-@slot children serve as defaults:
  footer: Col, anchor: scene.bottom {
    @slot
    text: Text, text: "Default footer", font_size: 14
  }
}
```

**Filling slots:** Use `@slotname { items }` inside the component instantiation block.

```animatix
slide: SlideLayout {
  @header {
    title: Text, text: "My Title", font_size: 48
  }
  // footer uses the default "Default footer" text
}
```

**Shipped behavior:**
- `@slot` marks a container as fillable within a component definition
- Instance `@slotname { items }` fills the slot by container label
- Non-`@slot` siblings in the container act as defaults when the slot is not filled
- Slot content participates in the component's keyframe structure
- Components can be used as slot content (recursive expansion)
- `@slot` inside `for`, `if`, `always`, `sequence`, or `stagger` blocks is disallowed

**Current scope:**
- Slots are resolved at compile time during component expansion
- Unfilled required slots (no fill + no defaults) produce an empty container at build time
- Slots are matched by container label (named, positional-independent)

---

## 13. Math & Graphs

**`Graph`**: Container mapping logical domains to physical bounds.
```animatix
graph: Graph, x_domain: (-5, 5), y_domain: (-10, 30), size: (400, 400)
```

**`CartesianPlot`**: Function `(x) => expression`
```animatix
parabola: CartesianPlot, func: (x) => x^2 + 3, color: red
```

**`PolarPlot`**: Function `(theta) => expression` with `t_domain`
```animatix
spiral: PolarPlot, func: (t) => t, color: blue
```

**Closures:**
```animatix
(x) => x^2           // single param
(x, y) => x + y      // multiple params
```

**Built-in math:** `sin(x)`, `cos(x)`, `lerp(a, b, t)`, `rand()`, `format("template {}", value, ...)`

---

## 14. Namespacing & Access

**Imports:**
```animatix
import "./shared.amx"
```

**Dotted assignment targets** resolve to runtime actors; final segment = property name.
```animatix
left.badge.color = red
right.frame.radius = 20
```

**Seeded property paths:** `node.at`, `node.radius`, `node.color`, `scene.background_color`, `node.at.x`, `node.at.y`, `node.color.r/g/b/a`.

**Unresolved rhs paths** report build diagnostics; host property keeps its default/fallback value.

**Future-facing:** Rich object-style traversal, index-based access, method-style query composition.

---

## 15. Known Gaps & Limitations

- **Re-declaration for Morphing/Media:** Morphing text or updating SVG/Image sources currently requires re-declaring the entire object at a new keyframe, breaking standard property assignment syntax.
- **Asymmetrical Reveal/Exit Actions:** Standard fade-out or cross-fade behaviors on some primitives and containers remain incomplete or non-intuitive compared to entrance counterparts.
- **Static Geometry:** Structural geometry inputs like `Polygon.points` and `Path.commands` are declaration-time only and cannot be animated dynamically frame-by-frame.
- **Missing Rotation:** Basic shapes like `Ellipse` do not currently support a dedicated rotation parameter, limiting fundamental shape manipulations without resorting to matrix actions.
- **Coordinate System Friction:** `at` (absolute coordinates) and `anchor`/`offset` (layout-based coordinates) often clash, requiring significant manual intervention when mixing them.

---

## 16. CLI Export

**Video (`animatix video`) and GIF (`animatix gif`) exports:**

| Flag | Default | Behavior |
|------|---------|----------|
| `--duration` | *auto* | Omit to use timeline length + hold |
| `--hold` | 1.0 | Trailing hold in seconds; ignored when `--duration` is set |
| `--fps` | 30 (video), 15 (GIF) | Output framerate |
| `--width` / `--height` | 1280x720 (video), 640x360 (GIF) | Output resolution |

**Auto-duration:** If `--duration` is omitted, the CLI builds the timeline, reads `Timeline::duration_seconds()` (the time of the last keyframe across all tracks, background, and child-order animations), and adds a trailing hold (configurable via `--hold`, default **1.0s**). This prevents the export from cutting off bluntly at the last animation's end frame.

```bash
# Export full timeline + 1s hold (auto-detected ~10.9s for swap_demo.amx)
animatix gif examples/swap_demo.amx -o out.gif

# Export with a 2-second trailing hold
animatix gif examples/swap_demo.amx -o out.gif --hold 2.0

# Export explicit 3-second slice (hold is ignored when --duration is set)
animatix gif examples/swap_demo.amx -o out.gif --duration 3.0
```

**Image export (`animatix image`):** Renders a single frame at `--time` (default 0s). No trailing hold applies.
