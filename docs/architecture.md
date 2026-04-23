# Animatix Architecture: The Vector-First Pipeline

This document describes the vector-first architecture used by Animatix. The system renders text, math, and SVGs as mathematical paths, enabling infinite scalability, true vector morphing, and a unified rendering model.

## 1. The Goal

- **Infinite Scalability:** Render text, math, and SVGs with perfect crispness at any zoom level.
- **True Vector Morphing:** Mathematically precise interpolation between different shapes.
- **Unified Rendering Model:** Everything becomes a unified mathematical "Path".

## 2. The Core Tech Stack

- **Vello (WGPU Compute):** GPU compute shaders for 2D path rendering with anti-aliasing.
- **Typst / Fontdue:** Extract raw Bézier curves from glyphs instead of rasterizing.
- **usvg:** Parse SVG files into paths.

## 3. The Unified Pipeline

### Module Resolution (Load Time)

Files are parsed and imports resolved via `FileId` assignments. The `ModuleGraph` prevents redundant parsing and detects cyclic dependencies. Imported `pub component` definitions are expanded before timeline build.

*Note: Real-time hot-reloading is postponed until the UI/Editor phase.*

### Compile Boundary

The practical compile target is **the post-expansion program** after module loading and component expansion:

1. Parse source files into `Vec<Stmt>`
2. Resolve imports and collect component definitions through `ModuleGraph::load_program(...)`
3. Expand component instances with `LoadedProgram::expand_components()`
4. Lower into an executable `Timeline` with `Timeline::build(...)`

A future compiler should target the expanded program, not the raw parser AST.

### Build-Time Responsibilities

`Timeline::build(...)` performs one-time lowering between the expanded program and frame evaluation:

- Standard-library seeding for expression evaluation
- Scene-node and root-node construction
- Keyframe and property-track creation
- `for` expansion during body processing
- Built-in action lowering into track keyframes
- Text/math/code glyph extraction into renderable paths
- SVG and image asset loading
- Layout placement for `Row`, `Col`, `Grid`, and `Stack`
- Plotting geometry sampling for `CartesianPlot` and `PolarPlot`
- Collecting `always` / labeled-`always` bodies into the retained modifier list

The output is a compiled timeline package: scene graph structure, typed tracks, prebuilt assets/paths, and retained modifier statements.

### Colorscheme Integration

Colorscheme v1 is a load-time/build-time defaulting layer. Built-in scheme selection, semantic aliases, and `color: auto` resolve before timeline evaluation and seed property-track structures. This keeps preview, scrubbing, image export, and video export deterministic.

### The Timeline Data Structure

The `Timeline` stores animation state with two complementary structures:

1. **`scene_graph`**: A hierarchical mapping of parent `SceneNode` identifiers to their children, forming a tree where each node represents a rendered entity.

2. **`tracks`**: A `BTreeMap` mapping each `SceneNode`'s identifier to its `AnimationTrack`, storing keyframed property values over time.

```text
scene_graph: HashMap<SceneNodeId, Vec<SceneNodeId>>
tracks: BTreeMap<SceneNodeId, AnimationTrack>
```

The `scene_graph` enables parent-to-child traversal for transform inheritance; `tracks` provide per-node animation data. Nodes without track entries are static.

### SceneNode Hierarchy

- **Root nodes**: Attach directly to the scene.
- **Container nodes** (`Row`, `Col`, `Grid`, `Stack`, `Group`): Hold children and apply layout/transform rules.
- **Leaf nodes** (`Text`, `Math`, `Svg`, `Circle`, `Rect`, plot output paths): Fully resolved renderables.
- **Anonymous nodes**: Receive auto-generated UIDs for individual keyframing without label collisions.

### Evaluate Phase: Recursive DFS Transform Computation

During `timeline.evaluate(time_ms)`, the engine traverses the scene graph recursively:

```text
function evaluate_node(node_id, parent_transform):
    local_transform = tracks[node_id].sample(time_ms)
    global_transform = parent_transform * local_transform
    global_opacity = parent_opacity * local_opacity

    for each child in scene_graph[node_id]:
        evaluate_node(child, global_transform, global_opacity)
```

This DFS accumulates transforms and opacities down the hierarchy. The final render list contains only leaf nodes with pre-computed global transforms.

### Frame-Time Responsibilities

`Timeline::evaluate(...)` is the frame-time execution entry point:

- Seeding per-frame evaluation environment (`t`, scene dimensions, sampled runtime lookup values)
- Executing retained `always` modifier bodies through `apply_modifier_stmt(...)`
- Applying frame-local overrides on top of sampled track values
- Resolving scene-anchor, percent, and container-default position bindings
- Traversing the scene graph and sampling property tracks at the requested time
- Interpolating morphable path/text track data through `PropertyTrack::evaluate(...)`
- Emitting the final `vello::Scene` for rasterization

The renderer backends are host-side consumers of that evaluated scene. They do not own language semantics.

### IR and Bytecode Foothold

The first IR layer is a **modifier IR** for `always` / labeled-`always` bodies whose payloads are compiled expressions, housed under `timeline/modifier_runtime/`.

The IR stabilizes the expression subset crossing the build-time/frame-time boundary: `let` values, conditionals, assignment RHS, dotted runtime lookups (`node.at.x`, `scene.background_color`).

Unsupported forms (closures, method/index/construct expressions) remain on explicit rejection or AST fallback paths.

The bytecode VM compiles the modifier IR subset into a small stack machine. Scope is limited to compiled modifier expressions, `let`/`assign`/`if` modifier statements, env loads/stores, and built-ins `sin`, `cos`, `lerp`, and `format`.

## 4. Layout Architecture Direction

Animatix ships a **layout-first authoring model**. The design goal is:

1. **Auto-layout should be the default authoring path** for AI-generated and human-authored scenes.
2. **Absolute positioning remains a first-class escape hatch** for motion graphics and deliberate manual placement.
3. **Parent containers should own child placement** whenever layout semantics are in use.

The long-term scene model is not "remove `at`"; it is "stop requiring `at` as the primary way to compose scenes." Authors describe hierarchy, grouping, spacing, and alignment first, dropping to explicit coordinates only when intentionally wanted.

The shipped layout model has four layers (in order of preference):

1. **Container layout by default** (`Row`, `Col`, `Grid`, `Stack`) with gap, alignment, and predictable child ordering
2. **Scene-relative placement** (anchors, percentage-based positioning)
3. **Manual override within layout** (opt-out of container placement)
4. **Fully absolute positioning** (`at: (x, y)` for motion graphics)

This layered model prefers deterministic parent-driven layout over dynamic constraint solving.

### Near-Term Size-Aware Layout

The current runtime has **placement before full measurement truth**. Container layout already consumes child size data. Authored-shape primitives, images, and text/math/code declarations feed measured bounds into the layout size track. SVG paths are available for rendering.

The recommended direction: keep the parent-driven container model, formalize a local layout-size contract, and keep the first slice declaration-time and deterministic rather than promising sampled per-frame reflow.

## 5. Pipeline Phases

### Phase A: Parsing and Data Unification (Load Time)

When an `.amx` file is loaded, the compiler parses it, resolves imports, and converts assets into a unified `PathTree` format:

- **Text & Math:** Typst calculates glyph positions; font outlines are stored as paths.
- **SVGs:** Loaded via `usvg` and converted to path definitions.
- **Shapes:** `.amx` primitives (circles, rects) are generated as mathematical paths.

### Phase B: Animation Engine & Interpolation (Per-Frame)

- **Scene Graph Traversal:** The `Timeline` maintains a hierarchical `scene_graph` for nested coordinate systems.
- **Global Transform Computation:** Recursive DFS from roots to leaves, accumulating transforms.
- **Opacity Inheritance:** Opacity multiplies down the tree (child at 0.8 opacity inside parent at 0.5 = final 0.4).
- **Morphing:** When an actor is re-declared at a later keyframe, the engine interpolates path coordinates to generate an intermediate path on the CPU.

### Phase C: Vello Scene Compilation (GPU, Per-Frame)

1. The timeline yields a flattened list of paths and colors for the current frame.
2. Paths are pushed into a `vello::Scene` object.
3. `vello.render_to_texture(...)` is called.
4. Vello's compute shaders calculate coverage and draw pixels to the WGPU output texture.

## 6. Handling Specific Media

### Per-Letter Text Animation & Morphing

Text is a collection of discrete curve groups, not a single block or texture:
- Different transformation matrices can be applied to individual letters.
- Letter "A" can morph directly into letter "B" via curve interpolation.

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
