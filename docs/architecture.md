# Animatix V2 Architecture: The Vector-First Pipeline

This document outlines the architectural roadmap for transforming the `animatix` rendering engine from a **Raster-First** pipeline (using texture atlases and MSDF/Alpha maps) into a **Vector-First** pipeline powered by **Vello**.

## 1. The Goal

The primary objectives of the Vector-First architecture are:
1.  **Infinite Scalability:** Render text, math, and SVGs with perfect crispness at any zoom level, avoiding texture resolution limits.
2.  **True Vector Morphing:** Enable mathematically precise interpolation between different shapes (e.g., morphing the letter "A" into an SVG star).
3.  **Unified Rendering Model:** Stop treating Text, Shapes, and SVGs as separate rendering paths. Everything becomes a unified mathematical "Path".

## 2. The Core Tech Stack

The transition to Vello significantly changes our rendering dependencies:
*   **Vello (WGPU Compute):** The heart of the new engine. Vello uses GPU compute shaders to draw 2D paths incredibly fast, handling anti-aliasing and complex fills perfectly without the CPU triangulating shapes.
*   **Typst / Fontdue (Path extraction):** Instead of rasterizing glyphs to bitmaps, we extract the raw Bézier curves (outlines) of the glyphs for rendering.
*   **usvg:** For parsing standard SVG files into paths.

## 3. The Unified Pipeline

### Module Resolution & Dependency Graph (Load Time)

Before evaluation, the compiler parses all files and resolves imports to create a unified AST. 

1. **FileId Assignment**: Every file loaded is assigned a unique `FileId` (e.g., `FileId(u32)`). This acts as a lightweight, copyable handle to the file's AST and source text, heavily inspired by `rustc` and `rust-analyzer` patterns.
2. **Import Resolution**: When an `import "path.amx"` is encountered, the path is resolved absolutely. If the file is already in the `ModuleGraph` (or `SourceMap`), its existing `FileId` is returned, preventing redundant parsing and re-evaluation.
3. **Cycle Detection**: The loader tracks a `visited` set of `FileId`s during resolution. If an import resolves to a `FileId` currently in the `visited` set, a cyclic dependency error is thrown.
4. **Linking**: Actors marked with `pub actor` are exposed to importing files. The final output is a single, flattened AST graph ready for the timeline.

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
- **Container nodes** (`Row`, `Col`, `Group`) hold children and apply layout transforms.
- **Leaf nodes** (`Text`, `Math`, `Circle`, `Svg`) are fully resolved renderables.
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
5.  **Morphing (The "Manim" Effect):**
    *   If a `Morph { from: path_a, to: path_b }` node exists, the engine pairs the control points of `path_a` with `path_b`.
    *   It mathematically interpolates the XY coordinates of the curves based on the `blend_factor` (0.0 to 1.0).
    *   The result is a brand-new, intermediate path generated purely on the CPU for that exact frame.

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

## 5. Architecture Migration Steps

When we are ready to implement this, the migration will happen in these phases:
1.  **Add Vello dependency:** Update `Cargo.toml`.
2.  **Rip out the Texture Atlas:** Delete `msdf.rs`, `text_shader.wgsl`, and the `TextInstance` WGPU buffers.
3.  **Implement Path Extraction:** Update `text.rs` to extract `vello::kurbo::BezPath` objects instead of calculating bounding boxes for rasterization.
4.  **Setup Vello Renderer:** Rewrite `RendererCore` in `core.rs` to hold a `vello::Renderer`. Update the `render_image` and `render_video` loops to construct and render a `vello::Scene`.
5.  **Implement Morphing Engine:** Write the algorithm to match and interpolate points between two `BezPath` objects.
6.  **Add SVG Support:** Extend the `ast.rs` and `timeline/mod.rs` to support parsing and evaluating these new vector formats.
