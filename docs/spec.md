# Animatix Language Specification

> This document describes the implemented language surface. Status callouts distinguish shipped runtime behavior from parser-only or planned syntax. Runnable examples are in `examples/README.md`.

---

## LLM Generation Checklist

Use these rules when generating `.amx` files:

- Start with `config { colorscheme: "editorial-dark", resolution: (1280, 720) }` unless the user asks otherwise.
- Declare actors as `label: Type, prop: value`; animate later with keyframes (`#1s`) and assignments (`label.prop = value [800ms, ease: ease-out]`).
- Use supported primitives only: `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Typst`, `Code`, `Svg`, `Image`, `Audio`, `Equation`, `Fragment`, `Graph`, `PlotCurve`, `BarChart`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Filter`, `Mask`, `Legend`.
- Avoid common hallucinations: `Circle` (use `Ellipse`), `Triangle` (use `Polygon`), `Chart`/`Diagram` (use `Graph`/`PlotCurve`), and any 3D primitives.
- Colors are RGBA tuples `(r, g, b, a)`, scheme tokens (`accent.primary`, `text.primary`, etc.), `auto`, or named colors (`RED`/`red`, `GREEN`/`green`, `BLUE`/`blue`, `BLACK`/`black`, `WHITE`/`white`, `YELLOW`/`yellow`, `ORANGE`/`orange`). Do not use hex strings.
- Timing modifiers use positional duration: `[1s]`, `[800ms, ease: ease-in-out]`, `[delay: 250ms, 0s]`. Do not write `duration: 1s`.
- `sequence`/`stagger` may contain actions, assignments, `let`, and nested `sequence`/`stagger`; actor declarations inside them are rejected.
- Asset paths in examples should point to files that exist under `examples/assets/`.

---

## Language Status Matrix

| Area | Surface | Parser | Runtime | Tests | Docs | Notes |
|---|---|---|---|---|---|---|
| Reactive | `always` | Yes | Runtime-real | Yes | Yes | Shipped stateless reactive model |
| Reactive | `for` | Yes | Runtime-real | Yes | Yes | Compile-time structural expansion |
| Reactive | `loop` / `yield` / `stop` / `pause` / `resume` | Rejected | Removed | Yes | Yes | Explicitly removed |
| Components | `pub component` instantiation | Yes | Runtime-real | Yes | Yes | Via `module.rs`; see `examples/components/09_components.amx` |
| Components | parameter binding + nested-label isolation | Yes | Runtime-real | Yes | Yes | Module/timeline tests |
| Components | dotted assignment targets / rhs property lookup | Yes | Runtime-real | Yes | Yes | Nested-label writes; nonexistent targets report diagnostics |
| Components | custom component actions | Yes | Runtime-real | Yes | Yes | `action` blocks inside components; inlined at expansion time |
| Modules | `pub let` exports | Yes | Runtime-real | Yes | Yes | Exported values from `.amx` files; see `examples/components/10_modules.amx`. |
| Modules | `import ... as` namespaced imports | Yes | Runtime-real | Yes | Yes | Aliased imports create namespaces for qualified access (`theme.accent`). |
| Modules | Re-exports (`pub let x = c.x`) | Yes | Runtime-real | Yes | Yes | Re-export chains resolved transitively through namespace imports. |
| Expressions | literals / arithmetic / calls / paths / conditionals | Yes | Runtime-real | Yes | Yes | Stable expression core |
| Expressions | closures | Yes | Runtime-real | Yes | Yes | Used by plotting/reactive examples |
| Expressions | `Expr::Method` | Yes | Runtime-real | Yes | Yes | Method dispatch: `string.length()`, `list.get(0)`, `num.abs()` |
| Expressions | `Expr::Index` | Yes | Runtime-real | Yes | Yes | Array/vector/string index: `items[0]`, `pos[1]`, `text[0]` |
| Expressions | `Expr::Construct` | Yes | Runtime-real | Yes | Yes | Object construction: `Point { x: 10, y: 20 }` |
| Primitives | All shapes (`Text`, `Typst`, `Svg`, `Image`, `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Mask`, etc.) | Yes | Runtime-real | Yes | Yes | See `examples/basics/01_shapes.amx`, `examples/layout/13_paths.amx`, `examples/composition/20_feature_reel.amx` |
| 3D | `Graph3D`, `Line3D`, `Polyhedron` | — | **Not supported** | — | Yes | Explicitly not planned; all rendering is 2D |
| Primitives | `Code` | Yes | Runtime-real | Yes | Yes | See `examples/basics/01_shapes.amx` |
| Plotting | `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane` | Yes | Runtime-real | Yes | Yes | `PlotCurve` with `kind: cartesian|polar|parametric|implicit`. See `examples/data/07_plots.amx`, `examples/data/18_number_plane_contours.amx` |
| Post-processing | `Filter` (blur, brightness, contrast, saturate, hue-rotate, sepia) | Yes | Runtime-real | Yes | Yes | Container primitive; renders children offscreen then applies GPU filtering with a CPU fallback when no GPU backend is available. See `examples/animation/08_effects.amx` |
| Morphing | re-declaration morphing + path/text interpolation | Yes | Runtime-real | Yes | Yes | Core morph path via re-declaration |
| Morphing | `strategy:auto\|match\|fade`, `path_arc`, `stretch` | Yes (scoped) | Runtime-real on timed path-morphing | Yes | Yes | |
| Actions | Entrance: `fade-in`, `draw-in`, `wipe-in`, `reveal-in`; Motion: `move`, `shift`, `rotate`, `scale`; Exit: `fade-out`, `wipe-out`, `reveal-out`, `draw-out`; Effects: `shake`, `pulse`, `bounce`; Reorder: `swap`, `reorder` | Yes | Runtime-real | Yes | Yes | Built-ins |
| Actions | broader verb-first surface | Yes | Partial | Partial | Yes | Shape exists; small subset implemented |
| Composition | `sequence { ... }` | Yes | Runtime-real | Yes | Yes | Sequential composition; nested sequences and staggers supported |
| Composition | `stagger [150ms] { ... }` | Yes | Runtime-real | Yes | Yes | Shared interval offset; nested sequences and staggers supported |
| Colorscheme | `let name = Colorscheme { extends: "...", auto: (...) }` | Yes | Runtime-real | Yes | Yes | Source parser accepts construct keys including dotted token overrides |
| Components | `@slot` markers with named slot fills | Yes | Runtime-real | Yes | Yes | Component-internal containers with `@slot`; instantiation via `@slotname { items }` |
| Multi-Scene | `# SceneName` scene declarations | Yes | Runtime-real | Yes | Yes | Top-level scene markers; `group_scenes()` post-processing |
| Multi-Scene | `play SceneName [transition, duration]` | Yes | Runtime-real | Yes | Yes | Scene-level play statements with transition types |
| Multi-Scene | `Composition::build()` / `BuildTarget` | — | Runtime-real | Yes | Yes | Per-scene timeline building, edge resolution, global time mapping |
| Multi-Scene | CLI export (video/GIF/image) | — | Runtime-real | Yes | Yes | `render_*_composition` functions; auto-routing via `BuildTarget` |
| Multi-Scene | GUI scene list / composition timeline | Yes | Runtime-real | Yes | Yes | Sidebar scene tab, composition scene track, and scene source edits shipped |
| Multi-Scene | Transition blending (dual render) | Yes | Runtime-real | Yes | Yes | Phase 7; `TransitionCompositor` + WGSL shader; wired in CLI preview, GUI preview, and export |
| Multi-Scene | Cross-file scene composition | Yes | Runtime-real | Yes | Yes | `import "file.amx" as alias` + `play alias.SceneName`; namespace scene registry |

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

**Pre-Keyframe Actor Declarations (Hidden by Default):**
Actors declared **before the first keyframe** are hidden by default (`opacity: 0`). They remain invisible until an entrance action (e.g. `fade-in`) or an explicit `opacity` assignment makes them visible.

```animatix
// These actors are hidden until fade-in runs
hello: Text, text: "Hello", font_size: 72, color: text.primary, anchor: scene.center
backdrop: Rect, size: fill, color: scene.background, anchor: scene.center

#0.5s
fade-in hello [800ms, ease: ease-out]

#1s
fade-in backdrop [600ms]
```

Actors declared **inside a keyframe** (including `#0s`) are visible by default (`opacity: 1`). To hide an in-keyframe actor, set `opacity: 0` explicitly.

```animatix
#0s
actor: Rect, size: (100, 100), opacity: 0   // explicitly hidden

#1s
fade-in actor [500ms]
```

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

**Primitive-type defaults:** Text-like (`Text`, `Typst`, `Code`) → `text.primary`; shape fills → `surface.primary`; shape strokes (`Line`) → `stroke.default`; plot curves → `accent.primary`.

```animatix
config { colorscheme: "editorial-dark" }
title: Text, text: "Hello"           // color: text.primary
panel: Rect, size: (200, 100)       // color: surface.primary
badge: Ellipse, size: (40, 40), color: auto
```

**RGBA format:** `(r, g, b, a)` where each component is a **0–1 float**.
```animatix
// Correct
red: Rect, color: (1.0, 0.0, 0.0, 1.0)
// NOT supported: hex strings like #ff0000
```

**Colorscheme tokens** (available when a colorscheme is active):
- `scene.background`
- `accent.primary`, `accent.secondary`, `accent.info`, `accent.success`, `accent.warning`, `accent.danger`
- `text.primary`, `text.secondary`, `text.muted`
- `stroke.default`
- `surface.primary`, `surface.secondary`

**Named colors** (case-insensitive): `RED`/`red`, `GREEN`/`green`, `BLUE`/`blue`, `BLACK`/`black`, `WHITE`/`white`, `YELLOW`/`yellow`, `ORANGE`/`orange`.

`Colorscheme` construct syntax in source:
```animatix
let forest = Colorscheme {
  extends: "editorial-dark",
  auto: ((0.35, 0.82, 0.55, 1.0), (0.98, 0.83, 0.44, 1.0))
}
config { colorscheme: "forest" }
```

> **Parser note:** keys inside `Colorscheme { ... }` accept dotted token overrides like `scene.background`, matching the runtime/API behavior.

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
| `Text`/`Typst`/`Code` declarations | positional duration + `delay` + `ease` |
| Morph keys (`strategy`, `path_arc`, `stretch`) | timed path-morphing re-declarations only |

Duplicate modifier keys: last value wins. `ease` without duration = instant change.

**Built-in Actions:**
- **Motion:** `move`, `shift`, `rotate`, `scale`
- **Entrance:** `fade-in`, `draw-in`, `wipe-in`, `reveal-in`
- **Exit:** `fade-out`, `wipe-out`, `reveal-out`, `draw-out`
- **Effects:** `shake`, `pulse`, `bounce`
- **Highlight:** `highlight`, `unhighlight` — animate highlight overlay on Equation fragments or Typst actors (whole-actor highlighting)
- **Reorder:** `swap`, `reorder`

**Action signatures for generation:**

| Action | Shape |
|---|---|
| `fade-in`, `fade-out`, `draw-in`, `draw-out`, `wipe-in`, `wipe-out`, `reveal-in`, `reveal-out` | `verb target [duration, delay, ease]` |
| `move` | `move target [to: Vec2, duration, ease]` |
| `shift` | `shift target [by: Vec2, duration, ease]` |
| `rotate` | `rotate target [by: Num, duration, ease]` |
| `scale` | `scale target [by: Num, duration, ease]` |
| `shake`, `pulse`, `bounce` | `verb target [duration, intensity: Num]` |
| `highlight` | `highlight target [color: Color, blend: Str, padding: Num, radius: Num, duration, ease]` |
| `unhighlight` | `unhighlight target [duration, ease]` |
| `swap` | `swap childA, childB [duration, ease]` |
| `reorder` | `reorder container [order: {childA, childB, ...}, duration, ease]` |

**Rotation:** Two ways to rotate:
- `rotate item [by: angle, duration]` - Visual transform (applies to actor transform matrix)
- `item.rotation = value [duration]` - Property-based rotation

> Rotation values are in radians by default. Use the `deg()` helper to convert: `deg(90)` equals ≈1.5708 radians.

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

`reorder container [order: {childA, childB, childC}, duration]` — Reorders all children of a container to a specified order. The `order` modifier is required and must be a list containing exactly the same labels as the container's current children (no additions or omissions). Requires `dynamic_layout: true`.

```animatix
config { dynamic_layout: true }

row: Row, gap: 8 {
  a: Rect, size: (30, 40)
  b: Rect, size: (30, 80)
  c: Rect, size: (30, 60)
}

# Reverse the row
#2s
reorder row [order: {c, b, a}, 500ms, ease: ease-out]

# Back to original order
#3s
reorder row [order: {a, b, c}, 500ms, ease: ease-out]
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

Nested `sequence`/`stagger` blocks and `let` declarations are allowed. Actor declarations inside `sequence`/`stagger` are rejected; declare actors outside the composition block, then animate them inside it.

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

Implemented: `Row`, `Col`, `Grid`, `Stack`, `Group`, `Filter`, `Mask`.

- **Row/Col:** `gap` (spacing, single value or per-axis tuple `(row_gap, col_gap)`), `padding` (inset, single value or per-side tuple `(top, right, bottom, left)`), `align` ("start" | "center" | "end"), `vertical_align` ("baseline" — aligns children by their text baseline instead of the container's cross-axis center)
- **Grid:** `cols` + `gap` + `padding`
- **Stack:** Overlapping children around shared origin; supports `padding`, `align` ("start" | "center" | "end")
- **Group:** Non-layout grouping/transform inheritance

**Declaration-time measure/place contract:** Layout containers consume each child's `size` track at timeline build; children with explicit `at` opt into manual placement instead.

**Warning:** Setting `at` or `position` on a child of a layout container (Row/Col/Grid/Stack) triggers a build-time warning. In a managed layout, `at`/`position` is ignored — the container owns placement. Use `transform` for visual offsets without disrupting the parent's layout algorithm.

```animatix
row: Row, gap: 12, padding: 20, align: "center" {
  Rect, color: red
  Ellipse, color: blue
}

// Per-axis gap and per-side padding
col: Col, gap: (8, 16), padding: (10, 20, 10, 20) {
  a: Rect, size: (100, 50)
  b: Rect, size: (100, 50)
}

// Stack with alignment
stack: Stack, align: "center", padding: 24 {
  bg: Rect, size: (200, 100), color: surface.secondary
  label: Text, text: "Centered", color: text.primary
}

// Baseline alignment in a Row
badge_row: Row, vertical_align: "baseline" {
  icon: Text, text: "✦", font_size: 24
  label: Text, text: "Premium", font_size: 14
}
```

**Scene anchors:** `scene.top_left`, `scene.top`, `scene.center`, etc.  
**Percentage placement:** `at: (82%, 76%)`

### Phase 7: Percentage & Intrinsic Sizing

**Percentage child sizing:** Container children can be sized relative to the parent's content box using percentage strings:

```animatix
Row, gap: 0 {
    a: Rect, size: (50%, 40)    // 50% of parent width, 40px height
    b: Rect, size: fill          // fill remaining (100% width, auto height)
}
```

- Percentage values (e.g. `"50%"`) resolve against the parent container's **content box** (after padding/gap).
- `size: fill` is shorthand for `size: (100%, auto)` — fills the available width.
- `size: auto` or `size: fit` on a layout container makes it shrink-wrap to its children's content.
- `size: auto` on a non-container primitive produces a build-time warning (only containers shrink-wrap).
- `size: fill` at the top level (no parent container) produces a build-time warning.

**Intrinsic container sizing:** Containers with `size: auto` or `size: fit` compute their own size from children:

```animatix
// Row shrink-wraps to its children
row: Row, size: auto, gap: 10 {
    a: Rect, size: (100, 50)
    b: Rect, size: (200, 50)
}
// row.total_width ≈ 100 + 10 + 200 = 310px
```

- **Row:**  width = sum of child widths + gaps; height = max child height.
- **Col:**  width = max child width; height = sum of child heights + gaps.
- **Grid:** uses Taffy's `Auto` track sizing when no explicit template is given.
- `size: fit` behaves identically to `size: auto` (both rely on intrinsic sizing).

**Min/Max constraints:** Clamp the resolved size of any actor (child or container):

```animatix
a: Rect, size: (500, 50), min_width: 100, max_width: 300, min_height: 20
```

- `min_width`, `min_height` — enforce a minimum size.
- `max_height` — enforce a maximum size.
- All four are optional (omit for no constraint).
- Constraints apply after percentage resolution: `min ≤ resolved ≤ max`.
- These are animatable (ASSIGNABLE).

**Important notes:**
- Percentage sizing resolves against the parent's **content box** (size minus padding).
- If the parent container also uses intrinsic sizing, percentages resolve after the parent's size is computed from non-percentage children.
- Existing fixed-size layouts continue to work unchanged (backward compatible).

---

## 9. Primitive Types

**Shapes:** `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`

**Text-like:** `Text`, `Typst`, `Code`, `Svg`, `Image`

**Equation:** `Equation` (container), `Fragment` (child) — typeset equations with per-segment highlighting

**Path commands:** `move_to(...)`, `line_to(...)`, `quad_to(...)`, `curve_to(...)`, `close()`

```animatix
circle: Ellipse, size: (100, 100)
poly: Polygon, points: {(0,0), (100,0), (50,100)}
path: Path, commands: {move_to(0, 0), line_to(100, 100), close()}
arrow: Arrow, from: (0, 0), to: (120, 40), head_size: 18
img: Image, url: "examples/assets/checker.png", at: (100, 100), size: (200, 150)
```

**Common generation properties:**

| Actor kind | Useful properties |
|---|---|
| All actors | `at`, `position`, `anchor`, `offset`, `opacity`, `rotation`, `scale`, `transform` |

> **Layout-managed children** (`Row`/`Col`/`Grid`/`Stack`): `at` and `position` trigger a build-time warning. Use `transform` for visual offsets inside managed layouts — it works seamlessly without disrupting the container's layout algorithm.
| Sized actors | `size` |
| Drawables | `color` |
| Shapes | `stroke`, `stroke_width`, `fill_opacity`, `stroke_progress` |
| `Line` | `from`, `to` |
| `Arrow` | `from`, `to`, `head_size` |
| `Callout` | `from`, `to`, `head_size`, `label`, `label_at`; targeted mode: `target: actor`, `place: top/bottom/left/right/auto`, `standoff`, `to_offset` |

**Callout targeted mode** places the arrow tip automatically on a named actor's edge. When `target` is set:

- `place` accepts the typed `CalloutPlace` enum: `top`, `bottom`, `left`, `right`, or `auto` (default). `auto` picks the side closest to `label_at`.
- The engine resolves the target's world-space bounds by composing its full ancestor transform chain, so nested actors inside scaled or rotated containers attach correctly.
- `standoff` (default `40.0`) offsets the tip from the resolved edge; `to_offset` adds a fine-grained manual tweak on top.
- A missing or unknown `target` name is reported as a build-time `CalloutTargetNotFound` diagnostic rather than a per-frame render warning.
- Array actors use source-level syntax: `target: bars[2]` in declarations, and `note.target = bars[i]` when `i` is bound in an assignment. Internal generated labels like `bars__2` are an implementation detail and should not appear in user source.
- **GUI affordances:** selecting a targeted `Callout` in the preview shows four edge handles around the target for clicking/dragging `place`, a standoff drag handle on the tail axis, and a label drag handle. Hold **Shift** while dragging to detach from the target — the current `from`/`to` values are baked into the source and `target` is removed, converting to manual mode without a visual jump.
| `Legend` | `at` |
| `Polygon` | `points: {(x, y), ...}` |
| `Path` | `commands: {move_to(...), line_to(...), curve_to(...), close()}` |
| `Text` / `Typst` / `Code` | `text` / `content` / `code`, `font_size`, `font_family`, `font_weight`, `font_style`, `line_height`, `letter_spacing`, `word_spacing`, `text_max_width`, `text_align`, `overflow` |
| `Image` / `Svg` | `url` |
| `Filter` | `blur`, `brightness`, `contrast`, `saturate`, `hue_rotate`, `sepia` |
| `Graph` / plots | `x_domain`, `y_domain`, `func`, `kind`, `resolution`, `density`, `levels` |
| `Row` / `Col` / `Grid` / `Stack` | `gap` / `gap: (row, col)`, `padding` / `padding: (top, right, bottom, left)`, `align`, `vertical_align` (Row/Col), `cols` (Grid) |

**Text shorthand:**
```animatix
title: "Hello"                    // desugars to: title: Text, text: "Hello"
title: "Hello" [2s, ease: bounce] // with modifiers
```

**Typst shorthand:**
```animatix
eq: $$ x^2 + y^2 $$                    // desugars to: eq: Typst, content: "x^2 + y^2"
eq: $$ x^2 $$ [2s, ease: bounce]       // with modifiers
```

A bare `$$ ... $$` block produces a `Typst` actor. The content between `$$` delimiters is taken as raw Text (unquoted) and becomes the `content` property. A label is required. Modifiers are supported.

### Typography Properties

Text-like primitives (`Text`, `Typst`, `Code`) support typography properties for fine-grained control over text appearance:

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `font_weight` | Num or Str | `400` | Font weight. Numeric values 100–900, or strings `"normal"` (400) / `"bold"` (700). |
| `font_style` | Str | `"normal"` | Font style. `"normal"` or `"italic"`. |
| `line_height` | Num | `1.2` | Line height multiplier relative to font size. `1.0` = single-spaced, `1.5` = 1.5× spacing. |
| `letter_spacing` | Num | `0.0` | Additional spacing between letters in pixels. Positive values increase spacing; negative values tighten. |
| `word_spacing` | Num | `0.0` | Additional spacing between words in pixels. |

```animatix
label: Text, text: "Typography", font_size: 48,
  font_weight: "bold", font_style: "italic",
  letter_spacing: 2.0, line_height: 1.4
```

`font_weight` accepts numeric values or keyword strings:
- `100`–`900`: standard CSS-like weight scale
- `"normal"`: equivalent to `400`
- `"bold"`: equivalent to `700`

All typography properties are animatable via keyframes:

```animatix
#0s
label: Text, text: "Hello", font_weight: "normal"

#2s
label.font_weight = "bold" [1s]
label.letter_spacing = 4.0 [1s]
```

### Text Wrapping & Overflow

Text primitives support wrapping and overflow control for multi-line layout:

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `text_max_width` | Num | `0` | Maximum width in pixels for text wrapping. `0` = no wrapping (single line). |
| `text_align` | Str | `"left"` | Horizontal alignment within the text box. `"left"`, `"center"`, `"right"`, or `"justify"`. |
| `overflow` | Str | `"visible"` | Text overflow behavior. `"visible"` (render beyond bounds), `"clip"` (hide overflow), or `"ellipsis"` (show `…` truncation). |

```animatix
// Wrapped text with center alignment
desc: Text, text: "Long paragraph that wraps at 300px",
  text_max_width: 300, text_align: "center", line_height: 1.5

// Single-line text with ellipsis overflow
trunc: Text, text: "Very long title that gets truncated",
  text_max_width: 200, overflow: "ellipsis"
```

**Automatic container-to-child width propagation:** When a `Text`/`Typst`/`Code` actor is placed as a direct child of a `Col` or `Grid` container (i.e., a container that constrains its children along the cross axis), the child's `text_max_width` is automatically set to the container's content-box width if no explicit `text_max_width` is specified. This enables natural text wrapping inside columns without manual width configuration:

```animatix
col: Col, gap: 8, padding: 16 {
  // text_max_width auto-set to col's content width
  title: Text, text: "Wrapped title inside a column", font_size: 24
  body: Text, text: "Body text that wraps naturally at the column's width"
}
```

> **Note:** `Row` does NOT propagate width to children (it places them side-by-side). Use `Col` or `Grid` when you want text to wrap at the container width.

### Transform Property

All actors support a `transform` property: a 6-element array `[a, b, c, d, tx, ty]` representing a full 2D affine matrix. This coexists with `rotation` and `scale` as independent transform layers.

```animatix
sheared: Rect, size: (100, 100), transform: (1, 0.5, 0, 1, 0, 0)

> **Layout compatibility:** Unlike `at`/`position`, `transform` works seamlessly inside managed layouts. A child of a `Row` or `Col` can use `transform` to offset itself visually without the container losing track of its layout slot.

```animatix
row: Row, gap: 8, padding: 10 {
  a: Rect, size: (40, 40)                    // normal layout slot
  b: Rect, size: (40, 40), transform: (1, 0, 0, 1, -5, 0) // shifted left 5px visually, still occupies its slot
}
```

Multiplication order (left to right, applied right-to-left to points):
```
parent × translate(position) × transform(matrix) × rotate(rotation) × scale(scale)
```

Shorthand forms are accepted:
- `transform: (sx, sy)` → `[sx, 0, 0, sy, 0, 0]` (non-uniform scale)
- `transform: (a, b, c, d)` → `[a, b, c, d, 0, 0]` (linear map only)
- `transform: (a, b, c, d, tx, ty)` → full affine matrix

### Legend

`Legend` is an **annotation primitive** that auto-generates a color-coded legend from scene content.
It renders color swatches with compiled text labels, positioned at a configurable location. Labels
automatically choose a contrasting color based on the sampled scene background.

```animatix
legend: Legend, at: (100, 100), title: "Series",
        font_size: 14, swatch_size: 16, gap: 8,
        text_max_width: 240
```

**Legend properties:**

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `at` | Vec2 | `(100, 100)` | Position of the top-left corner of the legend |
| `title` | Str | `""` | Optional title rendered above the entries |
| `font_size` | Num | `14` | Label font size in points |
| `label_color` | Color / `auto` | `auto` | Label color; `auto` selects contrast against the scene background |
| `swatch_size` | Num | `16` | Color swatch size in scene units |
| `gap` | Num | `8` | Vertical gap between rows and title/entries |
| `text_max_width` | Num | `240` | Maximum label width before wrapping; `0` disables wrapping |

**Legend entries** are auto-generated after the scene is built. The scan includes color-bearing leaf
actors such as shapes, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, and `BarChart`; structural
actors such as graphs, grids, containers, and other legends are excluded. Full-viewport background
shapes using `fill` or `100%` sizing are excluded automatically. Auto labels are derived from the actor
label (`line_a` → `Line A`).

Actors can opt into or out of legend scanning with the generic `legend` property:

```animatix
line_a: Line, color: red, legend: "Revenue"
axis: Line, color: gray, legend: false
```

`legend` accepts `Bool | Str`: `false` hides the actor, a string supplies an explicit label, an omitted
value leaves participation automatic, and `true` force-includes the actor even when a heuristic would
exclude it (for example, a full-viewport background).

### Graph Coordinate Mapping

Actors declared inside a `Graph` block have their `at`/`position`/`from`/`to` properties automatically mapped from math coordinates to screen pixels.

```animatix
graph: Graph, at: (960, 540), size: (500, 500), x_domain: (-10, 10), y_domain: (-10, 10) {
  // These are math coordinates, mapped to screen automatically
  point: Ellipse, at: (2, 2), size: (10, 10)
  arrow: Line, from: (0, 0), to: (5, 5)
}
```

### Filter (Post-Processing)

`Filter` is a **container primitive** that renders its children to an offscreen texture and applies post-processing filters before compositing back to the parent scene.

```animatix
bg: Filter, blur: 40, brightness: 0.5 {
  img: Image, url: "photo.jpg", size: fill
}
```

**Filter properties** (all animatable via keyframes):

| Property | Default | Description |
|----------|---------|-------------|
| `blur` | 0 | Gaussian blur radius in px |
| `brightness` | 1.0 | Multiplier on all channels |
| `contrast` | 1.0 | Contrast curve offset |
| `saturate` | 1.0 | 0 = grayscale, 1 = unchanged |
| `hue_rotate` | 0 | Hue rotation in degrees |
| `sepia` | 0 | Sepia intensity (0–1) |

Pipeline order: **blur → color matrix → opacity**. Nested filters are allowed but each level adds one offscreen pass.

### Audio

`Audio` is a non-visual actor that embeds an external audio file into the exported video.

```animatix
music: Audio, source: "background.mp3"
voice: Audio, source: "voiceover.wav", volume: 0.8 [5s, delay: 1s]
```

**Audio properties:**

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `source` | String | — | Path to the audio file (relative or absolute) |
| `volume` | F32 | 1.0 | Playback volume multiplier (0.0–1.0) |

**Timing modifiers:**
- First bare time literal (for example `[5s]`) trims playback to that duration. Without this, the full file plays.
- `delay: <time>` shifts the audio start time on the global timeline.
- Do not use `duration: 5s`; named `duration` modifiers are rejected.

During video export, all `Audio` actors from the current scene (or all scenes in a composition) are mixed together. Overlapping clips are blended; each clip respects its individual `volume` and start offset.

### Available Primitives & Common Confusions

**Shapes:** `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`

**Text-like:** `Text`, `Typst`, `Code`, `Svg`, `Image`

**Plotting:** `Graph`, `PlotCurve`, `BarChart`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`

**Containers:** `Row`, `Col`, `Grid`, `Stack`, `Group`, `Filter`, `Mask`

**Annotations:** `Callout`, `Legend`

**Other:** `Audio`

> **Not supported (common LLM hallucinations):**
> - `Circle` — use `Ellipse` with equal `size`
> - `Triangle` — use `Polygon` with 3 points
> - `Graph3D`, `Line3D`, `Polyhedron` — 3D is not supported; all rendering is 2D
> - `Chart`, `Diagram` — use `Graph`, `PlotCurve`, or `BarChart`

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

**BarChart** — produces a set of rectangular bars from `data: {(key, value), ...}` lists. Supports standalone mode (pixel coords) and Graph-child mode (math coords). See `examples/projects/fft_explain.amx`.

### Built-in Variables

The following variables are automatically available in `always` blocks and expression evaluation contexts:

| Variable | Type | Description |
|----------|------|-------------|
| `t` | `Num` | Scene-local time in **seconds**, starting at `0.0` when the scene begins. In multi-scene compositions, `t` resets to `0` at each scene boundary; it does **not** accumulate across scenes. |
| `scene_width` | `Num` | Scene width in pixels (from `config { resolution: (w, h) }`). |
| `scene_height` | `Num` | Scene height in pixels. |

**Scene anchor points** (available as `Vec2`):
```animatix
scene.top_left     scene.top       scene.top_right
scene.left         scene.center    scene.right
scene.bottom_left  scene.bottom    scene.bottom_right
```

**Actor anchor points** (available as `Vec2`):
Every actor exposes world-space bounding-box anchor points using the scene
vocabulary: `label.top_left`, `label.top`, `label.top_right`, `label.left`,
`label.center`, `label.right`, `label.bottom_left`, `label.bottom`,
`label.bottom_right`. They resolve lazily from the frame environment, so they
follow layout, transforms, keyframes, and `always` overrides.

```animatix
always {
  cursor.at = n0.top + (0, -20)
  link.from = n0.right
  link.to = n1.left
}
```

**Actor property lookups:** Any actor label and property is accessible: `ball.position`, `title.color`, etc.

> **Note:** `always` is stateless — variables do not persist between frames. Physics-style integration should use analytical expressions of `t` (e.g., `position = p0 + v0*t + 0.5*a*t²`) or keyframe tracks. Per-actor stateful updaters are not planned. See the Icebox notes on interactive step control and eval-path unification in `docs/roadmap.md`.

### Animation State Flags

For every INJECTABLE property, a boolean flag `{label}._animating_{property}` is
injected into the `always` evaluation environment. The flag is `1` when the
property is **currently interpolating between two keyframes** (the next keyframe
strictly after the current time has a non-Linear easing, indicating a real
animation target rather than a build-time snapshot). It is `0` when the property
is at rest (between animation segments, before the first keyframe, or after the
last).

This lets reactive blocks detect when a property is being driven by keyframes
and defer to interpolation:

```animatix
always {
  // Only override opacity when no keyframe is animating it
  if circle._animating_opacity == 0 {
    circle.opacity = 0.5
  }
}
```

> **Why not just check for keyframe existence?** The build pipeline inserts
> snapshot keyframes (with `Easing::Linear`) as scaffolding for duration-based
> animations. A property that has keyframes may still be at rest between
> animation segments. The `_animating_*` flag distinguishes active
> interpolation (snapshot → target with user-specified easing) from rest
> (target → next snapshot, both with the same value).

Available flags follow the property name: `_animating_at`, `_animating_position`,
`_animating_size`, `_animating_rotation`, `_animating_scale`,
`_animating_transform`, `_animating_color`, `_animating_opacity`, etc.

---

### Programmatic Actor Generation

**Status:** Implemented in parser and build.

Actors can be generated programmatically using `for` loops with an index variable
and array-indexed labels.

```animatix
for mag, i in magnitudes {
  bars[i]: Rect, size: (12, mag * 180), color: accent.primary
}
```

The `for item, i in list` form binds both the element value and a zero-based
index. Inside the body, `name[expr]: Type` declares an array actor element:
`bars[i]` produces labels `bars__0`, `bars__1`, etc.

Generated actors are first-class timeline actors — they support re-declaration
morphing, property assignment, and all built-in actions.

```animatix
#0s
for mag, i in zeros {
  bars[i]: Rect, size: (12, 0)
}

#2s
for mag, i in values {
  bars[i]: Rect, size: (12, mag * 180) [800ms]
}

#3s
bars[2].color = accent.danger
fade-in bars[0] [300ms]
```

Array-indexed declarations work inside containers (`Row`, `Col`, `Grid`,
`Graph`, etc.) and at the top level inside keyframes.

**Limitations:**
- Generated actor labels use `__` as an internal separator (reserved prefix).
- Array index expressions must evaluate to non-negative integers.
- `always` blocks reject actor declarations (array-indexed or otherwise).
- Very large generated arrays should prefer specialized primitives like
  `BarChart` for performance.

**Runtime-indexed assignment targets:** Inside `always`, an assignment target
may use a runtime index expression:

```animatix
always {
  bars[i].color = accent.danger
  bars[i].opacity = 1.0
}
```

The target resolves to `bars__{i}` at frame time and requires the matching
for-loop-generated actor set. Non-integer or out-of-range indices warn and are
skipped. Actions remain build-time only.

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

Re-export chains are resolved transitively. Nested aliased imports are available through arbitrary-depth paths such as `lib.inner.export_name`. Values are evaluated at build time in the importing scene's environment.

**Current limitations:**
- Text/Typst/Code property assignment recompiles glyph paths at timeline build/assignment time. Changing text content directly inside `always` is not supported as a render-time path.

---

## 12. Components

**Definition:**
```animatix
pub component MetricCard(title: Str = "Metric", value: Str = "0") {
    frame: Rect, size: (240, 120), color: blue
    title_text: Text, text: title, at: (0, -20)
    value_text: Text, text: value, at: (0, 24)
    badge: Ellipse, size: (24, 24), color: gold
}
```

**Component bodies** are pure actor templates — not scene containers. Only the following are valid inside `component { ... }`:
- Actor declarations (`label: Type, props`)
- Custom `action` blocks
- Assignments (`actor.prop = value`)
- `let` declarations
- Control flow: `if`, `for`, `sequence`, `stagger`
- Reactive blocks: `always`
- Comments
- Nested `component` definitions

The following are **not allowed** in component bodies: `config { }`, keyframes (`#0s`/`#+1s`), `import`, `use`,
`play`, scene declarations (`# SceneName`), and `viewport` declarations.

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
pub component Button(text: Str = "OK") {
    action pulse(amount: Num = 1.2) {
        self.scale = amount [100ms]
        self.scale = 1.0 [100ms]
    }
    frame: Rect, size: (100, 40)
}
```

Invoke on any instance:
```animatix
btn: Button, text: "Click"

#0s
pulse btn [amount: 1.3, 200ms]
```

**Semantics:**
- Custom actions are **inlined at component expansion time**. The invocation is replaced with the action's body statements.
- Invocation timing modifiers **override** body timing modifiers. `pulse btn [200ms]` replaces any `[100ms]` in the body with `[200ms]`.
- Named invocation modifiers bind to action parameters. `pulse btn [amount: 1.3, 200ms]` binds `amount` and overrides timing. Bound params may also be used as field paths in the body, e.g. `point.x` after `point` is bound.
- Use `self` to refer to the component instance. `self.scale` rewrites to `btn.scale`.
- Multi-target invocation (`pulse btn1, btn2`) expands the action body once per matching target, rewriting `self` to each concrete target.
- Actions work inside `sequence`, `stagger`, and keyframes with correct timing.

**Limitations:**
- Actions cannot be defined at module scope (only inside components)

### 12.1 Slots

Slots allow component authors to declare fillable regions that can be customized at instantiation time.

**Declaring slots:** Place the `@slot` marker inside a container's children block.

```animatix
pub component SlideLayout {
  backdrop: Rect, size: fill, color: scene.background, anchor: scene.center

  // Required-style slot — empty if not filled:
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

## 13. Type Annotations

Animatix has a **gradual type system**. Type annotations are optional and may be added to component and action parameters. When present, the type checker validates property assignments and action invocations at build time, reporting type mismatches as diagnostics.

### 13.1 Syntax

Parameter forms are:

```animatix
name
name: Type
name: default_value
name: Type = default_value
```

Type annotations appear after a colon in parameter declarations:

```animatix
pub component Button(size: Vec2, color: Color, label: Str) {
    bg: Rect, size: size, color: color
    text: Text, text: label
}
```

Action parameters follow the same syntax:

```animatix
pub component Badge(color: Color) {
    action pulse(count: Num, intensity: Num) {
        // count and intensity are typed parameters
    }
}
```

Named type aliases make reusable annotations explicit:

```animatix
type LegendMode = Bool | Str
pub type Metric = Num | Str

pub component MetricCard(value: Metric) {
    // value accepts any alias-compatible type
}
```

Aliases are resolved before type checking, so they can wrap primitive types, unions, lists, and other aliases.
`pub type` aliases are exported by aliased module imports and must be referenced by `::`-namespaced
name in the importing scene, e.g. `types::Metric` after `import "./types.amx" as types`. Simple names
remain local to their defining file, avoiding cross-module collisions. The legacy dotted form
`types.Metric` is accepted and normalized to `types::Metric` for compatibility.

### 13.2 Available Types

| Type | Description | Literal Syntax |
|------|-------------|----------------|
| `Num` | Numeric values (integers and floats) | `42`, `3.14`, `-1.5` |
| `Str` | String literals | `"hello"`, `"path/to/file.amx"` |
| `Bool` | Boolean values | `true`, `false` |
| `Vec2` | 2D vector | `(100, 200)`, `(50%, 75%)` |
| `Vec4` | 4D vector | `(0.5, 0.2, 0.8, 1.0)` |
| `Color` | RGBA color | `rgb(0.38, 0.78, 1.0)` |
| `Actor` | Actor reference | `btn`, `self` |
| `Scene` | Scene reference | `scene` |
| `List<T>` | Homogeneous list of type `T` | `{1, 2, 3}` (inferred as `List<Num>`) |
| `A \| B` | Union of accepted types | `Bool \| Str`, `Num \| Str` |
| `Any` | Top type, accepts any value | — |

**Properties of `Color`** (inherited from `Vec4`):
| Accessor | Description |
|----------|-------------|
| `value.r` | Red channel |
| `value.g` | Green channel |
| `value.b` | Blue channel |
| `value.a` | Alpha channel |

### 13.3 Subtyping Rules

The subtyping relation `<:` is reflexive and transitive:

| Rule | Explanation |
|------|-------------|
| `Color <: Vec4` | A color **is** a 4D vector — all `Vec4` operations apply to `Color` values |
| `T <: Any` | Every type is a subtype of `Any` |
| `T <: T` | Identity — every type subtypes itself |
| `List<A> <: List<B>` iff `A <: B` | List subtyping is covariant with respect to element type |
| `T <: A \| B` iff `T <: A` or `T <: B` | An actual type is accepted when it matches any union branch |

**Examples:**
- A `Color` value may be assigned where `Vec4` is expected: `button.tint = rgb(1, 0, 0)` for a field typed `Vec4`.
- A `List<Color>` may be passed where `List<Vec4>` is expected.
- Any value may be passed where `Any` is expected: `debug(value: Any)`.

### 13.4 Type Inference

When no explicit type annotation is given, the type checker infers the type of common expressions:

| Expression | Inferred Type | Examples |
|------------|---------------|----------|
| Integer literal | `Num` | `42`, `0`, `-5` |
| Float literal | `Num` | `3.14`, `-0.5` |
| String literal | `Str` | `"hello"` |
| Boolean literal | `Bool` | `true`, `false` |
| Tuple `(a, b)` | `Vec2` | `(100, 200)`, `(50%, 75%)` |
| Tuple `(a, b, c, d)` | `Vec4` | `(0.1, 0.5, 0.8, 1.0)` |
| Function `rgb(r, g, b)` | `Color` | `rgb(0.38, 0.78, 1.0)` |
| Function `rgba(r, g, b, a)` | `Color` | `rgba(0.1, 0.5, 0.8, 0.5)` |
| Actor label | `Actor` | `btn`, `title` |
| List literal | `List<T>` (element-dependent) | `{1, 2, 3}` → `List<Num>` |

**Tuples of other arities** (3-tuple `(a, b, c)`) are not treated as vector types. For generic list values, use `{...}` syntax.

### 13.5 Examples

**With type annotations:**
```animatix
pub component Card(title: Str, width: Num, height: Num, bg: Color) {
    frame: Rect, size: (width, height), color: bg
    label: Text, text: title
}
```

The type checker verifies these at instantiation:
```animatix
my_card: Card, title: "Stats", width: 200, height: 150, bg: rgb(0.9, 0.9, 0.95)
// ✓ "Stats" : Str, 200 : Num, 150 : Num, rgb(...) : Color
```

```animatix
my_card: Card, title: 42, width: 200, height: 150, bg: rgb(0.9, 0.9, 0.95)
// ✗ type mismatch: expected 'Str' for parameter 'title', got 'Num'
```

**Without annotations (inference):**
```animatix
// No annotations — all types are inferred from usage
pub component Badge(label, size, color) {
    frame: Rect, size: size, color: color
    text: Text, text: label
}
```

**Example with `Any`:**
```animatix
pub component DebugBox(value: Any) {
    frame: Rect, size: (200, 50)
    // value can be Num, Str, Vec2, Color, etc.
}
```

**Example with `List`:**
```animatix
pub component Palette(colors: List<Color>) {
    // colors is a homogeneous list of Color values
    swatch1: Rect, size: (40, 40), color: colors[0]
    swatch2: Rect, size: (40, 40), color: colors[1]
    swatch3: Rect, size: (40, 40), color: colors[2]
}
```

### 13.6 Backward Compatibility

Type annotations are **optional everywhere**. Existing code without annotations continues to work unchanged:

- Component and action parameters without annotations accept any value (equivalent to `Any`).
- The type checker only fires on annotated parameters — unannotated code raises no type diagnostics.
- Adding annotations to existing components is a **non-breaking change** that enables stricter validation.

This means the type system can be adopted incrementally: annotate hot spots first (public component boundaries) while leaving internal code unannotated.

### 13.7 Strict Mode

Add `strict_types: true` to a `config` block to require type annotations on all component and action parameters:

```animatix
config { strict_types: true }

pub component Button(text: Str, size: Vec2) { ... }
// ✓ OK — all params are annotated

pub component Card(title) { ... }
// ✗ warning: parameter 'title' of component 'Card' is missing a type annotation
```

When strict mode is enabled:
- Unannotated parameters emit warnings
- Type mismatches continue to be reported as errors
- Existing annotated code requires no changes

---

## 14. Typst & Graphs

**`Graph`**: Container mapping logical domains to physical bounds. Supports axes, optional grid lines, and ticks.
```animatix
graph: Graph, x_domain: (-5, 5), y_domain: (-10, 30), size: (400, 400)
```

**`PlotCurve`**: Single-stroke curve plot. Supports `stroke_progress` animation
for incremental trace reveals. Set `stroke_progress: 0` at declaration, then
animate to `1`:

```animatix
signal: PlotCurve, kind: "cartesian", func: (x) => sin(x),
  stroke: accent.primary, stroke_width: 4, stroke_progress: 0

signal.stroke_progress = 1 [1.5s, ease: ease-out]
```

The `kind` property selects the sampling strategy:

**Runtime parameters:** PlotCurve closures can reference `let` variables
and declaration-time numeric properties. These are re-evaluated per frame,
so curves animate when the referenced values change:

```animatix
#0s
let freq = 2
curve: PlotCurve, kind: "cartesian", func: (x) => sin(freq * x),
  stroke: accent.primary, stroke_width: 3

always {
  freq = 2 + 3 * sin(t * 0.5)  // sweep frequency over time
}
```

Declaration-time numeric parameters can also be injected directly:

```animatix
curve: PlotCurve, kind: "cartesian", func: (x) => sin(freq * x),
  freq: 2, stroke: accent.primary
```

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

**Function transitions:** PlotCurve supports animated transitions between functions using output blending. The curve smoothly morphs from one function to another by interpolating their output values:

```animatix
#0s
curve: PlotCurve, kind: "cartesian", func: (x) => sin(x),
  stroke: accent.primary, stroke_width: 3

#2s
curve.func = (x) => cos(x) [1s, ease: ease-in-out]
```

At any point during the transition, the rendered curve is `y = lerp(from_func(x), to_func(x), progress)`. This works for cartesian, polar, and parametric plots. Implicit plots, VectorField, Heatmap, and ContourSet do not support transitions.

**Overlapping transitions:** If a new transition starts before the previous one completes, the system uses record-and-chain: it freezes the current blend state and uses it as the starting point for the new transition.

```animatix
#0s
curve: PlotCurve, kind: "cartesian", func: (x) => sin(x), stroke: accent.primary

#1s
curve.func = (x) => cos(x) [2s]  // starts at 1s, would end at 3s

#2s
curve.func = (x) => x^2 [1s]  // overlaps: freezes sin→cos at 50%, chains to x^2
```

See `examples/data/24_plot_transitions.amx` for complete demonstrations.



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
contours: ContourSet, func: (x, y) => x^2 + y^2, levels: {1, 4, 9}, resolution: 96, color: blue
```

**Closures:**
```animatix
(x) => x^2           // single param
(x, y) => x + y      // multiple params
```

**Built-in math:** `sin(x)`, `cos(x)`, `tan(x)`, `sqrt(x)`, `exp(x)`, `ln(x)`, `atan2(y, x)`, `clamp(val, min, max)`, `abs(x)`, `min(a, b)`, `max(a, b)`, `floor(x)`, `ceil(x)`, `lerp(a, b, t)`, `rand()`, `seeded_rand(seed)`, `format("template {}", value, ...)`

**Built-in constants:** `pi` (π), `tau` / `two_pi` (2π), `e` (Euler's number), `PI`, `TAU`, `E` (uppercase aliases).

**Easing helpers (for use in `always` blocks):** `ease_linear(t)`, `ease_in(t)`, `ease_out(t)`, `ease_in_out(t)`, `bounce(t)`, `elastic(t)`, `back(t)`, `expo(t)` — each takes a progress value `t` in `[0, 1]` and returns the eased progress.

### Typst vs LaTeX Cheat Sheet

The `Typst` primitive uses **Typst** syntax, not LaTeX. Common mistakes:

| LaTeX | Typst | Notes |
|-------|-------|-------|
| `\frac{a}{b}` | `frac(a, b)` | Function call, not command |
| `\frac{a+b}{c}` | `frac(a + b, c)` | Parentheses required for compound numerators |
| `\lim_{x \to 1}` | `lim_(x -> 1)` | Underscore + arrow, no braces |
| `\sum_{i=1}^{n}` | `sum_(i=1)^n` | Same pattern as lim |
| `\sqrt{x}` | `sqrt(x)` | Function call |
| `\pi` | `pi` | No backslash |
| `\alpha` | `alpha` | Greek letters are bare words |
| `\times` | `times` | Multiplication symbol |
| `x^2` | `x^2` | Superscripts work the same |
| `x_{ij}` | `x_(ij)` | Subscripts use parens |
| `\int_a^b` | `integral_a^b` | Named function |
| `\infty` | `infinity` | Named constant |

**Full Typst math reference:** <https://typst.app/docs/reference/math/>

> **Typst highlight support:** Typst actors support per-actor highlight properties (`highlight_color`, `highlight_opacity`, `highlight_padding`, `highlight_radius`, `highlight_blend`) and the `highlight`/`unhighlight` built-in actions for whole-actor highlighting. This is distinct from Equation+Fragment per-segment highlighting — a Typst actor is highlighted as a single unit.

---

## 15. Expressions & Access

### List vs Tuple

Animatix distinguishes between **fixed-size tuples** `(...)` and **variable-length lists** `{...}`:

| Syntax | Name | Usage |
|--------|------|-------|
| `(x, y)`, `(r, g, b, a)`, `(-6, 6)` | Tuple | Vec2, Vec4, colors, domain ranges (fixed arity) |
| `{a, b, c}`, `{1, 4, 9}` | List | Points, commands, levels, data, for-iterables (variadic) |

Tuples of length 2 are inferred as `Vec2`, length 4 as `Vec4`. All other tuples produce a generic tuple type.
Lists are always inferred as `List<T>` and can be empty `{}` or single-element `{42}`.

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
let items = {10, 20, 30}
let first = items[0]    // 10

let text = "hello"
let ch = text[1]        // "e"

let pos = (100, 200)
let x = pos[0]          // 100
```

Supported: `List`, `Str` (char), `Vec2/3/4`, `Color`.

### List Mutation Helpers

`list_swap(list, i, j)` returns a new list with two elements swapped.
`list_set(list, i, value)` returns a new list with one element replaced. Both
are intended for build-time algorithm precomputation.

```animatix
let arr = {5, 2, 8}
let swapped = list_swap(arr, 0, 2)   // {8, 2, 5}
let replaced = list_set(arr, 1, 9)   // {5, 9, 8}
```

### Match

Animatix supports Rust-style `match` as both an expression and a statement.
Patterns include literal values, ranges (`1..=3`), or-patterns (`0 | 2`), and
tuples. A `_` wildcard arm is required.

```animatix
color = match floor(t) % 2 {
  0 => red,
  _ => blue,
}
```

```animatix
match floor(t) % 3 {
  0 => { bars[0].color = red }
  1 => { bars[1].color = red }
  _ => {}
}
```

### Method Calls

Methods dispatch on the receiver's type:

```animatix
let text = "hello,world"
let parts = text.split(",")     // List["hello", "world"]
let n = text.length()           // 13

let items = {1, 2, 3}
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

Returns a `Value::Object` with typed fields. Field reads (`p.x`, `p.a.b`) are implemented; e.g. `p.x + p.y` works after `let p = Point { x: 10, y: 20 }`. Field writes (`p.x = 30`, `p.a.b = 10`) are supported for build-time variable tracks and frame-local `always` object variables.

---

## 16. Known Gaps & Limitations

- **Media source assignment:** `Image.url` and `Svg.url` assignments support timed keyframe animation. Image sources and SVG path sets snap between sources at the midpoint of a timed assignment; SVG assignments also update measured size. Text/Typst/Code assignment recompiles glyph paths at timeline build/assignment time.
- ~~**Static Geometry:** Structural geometry inputs like `Polygon.points` and `Path.commands` are declaration-time only and cannot be animated dynamically frame-by-frame.~~ Both now support timed assignments with path morphing.

---

## 17. CLI Export

**Video (`animatix video`) and GIF (`animatix gif`) exports require the `video` feature** (see AGENTS.md for the Nix/FFmpeg setup):

| Flag | Default | Behavior |
|------|---------|----------|
| `--duration` | *auto* | Omit to use timeline length + hold |
| `--hold` | 1.0 | Trailing hold in seconds; ignored when `--duration` is set |
| `--fps` | 30 (video), 15 (GIF) | Output framerate |
| `--width` / `--height` | 1280x720 (video), 640x360 (GIF) | Output resolution |

**Auto-duration:** If `--duration` is omitted, the CLI builds the timeline, reads `Timeline::duration_seconds()` (the time of the last keyframe across all tracks, background, and child-order animations), and adds a trailing hold (configurable via `--hold`, default **1.0s**). This prevents the export from cutting off bluntly at the last animation's end frame.

```bash
# Export full timeline + 1s hold
animatix gif examples/layout/12_reorder.amx -o out.gif

# Export with a 2-second trailing hold
animatix gif examples/layout/12_reorder.amx -o out.gif --hold 2.0

# Export explicit 3-second slice (hold is ignored when --duration is set)
animatix gif examples/layout/12_reorder.amx -o out.gif --duration 3.0
```

**Parallel rendering:** Video and GIF exports render frames in parallel using all available CPU cores. Each thread gets its own GPU context and a cloned Timeline, then renders a chunk of frames. Encoding (GIF quantization / video muxing) remains sequential to preserve frame order and codec state.

**WebM output:** Use the `--codec vp9` flag and a `.webm` output extension to export in WebM format with VP9 video and Opus audio. When `--codec auto` (default) is combined with a `.webm` output path, VP9 is auto-selected. Audio segments are muxed using `libopus` instead of `aac` for WebM output.

```bash
# Export to WebM with VP9
animatix video examples/composition/20_feature_reel.amx -o out.webm --codec vp9

# Auto-detect WebM from output extension (auto-selects VP9)
animatix video examples/composition/20_feature_reel.amx -o out.webm

# WebM with custom resolution and framerate
animatix video examples/composition/14_multiscene.amx --width 960 --height 540 --fps 24 -o out.webm
```

```bash
# Low-FPS quick preview
animatix gif examples/composition/20_feature_reel.amx -o out.gif --fps 10
```

**Image export (`animatix image`):** Renders a single frame at `--time` (default 0s). No trailing hold or parallelization applies.

**GUI Preview Audio:** The `animatix-gui` preview panel plays audio segments from `Audio` actors in sync with the timeline during playback. Audio files are decoded via `rodio` and cached in memory. Playback automatically starts, seeks, and pauses in sync with the timeline controller. Missing or unplayable audio files produce a warning but do not block preview.

### Multi-Scene Composition

Multi-scene compositions support the same export flags. Duration is auto-detected from the composition's global timeline (`Composition::global_duration_s`) rather than a single timeline.

```bash
# Export a multi-scene composition
animatix video examples/composition/14_multiscene.amx --width 1280 --height 720

# GIF export with quick preview settings
animatix gif examples/composition/19_cross_file_scenes.amx --width 640 --height 360 --fps 10
```

---

## 18. Multi-Scene Composition

> **Status:** Parser, composition engine, CLI export, transition blending, cross-file scene composition, GUI scene list, composition scene track, and scene editing are shipped.

### Scene Declarations

Scenes are declared using `# SceneName` at the top level:

```animatix
# Intro
title: Text, text: "Welcome"

#1s
fade-in title [500ms]
```

### Transitions

The `play` statement defines the successor scene and transition:

```animatix
# Intro
title: Text, text: "Welcome"

#1s
fade-in title [500ms]

play Diagram [fade, 300ms]

# Diagram
graph: Rect, size: (400, 400)
```

**Supported transitions:** `cut`, `fade`, `wipe-left`, `wipe-right`, `wipe-up`, `wipe-down`. Transition blending uses a dual offscreen render path with the `TransitionCompositor` WGSL shader.

### Per-Scene Configuration

A scene may contain its own `config` block after the scene declaration. Scene-scoped keys (`colorscheme`, `dynamic_layout`, `duration`) override the prelude; composition-scoped keys (`resolution`, `strict_types`) are ignored with a warning. See [Config Merge Semantics](#config-merge-semantics) below.

```animatix
# Intro
config { colorscheme: "editorial-dark" }
title: Text, text: "Welcome"
```

### Shared Prelude

Top-level statements before the first scene (imports, `pub let`, file-level `config`) are shared across all scenes:

```animatix
import "./theme.amx" as theme
pub let accent = theme.accent

config { resolution: (1280, 720) }

# Intro
title: Text, text: "Welcome", color: accent

# Diagram
graph: Rect, size: (400, 400)
```

### Cross-File Scene Composition

Scenes defined in imported files can be composed across modules using aliased imports:

```animatix
// effects.amx
# FadeIn
title: Text, text: "Welcome"
fade-in title [500ms]

# WipeTransition
content: Rect, size: (400, 400)
```

```animatix
// main.amx
import "effects.amx" as effects

# Intro
title: Text, text: "Hello"
#1s
fade-out title [300ms]
play effects.FadeIn [fade, 300ms]

# Diagram
graph: Rect, size: (200, 200)
play effects.WipeTransition [wipe-right, 500ms]
```

Cross-file scenes are referenced as `alias.SceneName` in `play` statements. Each imported scene retains its own file's prelude (shared components, `pub let` values, config) for timeline compilation.

- Non-aliased imports (`import "file.amx"`) still flatten their scenes into the current file (backward compatible).
- Scene-scoped config keys (`colorscheme`, `duration`) in the imported file apply to its scenes.

### Config Merge Semantics

When both the shared prelude and a scene define `config` blocks, the **scene-level config takes precedence** for scene-scoped keys. The prelude provides base defaults; scene config overrides them.

```animatix
config { resolution: (1280, 720), colorscheme: "default-dark" }

# Intro
config { colorscheme: "editorial-dark" }  // overrides prelude colorscheme
// Intro uses: resolution (1280, 720) from prelude, colorscheme "editorial-dark" from scene config

# Diagram
// No scene config — inherits both resolution and colorscheme from prelude
```

**Config key scopes:**

| Key | Scope | Scene override? | Notes |
|-----|-------|----------------|-------|
| `resolution` | Composition | ❌ No | Set once in the prelude; affects canvas size and export dimensions. Scene-level `resolution` is ignored with a warning. |
| `strict_types` | Program | ❌ No | Enables strict type checking for the entire file. Scene-level `strict_types` is ignored. |
| `colorscheme` | Scene | ✅ Yes | Scene-level overrides prelude. Each scene can have a different colorscheme. |
| `dynamic_layout` | Scene | ✅ Yes | Scene-level overrides prelude. Enables per-frame layout recomputation. |
| `duration` | Scene | ✅ Yes | Scene-only; sets explicit scene duration (overrides keyframe-inferred duration). |

**Merge rules:**
- The shared prelude statements are prepended to every scene's body before timeline compilation.
- Scene-scoped keys (`colorscheme`, `dynamic_layout`, `duration`) override the prelude.
- Composition-scoped keys (`resolution`, `strict_types`) are inherited from the prelude only; scene-level overrides produce a warning and are ignored.
- Properties present only in the prelude are inherited by all scenes.
- Properties present only in a scene's config apply only to that scene.

### Backward Compatibility

Files without `# SceneName` declarations are single-scene files — all existing syntax, semantics, and behavior are preserved exactly. The parser produces the same AST as before; the timeline builder follows the existing single-timeline path.

### CLI Export

Multi-scene compositions are automatically detected and routed via `BuildTarget`. All export commands (`video`, `gif`, `image`) work identically for both single-scene and multi-scene files.

### Current Limitations

- **Multiple `play` targets** — a scene may have only one `play` statement; additional `play` statements emit a warning and are ignored.

---

## 19. Scene Persistence

> **Status:** Fully implemented. `persist` and `remove` actions are parsed, built, and carried across scene transitions via the CarryBag mechanism.

Actors can persist across scene transitions using `persist` and `remove` actions.

### Syntax

```animatix
persist actor1, actor2, ...
remove actor1, actor2, ... [duration]
```

### Semantics

- `persist` marks an actor to be carried into the next scene at its final state (position, size, color, opacity, etc.).
- `remove` fades out the actor over an optional duration and clears its persistence flag, preventing further carry.
- Persisted actors appear in the next scene at their final state; if the next scene re-declares the same label, it morphs from the carried state.
- Persistence is **sticky**: once an actor is persisted, it propagates through every subsequent scene automatically until an explicit `remove`.
- Persisting a container carries its entire subtree (all children) automatically.

### Examples

```animatix
config { colorscheme: "editorial-dark", resolution: (1280, 720) }

# Intro
title: Text, text: "Welcome", font_size: 48, at: (640, 200)
badge: Circle, radius: 30, at: (100, 100), color: accent.primary

#1s
persist title, badge

#2s
play Main [fade, 500ms]

# Main
// title and badge are carried from Intro
#0s
title.text = "Main Scene" [800ms]

#3s
remove badge [1s]

#4s
play Outro [fade, 500ms]

# Outro
// title still here (carried from Main), badge was removed
#0s
title.text = "The End" [800ms]

#2s
remove title [1s]
```

### Auto-Color Carry

When an actor uses `color: auto`, its auto-color slot is preserved across the carry. The actor keeps the same auto-cycle color in the destination scene even if the colorschemes differ.

### Layout-Managed Children

Actors managed by layout containers (e.g. children of `Row`, `Col`, `Stack`) have their world-space position resolved at the scene exit time. The carried actor is re-rooted to its absolute world position in the destination scene, decoupling it from the source layout.

To carry layout-managed children, persist their **container**, not the individual children:

```animatix
row: Row, at: (640, 360), gap: 20 {
    a: Rect, size: (80, 80)
    b: Rect, size: (80, 80)
}

#1s
persist row  // carries row + a + b together
```

### Warnings

| Code | Meaning |
|------|---------|
| `PersistIgnoresDuration` | `persist` was given a duration argument (ignored) |
| `PersistLayoutManagedChild` | `persist` targets a layout-managed leaf child directly (persist the container instead) |
| `PersistTargetNotCarried` | `persist` used in the last scene or a single-scene file (no successor to receive the carry) |
| `CarryAmbiguousPredecessor` | Scene has multiple predecessors in the play graph (only walk-order predecessor is used) |
| `PersistAfterRemove` | `persist` follows `remove` for the same actor in the same scene |

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
