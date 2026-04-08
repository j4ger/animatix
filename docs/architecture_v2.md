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

In the new architecture, the rendering loop changes from pushing instances to a vertex buffer, to building a `vello::Scene` graph on the CPU every frame and compiling it on the GPU.

### Phase A: Parsing and Data Unification (Load Time)
When an `.amx` file is loaded, all visual assets are converted into a unified `PathTree` format (a collection of Bézier curves and fill/stroke commands).
1.  **Text & Math:** The Typst layout engine calculates positions. For each glyph, we fetch its mathematical outline from the font (using `fontdue` or `ttf-parser`) and store it as a path.
2.  **SVGs:** Loaded via `usvg` and converted to path definitions.
3.  **Shapes:** `.amx` primitives (circles, rects) are generated as mathematical paths.

### Phase B: The Animation Engine & Interpolation (CPU, Per-Frame)
During `timeline.evaluate(time_ms)`:
1.  **Affine Transforms:** Animations like position, scale, and rotation are applied by multiplying a transformation matrix against the base paths.
2.  **Morphing (The "Manim" Effect):**
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
