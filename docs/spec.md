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
| Components | custom component actions | Yes | Runtime-real | Yes | Yes | `action` blocks inside components; inlined at expansion time |
| Modules | `pub let` exports | Yes | Runtime-real | Yes | Yes | Exported values from `.amx` files; see `examples/module_reuse_demo.amx`. |
| Modules | `import ... as` namespaced imports | Yes | Runtime-real | Yes | Yes | Aliased imports create namespaces for qualified access (`theme.accent`). |
| Modules | Re-exports (`pub let x = c.x`) | Yes | Runtime-real | Yes | Yes | Re-export chains resolved transitively through namespace imports. |
| Expressions | literals / arithmetic / calls / paths / conditionals | Yes | Runtime-real | Yes | Yes | Stable expression core |
| Expressions | closures | Yes | Runtime-real | Yes | Yes | Used by plotting/reactive examples |
| Expressions | `Expr::Method` | Yes | Runtime-real | Yes | Yes | Method dispatch: `string.length()`, `list.get(0)`, `num.abs()` |
| Expressions | `Expr::Index` | Yes | Runtime-real | Yes | Yes | Array/vector/string index: `items[0]`, `pos[1]`, `text[0]` |
| Expressions | `Expr::Construct` | Yes | Runtime-real | Yes | Yes | Object construction: `Point { x: 10, y: 20 }` |
| Primitives | All shapes (`Text`, `Math`, `Svg`, `Image`, `Rect`, `Ellipse`, `Line`, `Polygon`, `Path`, etc.) | Yes | Runtime-real | Yes | Yes | See `showcase.amx`, `arc_polygon_path_demo.amx`, `primitive_breadth_demo.amx`, `arrow_demo.amx`, `image_demo.amx` |
| Primitives | `Code` | Yes | Runtime-real | Yes | Yes | See `examples/code_demo.amx` |
| Plotting | `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet` | Yes | Runtime-real | Yes | Yes | `PlotCurve` with `kind: cartesian|polar|parametric|implicit`. See `examples/plotting.amx` |
| Morphing | re-declaration morphing + path/text interpolation | Yes | Runtime-real | Yes | Yes | Core morph path via re-declaration |
| Morphing | `strategy:auto\|match\|fade`, `path_arc`, `stretch` | Yes (scoped) | Runtime-real on timed path-morphing | Yes | Yes | |
| Actions | Entrance: `fade-in`, `draw-in`, `wipe-in`, `reveal-in`; Motion: `move`, `shift`, `rotate`, `scale`; Exit: `fade-out`, `wipe-out`, `reveal-out`, `draw-out`; Effects: `shake`, `pulse`, `bounce`; Reorder: `swap`, `reorder` | Yes | Runtime-real | Yes | Yes | Built-ins |
| Actions | broader verb-first surface | Yes | Partial | Partial | Yes | Shape exists; small subset implemented |
| Composition | `sequence { ... }` | Yes | Runtime-real | Yes | Yes | Sequential composition; nested sequences and staggers supported |
| Composition | `stagger [150ms] { ... }` | Yes | Runtime-real | Yes | Yes | Shared interval offset; nested sequences and staggers supported |
| Colorscheme | `Colorscheme "name" { ... }` | Yes | Runtime-real | Yes | Yes | Native AMX primitive with `extends` |
| Components | `@slot` markers with named slot fills | Yes | Runtime-real | Yes | Yes | Component-internal containers with `@slot`; instantiation via `@slotname { items }` |
| Multi-Scene | `# SceneName` scene declarations | Yes | Runtime-real | Yes | Yes | Top-level scene markers; `group_scenes()` post-processing |
| Multi-Scene | `play SceneName [transition, duration]` | Yes | Runtime-real | Yes | Yes | Scene-level play statements with transition types |
| Multi-Scene | `Composition::build()` / `BuildTarget` | — | Runtime-real | Yes | Yes | Per-scene timeline building, edge resolution, global time mapping |
| Multi-Scene | CLI export (video/GIF/image) | — | Runtime-real | Yes | Yes | `render_*_composition` functions; auto-routing via `BuildTarget` |
| Multi-Scene | GUI scene list / composition timeline | — | Pending | No | Planned | Phase 4–6 of implementation plan |
| Multi-Scene | Transition blending (dual render) | — | Pending | No | Planned | Phase 7; hard cuts only in Phase 1

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
| `#` | Keyframe (absolute `#0s` or relative `#+1s`) or scene declaration (`# SceneName`) | `#2.5s`, `#+1s`, `# Intro` |
| `play` | Scene transition statement | `play Diagram [fade, 300ms]` |
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

**Primitive-type defaults:** Text-like (`Text`, `Math`, `Code`) → `text.primary`; shape fills → `surface.primary`; shape strokes (`Line`) → `stroke.default`; plot curves → `accent.primary`.

```animatix
config { colorscheme: "editorial-dark" }
title: Text, text: "Hello"           // color: text.primary
panel: Rect, size: (200, 100)       // color: surface.primary
badge: Ellipse, size: (40, 40), color: auto
```

`Colorscheme` primitive with `extends` inheritance is supported. See [`architecture.md`](architecture.md) §Colorscheme System.

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
- **Entrance:** `fade-in`, `draw-in`, `wipe-in`, `reveal-in`
- **Exit:** `fade-out`, `wipe-out`, `reveal-out`, `draw-out`
- **Effects:** `shake`, `pulse`, `bounce`
- **Reorder:** `swap`, `reorder`

**Rotation:** Two ways to rotate:
- `rotate item [by: angle, duration]` - Visual transform (applies to actor transform matrix)
- `item.rotation = value [duration]` - Property-based rotation

Vector reveal actions (`draw-in`, `reveal-in`, `wipe-in`, `wipe-out`, `reveal-out`, `draw-out`) are **leaf-only**; containers/groups report diagnostics.

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

**GUI Reorder (Implemented)**

The GUI supports canvas drag-to-reorder for layout-managed children. Dragging a child inside a Row/Col/Grid container projects the mouse onto the main axis and computes an insertion index. Visual feedback includes a ghost at the original position and an accent-blue drop line. On release, the new order is persisted to source via AST mutation of the container's `children` block.

The inspector also exposes up/down arrow buttons for each child when a container is selected.

**`reorder`**

`reorder container [order: (childA, childB, childC), duration]` — Reorders all children of a container to a specified order. The `order` modifier is required and must be a tuple containing exactly the same labels as the container's current children (no additions or omissions). Requires `dynamic_layout: true`.

```animatix
config { dynamic_layout: true }

row: Row, gap: 8 {
  a: Rect, size: (30, 40)
  b: Rect, size: (30, 80)
  c: Rect, size: (30, 60)
}

# Reverse the row
#2s
reorder row [order: (c, b, a), 500ms, ease: ease-out]

# Back to original order
#3s
reorder row [order: (a, b, c), 500ms, ease: ease-out]
```

Overlapping reorders on the same container are disallowed and emit a diagnostic, same as `swap`.

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
circle: Ellipse, at: (0, 0)

#2s
circle: Ellipse, at: (100, 100) [2s]
```

**Shipped morph modifiers** (timed path-morphing only):
```animatix
[2s, strategy: auto]     // Engine decides (default)
[2s, strategy: match]   // Force point alignment
[2s, path_arc: 1.57]    // Curved interpolation hint
[2s, stretch: true]     // Bounds-normalized interpolation
```
`strategy: fade` cross-fades between overlapping states by rendering both source and target path sets at partial opacity.

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

> **See [`architecture.md`](architecture.md) §Layout System for full details.**

Implemented: `Row`, `Col`, `Grid`, `Stack`, `Group`.

- **Row/Col:** `gap` (spacing), `padding` (inset), `align` ("start" | "center" | "end")
- **Grid:** `cols` + `gap` + `padding`
- **Stack:** Overlapping children around shared origin (supports `padding`)
- **Group:** Non-layout grouping/transform inheritance

**Declaration-time measure/place contract:** Layout containers consume each child's `size` track at timeline build; children with explicit `at` opt into manual placement instead.

```animatix
row: Row, gap: 12, padding: 20, align: "center" {
  Rect, color: red
  Ellipse, color: blue
}
```

**Scene anchors:** `scene.top_left`, `scene.top`, `scene.center`, etc.  
**Percentage placement:** `at: (82%, 76%)`

---

## 9. Primitive Types

**Shapes:** `Rect`, `Ellipse`, `Line`, `Polygon`, `Path`

**Text-like:** `Text`, `Math`, `Code`, `Svg`, `Image`

**Path commands:** `move_to(...)`, `line_to(...)`, `quad_to(...)`, `curve_to(...)`, `close()`

```animatix
circle: Ellipse, size: (100, 100)
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

**Re-exports:** A module can re-export values from its own imports:

```animatix
// colors.amx
pub let primary = (0.38, 0.78, 1.0, 1.0)

// theme.amx
import "./colors.amx" as c
pub let accent = c.primary

// scene.amx
import "./theme.amx" as theme
panel: Rect, color: theme.accent
```

Re-export chains are resolved transitively. Values are evaluated at build time in the importing scene's environment.

**Current limitations:**
- Namespace access is one level: `alias.export_name`
- Property assignment for `text`/`latex`/`math`/`code` stores the value but does not trigger re-compilation of text paths at render time; infrastructure is in place but render-time font compilation is deferred.

---

## 12. Components

**Definition:**
```animatix
pub component MetricCard(title: "Metric") {
    frame: Rect, size: (240, 120), color: blue
    title_text: Text { text: title, at: (0, -20) }
    badge: Ellipse, size: (24, 24), color: gold
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

### Custom Component Actions

Define reusable action sequences inside a component:

```animatix
pub component Button(text: "OK") {
    action pulse {
        self.scale = 1.2 [100ms]
        self.scale = 1.0 [100ms]
    }
    frame: Rect, size: (100, 40)
}
```

Invoke on any instance:
```animatix
btn: Button, text: "Click"

#0s
pulse btn [200ms]
```

**Semantics:**
- Custom actions are **inlined at component expansion time**. The invocation is replaced with the action's body statements.
- Invocation modifiers **override** body modifiers. `pulse btn [200ms]` replaces any `[100ms]` in the body with `[200ms]`.
- Use `self` to refer to the component instance. `self.scale` rewrites to `btn.scale`.
- Actions work inside `sequence`, `stagger`, and keyframes with correct timing.

**Limitations:**
- No action parameters yet (MVP only supports fixed bodies)
- Multi-target invocation (`pulse btn, icon`) is not supported
- Actions cannot be defined at module scope (only inside components)

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

**`Graph`**: Container mapping logical domains to physical bounds. Supports axes, optional grid lines, and ticks.
```animatix
graph: Graph, x_domain: (-5, 5), y_domain: (-10, 30), size: (400, 400)
```

**`PlotCurve`**: Single-stroke curve plot. The `kind` property selects the sampling strategy:

| `kind` | Closure signature | Example |
|--------|-------------------|---------|
| `"cartesian"` | `(x) => y` | `func: (x) => x^2 + 3` |
| `"polar"` | `(theta) => r` | `func: (t) => 1 + sin(3*t)` |
| `"parametric"` | `(t) => (x, y)` | `func: (t) => (cos(t), sin(t))` |
| `"implicit"` | `(x, y) => scalar` | `func: (x, y) => x^2 + y^2 - 1` |

```animatix
parabola: PlotCurve, kind: "cartesian", func: (x) => x^2 + 3, color: red
spiral: PlotCurve, kind: "polar", func: (t) => t, color: blue
```



**`VectorField`**: Grid-sampled arrows from `(x, y) => (dx, dy)`.
```animatix
field: VectorField, func: (x, y) => (y, -x), density: 12, color: accent.primary
```

**`Heatmap`**: Scalar field visualization with color-mapped rectangles.
```animatix
heat: Heatmap, func: (x, y) => sin(x) * cos(y), resolution: 32, color: red
```

**`ContourSet`**: Multiple level-set curves for a scalar function.
```animatix
contours: ContourSet, func: (x, y) => x^2 + y^2, levels: (1, 4, 9), resolution: 96, color: blue
```

**Closures:**
```animatix
(x) => x^2           // single param
(x, y) => x + y      // multiple params
```

**Built-in math:** `sin(x)`, `cos(x)`, `lerp(a, b, t)`, `rand()`, `format("template {}", value, ...)`

---

## 14. Expressions & Access

### Dotted Paths

**Assignment targets** resolve to runtime actors; final segment = property name.
```animatix
left.badge.color = red
right.frame.size = (40, 40)
```

**Seeded property paths:** `node.at`, `node.size`, `node.color`, `scene.background_color`, `node.at.x`, `node.at.y`, `node.color.r/g/b/a`.

**Unresolved rhs paths** report build diagnostics; host property keeps its default/fallback value.

### Index Access

Values can be indexed by position:

```animatix
let items = (10, 20, 30)
let first = items[0]    // 10

let text = "hello"
let ch = text[1]        // "e"

let pos = (100, 200)
let x = pos[0]          // 100
```

Supported: `List`, `Str` (char), `Vec2/3/4`, `Color`.

### Method Calls

Methods dispatch on the receiver's type:

```animatix
let text = "hello,world"
let parts = text.split(",")     // List["hello", "world"]
let n = text.length()           // 13

let items = (1, 2, 3)
let len = items.length()        // 3
let second = items.get(1)       // 2

let x = -42.5
let y = x.abs()                 // 42.5
```

**String methods:** `length()`, `split(delim)`, `contains(substr)`, `trim()`, `starts_with(prefix)`, `ends_with(suffix)`
**List methods:** `length()`, `get(index)`, `contains(item)`
**Number methods:** `abs()`, `floor()`, `ceil()`, `round()`

### Object Construction

Named structs can be constructed with field syntax:

```animatix
let p = Point { x: 10, y: 20 }
```

Returns a `Value::Object` with typed fields. Field access is not yet implemented; objects are primarily used for reactive blocks and function returns.

---

## 15. Known Gaps & Limitations

- **Object Field Access:** `Value::Object` supports construction but field read (`p.x`) and write are not yet implemented.
- **Re-declaration for Morphing/Media:** Morphing text or updating SVG/Image sources currently requires re-declaring the entire object at a new keyframe, breaking standard property assignment syntax.
- ~~**Static Geometry:** Structural geometry inputs like `Polygon.points` and `Path.commands` are declaration-time only and cannot be animated dynamically frame-by-frame.~~ Both now support timed assignments with path morphing.

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

**Parallel rendering:** Video and GIF exports render frames in parallel using all available CPU cores. Each thread gets its own GPU context and a cloned Timeline, then renders a chunk of frames. Encoding (GIF quantization / video muxing) remains sequential to preserve frame order and codec state.

```bash
# 109-frame GIF rendered across 28 threads (~4 frames each)
animatix gif examples/swap_demo.amx -o out.gif --fps 10
```

**Image export (`animatix image`):** Renders a single frame at `--time` (default 0s). No trailing hold or parallelization applies.

### Multi-Scene Composition

Multi-scene compositions support the same export flags. Duration is auto-detected from the composition's global timeline (`Composition::global_duration_s`) rather than a single timeline.

```bash
# Export a multi-scene composition
animatix video examples/multi_scene_demo.amx --width 1280 --height 720

# GIF export with quick preview settings
animatix gif examples/multi_scene_mini.amx --width 640 --height 360 --fps 10
```

---

## 17. Multi-Scene Composition

> **Status:** Phases 1–3 shipped (parser, composition engine, CLI export). Phases 4–8 pending (GUI, transitions, cross-file scenes).  
> **Design doc:** [`docs/multi-scene-composition-design.md`](multi-scene-composition-design.md)

### Scene Declarations

Scenes are declared using `# SceneName` at the top level:

```animatix
# Intro
#0s
title: Text, text: "Welcome"
#1s
fade-in title [500ms]
```

### Transitions

The `play` statement defines the successor scene and transition:

```animatix
# Intro
#0s
title: Text, text: "Welcome"
#1s
fade-in title [500ms]

play Diagram [fade, 300ms]

# Diagram
#0s
graph: Rect, size: (400, 400)
```

**Supported transitions (hard cuts in Phase 1):** `cut`, `fade`, `wipe-left`, `wipe-right`, `wipe-up`, `wipe-down`. Transition blending (dual offscreen render) is deferred to Phase 7.

### Per-Scene Configuration

A scene may contain its own `config` block after the scene declaration:

```animatix
# Intro
config { colorscheme: "editorial-dark" }
#0s
title: Text, text: "Welcome"
```

### Shared Prelude

Top-level statements before the first scene (imports, `pub let`, file-level `config`) are shared across all scenes:

```animatix
import "./theme.amx" as theme
pub let accent = theme.accent

config { resolution: (1280, 720) }

# Intro
#0s
title: Text, text: "Welcome", color: accent

# Diagram
#0s
graph: Rect, size: (400, 400)
```

### Backward Compatibility

Files without `# SceneName` declarations are single-scene files — all existing syntax, semantics, and behavior are preserved exactly. The parser produces the same AST as before; the timeline builder follows the existing single-timeline path.

### CLI Export

Multi-scene compositions are automatically detected and routed via `BuildTarget`. All export commands (`video`, `gif`, `image`) work identically for both single-scene and multi-scene files.

### Current Limitations

- **Hard cuts only** — transition blending (Phase 7) is not yet implemented; scene changes are instantaneous cuts.
- **Live preview** (`animatix render`) shows only the first scene for multi-scene files.
- **GUI** does not yet show the scene list panel or composition timeline (Phases 4–6).
- **Tree-sitter grammar** has not been updated for `# SceneName` or `play` syntax.

---

## Appendix A: Source Formatting Specification

> Scope: serializer output (`animatix::to_source`) and GUI write-back.
> Goal: deterministic, readable `.amx` source that matches hand-authored style.

### A.1 General Principles

1. **Consistency** — Re-serializing already-formatted code produces byte-identical output.
2. **Readability** — Vertical space separates logical units; horizontal space is conserved.
3. **Determinism** — Formatting is entirely structural; no width heuristics or line-length limits.

### A.2 Indentation

| Item | Value |
|---|---|
| Indent unit | 2 spaces (U+0020) |
| Tab characters | Never emitted |
| Increase | One level per nested block or child list |
| Decrease | One level when closing a block or child list |

### A.3 Top-Level Layout

- Each **top-level** statement is separated by **one blank line** (`\n\n`).
- Inside a block (e.g. `sequence { … }`) statements are separated by a **single newline** (`\n`) only.
- Keyframe blocks (`#2s`, `#+500ms`) are top-level statements, so they are separated by blank lines from neighbours.

### A.4 Actor Declarations

**Without children (flat):**
```amx
label: Type, prop1: value1, prop2: value2 [mod1, mod2]
```
- Label and type are separated by `: ` (colon + space).
- Properties are comma-separated on the **same line**.
- Modifiers follow properties in `[…]` brackets.
- No trailing comma after the last property.

**With children (container):**
```amx
label: Type, prop1: value1 {
  child1: Type, prop1: value1
  child2: Type, prop2: value2
}
```
- All properties and modifiers stay on the **header line**.
- Opening `{` is preceded by a single space and follows the last modifier.
- Each child gets its **own line**, indented +1 level.
- Closing `}` gets its **own line** at the parent's indentation level.
- No trailing comma after the last child.

### A.5 Block Statements

```amx
sequence {
  fade-in a [400ms]
  fade-in b [400ms]
}

stagger [100ms] {
  fade-in label [400ms]
  fade-in a [400ms]
}

if condition {
  then_stmt1
} else {
  else_stmt1
}
```

- Header stays on one line.
- Body statements each get their own line, indented +1 level.
- Closing brace on its own line at the parent's indentation level.
- `else` is separated from the closing `}` of `if` by a single space (no newline).

### A.6 Keyframe and Scene Blocks

```amx
#2s
stmt1
stmt2

# SceneName
#0s
stmt1
stmt2

play NextScene [fade, 300ms]
```

- Time marker / scene name on its own line.
- Body statements each on their own line, **not indented**.
- A blank line separates consecutive keyframe / scene blocks at the top level.
- `play` appears at the scene body level on its own line.

### A.7 Comments and Expressions

- **Trailing comments** (`// comment`) are preserved with exactly **2 spaces** before `//`.
- Expressions are always emitted **inline**; they never contain newlines.
- `config` keeps its settings inline.

### A.8 Write-Back Pipeline

The GUI inspector mutates the AST via `source_edit::apply_edit`, then the entire file is re-serialized. Formatting is applied at the **serialization layer only** (`animatix::to_source`). No formatting state is carried through the edit; the serializer is the single source of truth for layout.
