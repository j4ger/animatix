# Animatix Architecture

## Overview

Animatix is a layout-first animation system with three core components:

1. **Parser** (Chumsky-based) — Converts `.amx` source into an AST
2. **Timeline** — Compiles AST into animated property tracks
3. **Renderers** (Vello/WGPU, PPM, Frame sequences) — Rasterizes evaluated scenes

---

## 1. File Processing Pipeline

### Source ↔ AST

```
.amx File → Tree-sitter grammar (syntax highlighting)
         ↓
       Chumsky parser (semantic analysis)
         ↓
     AST (Expr, Stmt hierarchy)
         ↓
     to_source::stmts_to_source()  (re-serialization for GUI write-back)
```

The AST is round-trippable. The GUI inspector mutates the AST directly and re-serializes the entire file. Formatting is normalized during re-serialization.

### Module System

The `ModuleGraph` manages file dependencies: tracks `import` declarations, resolves relative paths, and collects `pub` exports (components via `pub component`, values via `pub let`).

### Timeline Compilation

`Timeline::build_with_diagnostics(...)` is the main compilation entry:
1. **Module Expansion** (`module/expand.rs`): Inlines component instances
2. **IR Compilation** (`timeline/modifier_runtime/`): Compiles `always` blocks to bytecode
3. **Track Building** (`timeline/build.rs`): Creates `AnimationTrack` entries per actor
4. **Layout Resolution** (`timeline/layout.rs`): Computes container placements

---

## 2. Data Structures

### Timeline

```rust
Timeline {
    tracks: BTreeMap<String, AnimationTrack>,
    nodes: BTreeMap<String, SceneNode>,
    root_nodes: Vec<String>,
    modifiers: Vec<Stmt>,
}
```

### AnimationTrack

Per-actor storage is organized into three tiers:

- **Header**: `label`, `kind: ActorKindId`, `first_seen_ms`, `children`
- **Geometry tier**: `position`, `motion_offset`, `size`, `layout_size`, `rotation`, `scale`, `placement_mode`, `position_binding`
- **Style tier**: `color`, `opacity`, `stroke_width`, `stroke_color`, `stroke_progress`, `fill_opacity`, `morph_options`
- **Payload** (kind-specific): `Shape { shape_type, line_from, line_to, arc_angles, points, vector_paths }`, `Text { content, text_paths }`, `Image { image }`, `Svg { svg_paths }`, `Plot { vector_paths }`, or `Empty`

### PropertyTrack

```rust
struct PropertyTrack<T> {
    keyframes: BTreeMap<u64, (T, Easing)>,  // time_ms → (value, easing)
    default_value: T,
}
```

---

## 3. Runtime Evaluation

### Frame-Time Pipeline

```
evaluate(time_s):
  1. clear scene
  2. background color
  3. for track in timeline:
       sample properties at time_ms
       build RenderCommand
  4. flatten to render list
  5. push to vello::Scene
```

### Sampling Logic

`PropertyTrack::evaluate(time_ms)`:
1. Find keyframes bracketing `time_ms`
2. Interpolate between prev and next using stored easing
3. Return interpolated value via `Interpolate` trait

---

## 4. Layout System

Animatix uses a **parent-driven layout system** with an explicit manual-placement escape hatch.

### Placement Modes

| Mode | Description |
|------|-------------|
| **Layout-managed** | Parent container owns placement (Row, Col, Grid, Stack) |
| **Scene-relative** | `anchor: scene.top` + `offset`, or percentage `at: (50%, 60%)` |
| **Manual absolute** | `at: (1180, 80)` — direct authored placement |

### Container Types

- **Row/Col**: Taffy-backed linear layout with `gap` and cross-axis `align`
- **Grid**: Taffy-backed grid with explicit `cols` and `gap`
- **Stack**: Special-cased; all admitted children share the same origin
- **Group**: Scene-graph grouping only; no layout algorithm

### Layout Measurement

Layout consumes a dedicated `layout_size` track per child:
- Shapes seed from authored geometry
- Text/Math/Code seed from measured glyph bounds
- Image seeds from intrinsic or authored size

Children without seeded `layout_size` are excluded from layout admission. Legacy `size` still exists for rendering compatibility.

### Dynamic Layout

When `config { dynamic_layout: true }` is enabled, admitted children are re-sampled from `layout_size` per frame and positions are recomputed. Membership remains static (build-time admission only); `gap`, `align`, `cols` do not animate.

---

## 5. Animation System

### Keyframe Timing

- **Absolute**: `#2s` — At 2 seconds
- **Relative**: `#+1s` — 1 second after last absolute keyframe
- **Stagger**: Offsets children by fixed interval

### Easing

Standard easing curves (`Linear`, `EaseIn`, `EaseOut`, `EaseInOut`, `Bounce`, etc.) transform animation progress.

### Path Morphing

Re-declaring an actor at a later keyframe triggers automatic path interpolation. The pipeline runs at 4 levels in `timeline/morph.rs`:

1. **List alignment** — match path count between source/target
2. **Subpath alignment** — match subpath count (centroid sort when `strategy: match`)
3. **Segment alignment** — equalize segment count by splitting longest Beziers
4. **Interpolation** — lerp points with optional arc curvature and bounds normalization

Shipped morph modifiers: `strategy: auto|match`, `path_arc`, `stretch`. `strategy: fade` is deferred.

---

## 6. Rendering

### Vello Pipeline

```
evaluate(time_ms) → Vec<RenderCommand>
  ↓
for cmd in commands:
  match cmd { Fill {..}, Stroke {..}, Image {..} }
  ↓
vello.encode(&mut encoder) → render_pass.draw(encoder)
```

### Export

- **PPM/PNG**: CPU-side RGBA buffer for single frames
- **Video/GIF**: Parallel frame rendering + FFmpeg muxing

---

## 7. Expression Evaluation

Expressions are evaluated via `evaluate_expr` using an `Environment` (`Rc<RefCell<HashMap<String, Value>>>`).

Built-ins: `sin`, `cos`, `lerp`, `rand`, `format`. Closures use arrow syntax `(x) => x^2`.

The plotting system (`CartesianPlot`, `PolarPlot`, `ParametricPlot`, `ImplicitPlot`) samples closure `func` at discrete points with adaptive refinement.

---

## 8. Reactive System

The reactive system provides per-frame dynamic behavior on top of the static keyframe base layer.

### Per-Frame Pipeline

1. **Advance Time** — determine requested timeline time
2. **Evaluate Keyframe Tracks (Base Layer)** — sample all tracks at current time
3. **Execute Reactive Blocks (Modifier Layer)** — run stateless `always` evaluation
4. **Render** — commit final values

### Constructs

| Construct | When Resolved | Runtime Cost |
|-----------|---------------|--------------|
| `for` | Compile time | Zero |
| `always` | Per requested frame | Full re-evaluation |

`always` has no hidden memory between frames. Repeated behavior uses explicit time math:

```animatix
always {
  pulse.size = if (t % 1.0) < 0.5 { (120, 120) } else { (180, 180) }
}
```

### Composition Rules

When both a keyframe track and an `always` block affect the same property, the modifier wins (`always` overrides keyframes).

### Language Promise

> For pure authored scenes, the frame at time `t` is a random-access function of the source, the requested time, and the render dimensions.

---

## 9. Property System

The property system uses a **registry-driven generic engine** to eliminate N×M match-block explosion.

### Schema

Every property is described by a static `PropertySchema` record:

```rust
struct PropertySchema {
    name: &'static str,
    value_type: ValueType,      // F32, Vec2, Color, String, etc.
    flags: PropertyFlags,       // ANIMATED | LAYOUT_AFFECTING | ASSIGNABLE | INJECTABLE
    field: ActorField,          // Which storage tier to write
    group: Option<GroupMembership>, // For compound cross-property resolution
}
```

The `PROPERTY_REGISTRY` is a static sorted slice. Lookup is O(log n) binary search.

### Engine

A single `process_declaration_property()` function handles all declaration-time property writes:
- Validates property against actor kind
- Deferrs compound properties to group handlers
- Executes build-time-only properties immediately
- Writes animated properties as keyframes

Assignment-time properties use `process_assignment_property()` with the same schema → field mapping.

### Group Handlers

Compound properties that need cross-property coordination:
- **PositionBinding**: `at` + `anchor` + `offset` → resolved binding
- **VectorShapeState**: `radius`, `sides`, `from`, `to`, `start_angle`, `sweep_angle`, `points`, `commands` → shape geometry
- **PlotDomain**: `x_domain`, `y_domain`, `t_domain`, `func`, etc. → plot curve builder
- **ContainerLayout**: `gap`, `align`, `cols` → container metadata

---

## 10. Colorscheme System

Colorschemes provide declarative color contracts with two pieces:

1. **Semantic tokens** — `scene.background`, `text.primary`, `accent.primary`, `surface.primary`, `stroke.default`
2. **Auto color pool** — deterministic distinct-color assignment via `color: auto`

### Precedence (lowest to highest)

1. Runtime hardcoded default
2. Colorscheme primitive-type defaults
3. Alias-based declaration defaults
4. `color: auto`
5. Explicit declaration values
6. Later timed assignments
7. Frame-local `always` overrides

### Surface

- Built-in schemes: `default-dark`, `default-light`, `editorial-dark`
- Inline definition: `let ocean = Colorscheme { extends: "default-dark", ... }`
- Module import (future): `import "theme.amx" as theme`

---

## 11. Source Write-Back (GUI)

The GUI inspector persists edits back to `.amx` source via **AST mutation + re-serialization**:

```
source_text ──parse──► AST (Vec<Stmt>)
      ▲                    │
      │                    │ GUI edits mutate AST
      │                    ▼
   write back         to_source::stmts_to_source()
```

**Components:**
- `to_source::ToSource` — serializes every AST node back to `.amx` syntax
- `source_edit_v2` — semantic edit API (`SetProperty`, `InsertProperty`, `InsertKeyframe`)

**Benefits over old byte-span surgery:** no span invalidation, no re-parsing, robust property aliasing.

**Trade-offs:** formatting is normalized; inline comments after properties are preserved via `Property.trailing_comment`, but blank lines and indentation style are not.

---

## 12. Module & Component System

### Imports

- `import "path"` — flattens imported statements into current scene
- `import "path" as name` — creates namespace for `pub let` exports (`name.export_name`)

### Components

```animatix
pub component MetricCard(title: "Metric") {
    frame: Rect, size: (240, 120), color: blue
    title_text: Text { text: title, at: (0, -20) }
}
```

- `pub` required for cross-file visibility
- Instance props bind by name to component params
- Nested labels are instance-prefixed (isolated per instance)
- External dotted assignment: `left.badge.color = red`
- Slots: `@slot` markers inside containers, filled via `@slotname { items }`

---

## 13. Analyzer & LSP

Language intelligence is shared via `animatix-analyzer`:

```
animatix (core parser/AST)
    ↓
animatix-analyzer (SymbolTable, Completer, Diagnostics, Hover, Definitions)
    ↓
animatix-gui (direct calls)    animatix-lsp (tower-lsp, JSON-RPC)
```

- **No I/O** in analyzer — pure computation on `&str` or `&[Stmt]`
- **Dual parsers**: chumsky for semantic AST, tree-sitter for position-based queries
- **LSP capabilities**: completion, hover, goto-definition, document symbols, diagnostics

---

## 14. File Structure

```
crates/
├── animatix/              # Core library
│   └── src/
│       ├── ast.rs         # AST types
│       ├── parser.rs      # Chumsky parser
│       ├── diagnostics.rs # Diagnostic types
│       ├── module.rs      # Module system
│       ├── source_index.rs# Source location mapping
│       ├── to_source.rs   # AST re-serialization
│       └── timeline/      # Timeline compilation, actions, morphing, plotting
│
├── animatix-analyzer/     # Shared language intelligence
│   └── src/
│       ├── lib.rs         # Analyzer struct
│       ├── symbol_table.rs# Symbol extraction
│       ├── completer.rs   # Completions
│       └── diagnostics.rs # Semantic diagnostics
│
├── animatix-lsp/          # LSP server (tower-lsp)
├── animatix-gui/          # Desktop GUI (eframe/egui)
│   └── src/
│       ├── app.rs         # Main app shell
│       ├── document.rs    # Document session
│       ├── editor.rs      # Code editor
│       ├── preview_surface.rs # GPU render surface
│       └── app/           # Submodules (inspector, transport, workspace, etc.)
│
└── tree-sitter-animatix/  # Tree-sitter grammar
```

---

*For language details, see [`spec.md`](spec.md). For work items, see [`roadmap.md`](roadmap.md).*
