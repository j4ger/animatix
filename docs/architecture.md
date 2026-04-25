# Animatix Architecture

## Overview

Animatix is a layout-first animation system with three core components:

1. **Parser** (Chumsky-based) — Converts `.amx` source into an AST
2. **Timeline** — Compiles AST into animated property tracks
3. **Renderers** (Vello/WGPU, PPM, Frame sequences) — Rasterizes evaluated scenes

## 1. File Processing Pipeline

### A: Source → AST

```
.amx File → Tree-sitter grammar (syntax highlighting)
         ↓
       Chumsky parser (semantic analysis)
         ↓
     AST (Expr, Stmt hierarchy)
```

### B: Module System

The `ModuleGraph` manages file dependencies:
- Tracks `import` declarations
- Resolves relative paths
- Collects `pub` exports (components via `pub component`, values via `pub let`)

### C: Timeline Compilation

`Timeline::build_with_diagnostics(...)` is the main compilation entry:
1. **Module Expansion** (`module/expand.rs`): Inlines component instances
2. **IR Compilation** (`timeline/modifier_runtime/`): Compiles `always` blocks to bytecode
3. **Track Building** (`timeline/build.rs`): Creates `AnimationTrack` entries per actor
4. **Layout Resolution** (`timeline/layout.rs`): Computes container placements

## 2. Data Structures

### Timeline

```rust
Timeline {
    tracks: BTreeMap<String, AnimationTrack>,  // Actor properties
    nodes: BTreeMap<String, SceneNode>,         // Parent→children hierarchy
    root_nodes: Vec<String>,                    // Top-level actors
    modifiers: Vec<Stmt>,                       // Reactive blocks
}
```

### AnimationTrack

Per-actor storage for animated properties:
- `position: PropertyTrack<[f32; 2]>` — Keyframed positions
- `opacity: PropertyTrack<f32>` — Keyframed opacity
- `color: PropertyTrack<[f32; 4]>` — Keyframed colors
- And many more (see `timeline/track.rs`)

### PropertyTrack

```rust
struct PropertyTrack<T> {
    keyframes: BTreeMap<u64, (T, Easing)>,  // time_ms → (value, easing)
    default_value: T,                        // Before first keyframe
}
```

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

## 4. Layout System

### Container Hierarchy

- **Row/Col**: Children arranged horizontally/vertically with gap
- **Grid**: CSS-like grid layout with cols/rows
- **Stack**: Overlapping layers

### Positioning Modes

| Mode | Implementation |
|------|---------------|
| `at: (x, y)` | Absolute coordinates in scene space |
| `anchor` + `offset` | Relative to named scene anchors |
| Container auto-placement | Computed by parent, stored in track |

## 5. Animation System

### Keyframe Timing

- **Absolute**: `#2s` — At 2 seconds
- **Relative**: `#+1s` — 1 second after last absolute keyframe
- **Stagger**: Offsets children by fixed interval

### Easing

Standard easing curve transformation applied to animation progress:

```rust
fn apply_easing(progress: f32, easing: Easing) -> f32 {
    match easing {
        Easing::Linear => progress,
        Easing::EaseIn => progress * progress,
        // ... curves
    }
}
```

### Path Morphing

When an actor is re-declared at a later keyframe, path data interpolates:
1. Extract paths from both declarations
2. Point-match using `lyon` or stretch-based alignment
3. Interpolate position coordinates

## 6. Rendering

### Vello Pipeline

```
evaluate(time_ms) → Vec<RenderCommand>
  ↓
for cmd in commands:
  match cmd {
    Fill { path, color } => scene.fill_path(path, color),
    Stroke { path, width, color } => scene.stroke_path(path, width, color),
    Image { ... } => draw image,
  }
  ↓
vello.encode(&mut encoder)
  ↓
render_pass.draw(encoder)
```

### PPM Output

CPU-side rasterization for file output:
- Renders scene to 4-channel RGBA buffer
- Outputs PPM format for video encoding

## 7. Expression Evaluation

The animation engine evaluates mathematical expressions for properties through an `evaluate_expr` function that uses an `Environment` for variable and function lookup. The environment is `Rc<RefCell<HashMap<String, Value>>>` for shared ownership with interior mutability.

Built-in functions include `sin`, `cos`, `lerp`, `rand`, and `format`. The evaluator handles numeric, vector, and color arithmetic directly on expression values.

Closures use arrow syntax `(x) => x^2`. When evaluated, arguments are evaluated in the caller's environment, a clone is made, parameters are bound, and the body is evaluated in that extended environment.

The plotting system (`CartesianPlot`, `PolarPlot`) samples closure `func` at discrete points across the domain. An adaptive sampling strategy recursively refines areas where the curve deviates significantly from a linear approximation. Discontinuities and asymptotes are detected via delta ratios; `NAN` values break the path to prevent artifacts.

## 8. The Reactive System

The reactive system resolves a fundamental conflict: static keyframes describe a fixed timeline, but dynamic behavior requires per-frame evaluation. The rendered frame at time `t` is a direct function of `t`.

### Per-Frame Evaluation Pipeline

Each frame executes a four-stage pipeline:

1. **Advance Time**: Determine the requested timeline time.
2. **Evaluate Keyframe Tracks (Base Layer)**: Sample all `AnimationTrack` entries at the current time.
3. **Execute Reactive Blocks (Modifier Layer)**: Run stateless `always` evaluation on top of the sampled scene state.
4. **Render**: Commit final property values to the render list.

The base layer is purely declarative. The modifier layer stays random-access and preview-friendly by deriving results from the requested time.

### Reactive Constructs

| Construct | When Resolved | Runtime Cost |
|-----------|---------------|--------------|
| `for` | Compile time | Zero |
| `always` | Per requested frame | Full re-evaluation |

**`for`**: Resolved during timeline construction. At runtime, there is no loop structure—only expanded scene/timeline data. Compatible with random-access preview.

**`always`**: Runs every frame without exception. Receives current frame state and produces values that override or compose with the base layer. Evaluation is stateless: expressions are evaluated fresh each frame with no hidden prior-frame dependence.

```text
always { ball.at = (mouse.x, mouse.y) }
```

Repeated behavior uses explicit time math inside `always`:

```text
always {
  pulse.size = if (t % 1.0) < 0.5 { (120, 120) } else { (180, 180) }
}
```

### Composition Rules

When both a keyframe track and an `always` block affect the same property:

1. Sample the keyframe track at the current time (base layer)
2. Evaluate the `always` block (modifier layer)
3. The modifier wins—`always` overrides keyframes unless explicitly designed to compose

### Target Language Promise

> The frame at time `t` should be derivable directly from `t`, the scene source, and the render dimensions.

This contract is the shipped evaluation model for keyframes, layout, sampled path/property lookup, and `always`.

## 9. Architecture Notes

The major vector-first migration is complete:

- **Vello-backed rendering** is the active rendering path.
- **Path-based text, math, SVG, plotting, and shape rendering** are part of the runtime.
- **Timeline-driven interpolation and path morphing infrastructure** exist.
- **Runtime container layout support** exists for `Row`, `Col`, `Grid`, and `Stack`.

The remaining work expands the runtime surface on top of this architecture rather than replacing the renderer.

## 10. Future: Editor-Timeline Sync

### Status: **Planned / Partially Implemented**

A key planned feature is bidirectional sync between the GUI timeline scrubber and the text editor. When the user scrubs to a keyframe, the editor should automatically scroll to that keyframe's source location.

### Implementation Path

**Current State:**
- ✅ `Span` struct added to `ast.rs` — tracks line/column for source locations
- ✅ `span: Option<Span>` field added to `Keyframe` and `RelativeKeyframe` statements
- 🔄 Parser span capture: Not yet implemented (using `chumsky` with span-aware combinators)
- 🔄 Timeline index: Not yet built (`time -> source location` mapping)
- ❌ Editor cursor control: Blocked by egui TextEdit limitations (no cursor/scroll API)

### Technical Blockers

1. **AST Spans**: Parser needs to capture `Rich<'src, char>` span info into AST nodes
2. **egui TextEdit**: No programmatic cursor or scroll control available
   - Alternative: Use tree-sitter for AST + integrate a proper code editor (egui_code_editor, lapce, etc.)
3. **Unsaved Edit Handling**: Source text may diverge from parsed state; mapping becomes stale

### Usage Pattern (When Complete)

```
Timeline Scrub → Find nearest keyframe time →
  Query time→span index → Editor.scroll_to(span.line)
```

The span infrastructure in the AST preserves forward compatibility while the GUI layer evolves.
