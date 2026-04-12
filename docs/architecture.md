# Animatix Architecture: The Vector-First Pipeline

This document describes the current vector-first architecture used by Animatix and highlights a few design notes that still point toward future work. Historical migration notes are kept here only where they still help explain the current system.

## 1. The Goal

The primary objectives of the Vector-First architecture are:
1.  **Infinite Scalability:** Render text, math, and SVGs with perfect crispness at any zoom level, avoiding texture resolution limits.
2.  **True Vector Morphing:** Enable mathematically precise interpolation between different shapes (e.g., morphing the letter "A" into an SVG star).
3.  **Unified Rendering Model:** Stop treating Text, Shapes, and SVGs as separate rendering paths. Everything becomes a unified mathematical "Path".

## 2. The Core Tech Stack

The current renderer is built around these dependencies:
*   **Vello (WGPU Compute):** The heart of the new engine. Vello uses GPU compute shaders to draw 2D paths incredibly fast, handling anti-aliasing and complex fills perfectly without the CPU triangulating shapes.
*   **Typst / Fontdue (Path extraction):** Instead of rasterizing glyphs to bitmaps, we extract the raw Bézier curves (outlines) of the glyphs for rendering.
*   **usvg:** For parsing standard SVG files into paths.

## 3. The Unified Pipeline

### Module Resolution & Dependency Graph (Load Time)

Before evaluation, the compiler parses all files and resolves imports to create a unified AST. 

1. **FileId Assignment**: Every file loaded is assigned a unique `FileId` (e.g., `FileId(u32)`). This acts as a lightweight, copyable handle to the file's AST and source text, heavily inspired by `rustc` and `rust-analyzer` patterns.
2. **Import Resolution**: When an `import "path.amx"` is encountered, the path is resolved absolutely. If the file is already in the `ModuleGraph` (or `SourceMap`), its existing `FileId` is returned, preventing redundant parsing and re-evaluation.
3. **Cycle Detection**: The loader tracks a `visited` set of `FileId`s during resolution. If an import resolves to a `FileId` currently in the `visited` set, a cyclic dependency error is thrown.
4. **Linking**: Imported modules are flattened into a single AST for timeline compilation. Parser support for `pub component` definitions exists, but runtime component instantiation is still pending.

*Note on Hot-Reloading: While tracking file dependencies via a `ModuleGraph` naturally supports watching files for changes and invalidating specific `FileId`s, real-time hot-reloading is intentionally postponed until the UI/Editor phase to maintain a simple, stable evaluation model.*

### The Timeline Data Structure

The `Timeline` stores animation state using two complementary structures:

1. **`scene_graph`**: A hierarchical mapping of parent `SceneNode` identifiers to their child `SceneNode` identifiers. This forms a tree where each node represents a rendered entity (text, shape, SVG, or container).

2. **`tracks`**: A `BTreeMap` mapping each `SceneNode`'s identifier to its `AnimationTrack`, which stores keyframed property values over time.

```text
scene_graph: HashMap<SceneNodeId, Vec<SceneNodeId>>
tracks: BTreeMap<SceneNodeId, AnimationTrack>
```

The `scene_graph` enables parent-to-child traversal for transform inheritance, while `tracks` provide per-node animation data. Nodes without entries in `tracks` are static.

### SceneNode Hierarchy

`SceneNode`s form a tree with the following properties:
- **Root nodes** attach directly to the scene (no parent transform to inherit).
- **Container nodes** (`Row`, `Col`, `Grid`, `Stack`, `Group`) hold children and apply layout and transform rules.
- **Leaf nodes** (`Text`, `Math`, `Svg`, `Circle`, `Rect`, and plot output paths) are fully resolved renderables.
- **Anonymous nodes** receive auto-generated UIDs when no explicit label is provided, enabling individual keyframing without label collisions.

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

This DFS ensures all descendants receive correctly accumulated transforms and opacities. The final render list contains only leaf nodes with their pre-computed global transforms.

### Layout Architecture Direction

Animatix is moving toward a **layout-first authoring model** for scene composition.

The design goal is:

1. **Auto-layout should be the default authoring path** for AI-generated and human-authored scenes.
2. **Absolute positioning remains a first-class escape hatch** for motion graphics, fine-grained composition, and deliberate manual placement.
3. **Parent containers should own child placement** whenever layout semantics are in use.

This means the long-term scene model is not "remove `at`"; it is "stop requiring `at` as the primary way to compose scenes." Authors should be able to describe hierarchy, grouping, spacing, and alignment first, and only drop down to explicit coordinates when they intentionally want handcrafted placement.

The architectural consequence is that layout must become a real runtime concern rather than a one-off convenience pass. Containers should be able to:

- infer placement from layout rules
- expose explicit alignment and spacing semantics
- coexist with manually positioned children when needed
- preserve deterministic results for AI-authored scenes

### Current Layout Model

The shipped layout model has four layers, in order of preference:

1. **Container layout by default**
   - `Row`, `Col`, `Grid`, `Stack`
   - container-owned placement
   - alignment, gap, padding, and predictable child ordering

2. **Scene-relative placement**
   - anchors such as scene center / edges / corners
   - percentage-based or scene-derived placement for coarse composition

3. **Manual override within layout**
   - individual children may opt out of container placement where appropriate
   - explicit placement is allowed, but should be intentional rather than the baseline assumption

4. **Fully absolute positioning**
   - direct `at: (x, y)` remains supported for motion graphics and precise art direction

This layered model is deliberately simpler than a full general-purpose constraint solver. The architecture prefers deterministic parent-driven layout over highly dynamic constraint solving unless the simpler model proves insufficient.

For the concrete Phase 1 semantics and rollout slices, see [`layout_design.md`](layout_design.md).

### Phase A: Parsing and Data Unification (Load Time)
When an `.amx` file is loaded, the compiler parses it, resolves all imports (using `FileId` assignments to build the `ModuleGraph`), and converts all visual assets into a unified `PathTree` format (a collection of Bézier curves and fill/stroke commands).
1.  **Text & Math:** The Typst layout engine calculates positions. For each glyph, we fetch its mathematical outline from the font (using `fontdue` or `ttf-parser`) and store it as a path.
2.  **SVGs:** Loaded via `usvg` and converted to path definitions.
3.  **Shapes:** `.amx` primitives (circles, rects) are generated as mathematical paths.

### Phase B: The Animation Engine & Interpolation (CPU, Per-Frame)
During `timeline.evaluate(time_ms)`:
1.  **Scene Graph Traversal:** The `Timeline` maintains a hierarchical `scene_graph` mapping parent `SceneNode`s to their children. This tree structure enables true nested coordinate systems where transforms cascade down the hierarchy.
2.  **Global Transform Computation:** The engine performs a recursive depth-first search (DFS) from root nodes down to leaves, accumulating transforms along the way. A node's global transform equals its parent's global transform multiplied by its local transform.
    ```text
    global_transform(node) = parent.global_transform * local_transform(node)
    ```
    This applies to position, scale, and rotation. A circle positioned at (50, 0) inside a group rotated 90 degrees will orbit at (50, 0) relative to the group's center, then inherit the 90-degree rotation.
3.  **Opacity Inheritance:** Opacity also accumulates down the tree. A child with opacity 0.8 inside a parent with opacity 0.5 has a final opacity of 0.4 (0.5 * 0.8). This allows container-level fading to affect all descendants.
4.  **Affine Transforms:** Animations like position, scale, and rotation are applied by multiplying a transformation matrix against the base paths.
5.  **Morphing:**
    *   When a supported actor is re-declared at a later keyframe, the engine pairs compatible vector path data between the previous and next states.
    *   It mathematically interpolates the XY coordinates of the curves based on the sampled blend factor.
    *   The result is a brand-new intermediate path generated on the CPU for that exact frame.

### Layout Evaluation Implication

Today, some container layout behavior is applied during timeline construction. The target architecture should evolve toward a model where layout-relevant placement can be recomputed from sampled child state when necessary. This matters for scenes where child size, visibility, or spacing-related properties animate over time.

That does **not** mean every scene needs an expensive global layout solve every frame. It means layout containers should own a well-defined placement step whose semantics are compatible with timeline evaluation.

### Phase C: Vello Scene Compilation (GPU, Per-Frame)
1.  The timeline yields a final, flattened list of paths and their colors/gradients for the current frame.
2.  The engine pushes these paths into a `vello::Scene` object.
3.  The engine calls `vello.render_to_texture(...)`.
4.  Vello's compute shaders take over, calculating coverage and drawing the exact pixels to the WGPU output texture incredibly fast.

## 4. Handling Specific Media

### Per-Letter Text Animation & Morphing
Because text is no longer a single block or a texture lookup, but rather a collection of discrete curve groups:
*   We can apply different transformation matrices to the paths of individual letters (e.g., making the "e" in "Hello" jump).
*   We can morph the curves of the letter "A" directly into the curves of the letter "B".

## 5. Architecture Notes on What Is Already Landed

The major vector-first migration work is already reflected in the current repository:
1.  **Vello-backed rendering** is the active rendering path.
2.  **Path-based text, math, SVG, plotting, and shape rendering** are part of the current runtime.
3.  **Timeline-driven interpolation and path morphing infrastructure** already exist.
4.  **Runtime container layout support** exists for `Row`, `Col`, `Grid`, and `Stack`, with `Group` remaining the non-layout transform/grouping container.

The remaining work is mostly about expanding the runtime surface on top of this architecture rather than replacing the renderer again.

## 6. Expression Evaluation: Context-Aware Math Engine

The animation engine evaluates mathematical expressions for properties like positions, sizes, colors, and durations through a context-aware `evaluate_expr` path that uses an `Environment` for variable and function lookup.

### The Environment Pattern

The `Environment` provides runtime context for expression evaluation. It stores variables and built-in callable values alongside ordinary runtime values.

```text
Environment = Rc<RefCell<HashMap<String, Value>>>
```

Why this structure:
- **`Rc<RefCell<...>>`**: Allows shared ownership (multiple expressions can reference the same environment) with interior mutability (the environment can be modified at runtime).
- **`HashMap<String, Value>`**: Maps variable names to their runtime values. Keys are strings, making it easy to bind variables from the AST.
- **Nested scopes**: Environments can form a chain where outer scopes shadow inner ones, enabling proper lexical scoping.

Example environment contents:
```text
{
    "x": Num(100.0),
    "y": Num(200.0),
    "sin": <built-in function>,
    "PI": Num(3.14159...)
}
```

### Current Built-in Function Surface

The currently documented runtime-facing helper set is intentionally small:
- `sin`
- `cos`
- `lerp`
- `rand`
- `format`

The evaluator also supports numeric, vector, and color arithmetic directly in expression trees. This document should stay conservative about additional built-ins unless they are explicitly wired into the current runtime.

### Context-Aware Evaluation

The current `evaluate_expr` shape is conceptually:

```text
fn evaluate_expr(expr: &Expr, env: &Environment) -> Result<Value, EvalError>
```

Key points:

1. **Environment parameter**: Variable lookups consult the environment.
2. **Function calls**: Built-ins like `sin`, `cos`, `lerp`, `rand`, and `format` are resolved through that environment.
3. **Typed arithmetic**: The evaluator handles numeric, vector, and color arithmetic directly on expression values.
4. **Conservative docs**: Internal callable storage and exhaustive error details should be documented only when they match the current code precisely.

### Example Evaluation Flow

Given: `sin(x * PI / 180)` where `x = 90`

1. `evaluate_expr(Call("sin", [Binary(Ident("x"), Mul, ...)]), env)`
2. Look up `sin` in the environment as a built-in callable value
3. Evaluate arguments recursively:
   - `evaluate_expr(Ident("x"), env)` -> `Ok(Num(90.0))` (from env)
   - `evaluate_expr(Ident("PI"), env)` -> `Ok(Num(3.14159...))` (from env)
   - Evaluate the binary expression `x * PI / 180` -> `Ok(Num(1.57079...))`
4. Call the resolved built-in with the evaluated argument -> `Ok(Num(1.0))`

### Closure Evaluation

The system evaluates mathematical functions via closures that are natively parsed in the AST. Closures capture their lexical environment, enabling variable references from the surrounding scope.

#### Closure Syntax

Closures use arrow syntax with parameters and a body expression:

```text
(x) => x^2
(x, y) => x + y
(t) => sin(t) * cos(t)
```

#### Closure AST Node

```text
Expr::Closure {
    params: Vec<Ident>,      // Parameter names: [x, y]
    body: Box<Expr>,          // The expression body: x + y
    captured_env: Rc<RefCell<Environment>>, // Lexical environment
}
```

#### Evaluation Process

When evaluating a closure:

1. **Binding parameters**: Create a new local scope with parameter names bound to argument values
2. **Extending environment**: Push the local scope onto the captured environment chain
3. **Evaluating body**: Evaluate the closure body in this extended environment
4. **Restoring environment**: Pop the local scope after evaluation

```text
evaluate_closure(Closure { params: [x], body: x^2, cap_env }, [Num(3)])
1. local_scope = { "x": Num(3) }
2. extended_env = cap_env + local_scope
3. evaluate(body, extended_env) -> Num(9)
4. restore to cap_env
```

#### Graph Plotting with Closures

The `Graph` primitive maps logical mathematical domains to physical screen bounds. Child plots (`CartesianPlot`, `PolarPlot`) sample the closure `func` at discrete points across the domain.

**CartesianPlot evaluation:**
1. Graph determines logical x-range and sampling density
2. For each sample point `x_i` in the domain:
   - Evaluate `closure(x_i)` to get `y_i`
   - Map `(x_i, y_i)` from logical to physical coordinates
3. Connect sampled points to form the plot line

**PolarPlot evaluation:**
1. Graph determines logical theta-range and sampling density
2. For each sample point `theta_i`:
   - Evaluate `closure(theta_i)` to get `r_i`
   - Convert `(r_i, theta_i)` to Cartesian `(x, y)`
   - Map from logical to physical coordinates
3. Connect sampled points to form the curve

This approach treats functions as first-class values evaluated natively through the AST, rather than via string interpretation.

#### Adaptive Subsampling Algorithm

The plotting system uses an adaptive sampling strategy to efficiently render mathematical curves. Instead of uniformly sampling at a fixed resolution, the algorithm recursively refines areas where the curve deviates significantly from a straight line approximation.

**Algorithm Overview:**

1. **Initial sampling**: Start with a coarse resolution of 10 segments across the domain
2. **Midpoint evaluation**: For each line segment, evaluate the function at the true midpoint
3. **Tolerance check**: Compute the perpendicular distance from the midpoint to the linear segment connecting the endpoints
4. **Recursive subdivision**: If the distance exceeds the tolerance threshold, subdivide the segment at the midpoint and recurse
5. **Depth limit**: Cap recursion at a maximum depth of 10 to prevent infinite subdivision

**Pseudocode:**

```text
adaptive_sample(closure, x_start, x_end, depth):
    if depth >= max_depth:
        return [(x_start, closure(x_start)), (x_end, closure(x_end))]

    mid = (x_start + x_end) / 2
    y_start = closure(x_start)
    y_mid = closure(mid)
    y_end = closure(x_end)

    // Compute perpendicular distance from midpoint to line segment
    line_mid = (y_start + y_end) / 2
    distance = |y_mid - line_mid|

    if distance <= tolerance:
        return [(x_start, y_start), (x_end, y_end)]
    else:
        left = adaptive_sample(closure, x_start, mid, depth + 1)
        right = adaptive_sample(closure, mid, x_end, depth + 1)
        return left + right
```

**Discontinuity and Asymptote Detection:**

The engine detects steep asymptotes by monitoring the ratio of consecutive sample deltas. When a delta exceeds a threshold relative to the domain span, the algorithm identifies a potential discontinuity. Rather than drawing a misleading straight line through the asymptote, the engine injects `NAN` values into the path. Vello's path builder skips `NAN` coordinates, effectively breaking the path into separate segments. This prevents visual artifacts like vertical lines through `1/x` at zero.

**Bounding-Box Culling:**

During subdivision, the engine checks whether a segment lies entirely outside the graph's physical screen bounds. If both endpoints and the midpoint map to coordinates outside the visible region, the entire segment is discarded without further subdivision. This optimization reduces unnecessary function evaluations and path segments for portions of the curve that would not render anyway.

**AST Integration:**

The `tolerance` and `max_depth` parameters are extracted directly from the AST at parse time. `CartesianPlot` and `PolarPlot` nodes carry these values as properties, allowing the sampling engine to configure subdivision behavior per-plot without runtime string parsing.

**Why This Matters for Vello `BezPath` Performance:**

Vello renders paths by computing coverage per pixel. When you submit a path with many small segments, each segment still requires separate processing. The adaptive algorithm produces exactly the number of segments needed for visual accuracy: dense sampling where the curve bends sharply, sparse sampling where it is nearly linear.

This directly translates to:
- **Fewer path segments** submitted to Vello, reducing GPU command processing overhead
- **Smoother curves** at discontinuities without massive vertical lines from naive uniform sampling
- **Predictable performance** that scales with curve complexity rather than arbitrary resolution settings

The tolerance-based approach also means mathematical expressions like `sin(1/x)` near zero naturally receive more segments where oscillation is rapid, without requiring user-specified resolution parameters.

## 7. The Hybrid Evaluation Engine (Reactive System)

The reactive system resolves a fundamental conflict in animation: static keyframes describe a fixed timeline, but dynamic behavior requires per-frame evaluation. The shipped engine resolves this by making the rendered frame at time `t` a direct function of `t`.

### The Per-Frame Evaluation Pipeline

Each frame, the engine executes a four-stage pipeline:

1. **Advance Time**: Determine the requested timeline time.
2. **Evaluate Keyframe Tracks (Base Layer)**: Sample all `AnimationTrack` entries at the current time. This produces the default state for every animated property. Nodes without keyframes retain their static values from the scene graph.
3. **Execute Reactive Blocks (Modifier Layer)**: Run stateless `always` evaluation on top of the sampled scene state. `for` has already been expanded during timeline construction.
4. **Render**: Commit the final property values to the render list.

The key insight is that the base layer is purely declarative. It declares what the values *should be* at any given time. The modifier layer should stay random-access and preview-friendly by deriving results from the requested time rather than hidden execution history.

### Reactive Model

**`for` (compile-time structural expansion)**

`for` loops are resolved during timeline construction. At runtime, there is no loop structure, only expanded scene/timeline data.

```text
for item in items {
  // generate repeated structure during timeline build
}
```

The repeated structure is expanded before runtime evaluation and is compatible with random-access preview.

**`always` blocks (stateless render-time evaluation)**

An `always` block runs every frame without exception. It receives the current frame state and produces values that override or compose with the base layer.

```text
always { ball.at = (mouse.x, mouse.y) }
```

This runs for the requested frame. The expression `mouse.x` and `mouse.y` are evaluated fresh each time, giving live mouse tracking. There is no saved program counter and no hidden prior-frame dependence — just time/context-based evaluation.

**Repeated behavior**

Repeated behavior is expressed as explicit time math inside `always`, while structural repetition uses `for`.

```text
always {
  pulse.size = if (t % 1.0) < 0.5 { (120, 120) } else { (180, 180) }
}
```

This keeps evaluation stateless and random-access.

### Compile-Time vs Render-Time

The distinction matters for performance and semantics:

| Construct | When Resolved | Runtime Cost | State |
|-----------|---------------|--------------|-------|
| `for` | Compile time | Zero | Static keyframes |
| `always` | Per requested frame | Full expression re-evaluation | None (stateless) |

`for` produces no runtime overhead. The entire loop collapses into timeline data before the first frame renders.

`always` re-evaluates its expressions for each requested frame. If the expression tree is expensive, that cost is paid each time the preview/export asks for a frame.

### Composition Rules

When both a keyframe track and an `always` block affect the same property:

1. The base layer samples the keyframe track at the current time
2. The modifier layer evaluates the `always` block
3. The modifier wins. `always` overrides keyframes unless explicitly designed to compose (e.g., `ball.at.x = base.at.x + offset`)

This gives `always` the semantics of a render-time patch applied on top of the static timeline.

### Target Language Promise

The recommended long-term contract is:

> the frame at time `t` should be derivable directly from `t`, the scene source, and the render dimensions.

That contract is the shipped evaluation model for keyframes, layout, sampled path/property lookup, and `always`.
