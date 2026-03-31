# 1. Overview

This document specifies the rendering architecture for the Animatix animation engine. The system is built on wgpu (WebGPU) and utilizes a multi-pass rendering pipeline to support declarative primitives, Manim-inspired multi-strategy morphing, and advanced styling features efficiently.

**Core Design Goals:**
1. **Performance:** GPU instancing using bounded quads for primitives to avoid overdraw.
2. **Flexibility:** Unified multi-strategy morphing supporting mathematical (SDF), geometric (Mesh), and compositional transitions.
3. **Quality:** Anti-aliased edges via Signed Distance Fields (SDF) where applicable, and smooth path tessellation.
4. **Extensibility:** Modular shader passes for effects (shadows, patterns).

---

# 2. Render Pipeline Architecture

The rendering process is divided into three distinct passes. Each pass renders to a specific texture target, which is then used as input for subsequent passes.

## Pass 1: Base Shapes (SDF + Mesh)
- **Purpose:** Render all fundamental geometry (fills, strokes, glow, images, text).
- **Input:** Instance buffers (SDF and Mesh), Vertex buffers, Uniforms.
- **Output:** Main Texture (Color + Alpha).
- **Technique:** 
  - **SDF Shapes & Text:** GPU Instancing. Generates tightly bounded quads (scaled to the object's local extents plus stroke/glow padding) to avoid fill-rate bottlenecks.
  - **Mesh Shapes & Images:** Standard vertex/index buffers. Images map via UV coordinates.

## Pass 2: Pattern Fills (Optional)
- **Purpose:** Render shapes requiring texture-based pattern fills.
- **Input:** Main Texture (as stencil/mask), Pattern Textures.
- **Output:** Pattern Texture.
- **Technique:** Render pattern quads masked by the shape geometry from Pass 1.

## Pass 3: Post-Process & Composite
- **Purpose:** Apply drop shadows, blur, and composite all passes.
- **Input:** Main Texture, Pattern Texture, Shadow Uniforms.
- **Output:** Swapchain / Final Video Frame.

---

# 3. Shader Design (WGSL)

## 3.1 Common Utilities (`common.wgsl`)
Contains math helpers, easing functions, and generic SDF definitions capable of parametric evaluation.

## 3.2 Pass 1: Base Shapes (`base_pass.wgsl`)
Handles solid fills, strokes, glow effects, and text glyphs for SDF primitives.

**Struct Uniforms:**
```wgsl
resolution: vec2<f32>
time: f32
background_color: vec4<f32>
```

**Bindings:**
- Font Atlas Texture (sampled, for text primitives)
- Font Sampler

**Vertex Shader:**
- Generates a local quad scaled to the shape's bounds (extents + stroke_width + glow_radius).
- Applies `path_arc` rotational matrices to positional interpolation during morphs.
- Passes local UVs, Atlas UVs, and instance parameters to fragment shader.

**Fragment Shader:**
- Evaluates the SDF using mathematically interpolated parameters.
- Calculates fill/stroke alpha using `aa_edge` (smoothstep).
- Composites fill, stroke, glow, or samples the font atlas using `uv_rect`.

## 3.3 Pass 1: Mesh Primitives (`mesh_pass.wgsl`)
Handles paths, images, and SVGs using vertex buffers.

**Vertex Shader:**
- Interpolates position and UVs between start and end morph targets using point-matching.
- Applies `path_arc` to curve the trajectory of moving vertices.
- Applies easing to `morph_progress`.

**Fragment Shader:**
- Outputs interpolated color or samples bound image textures using the vertex UVs.

---

# 4. Multi-Strategy Morphing Implementation

Taking inspiration from Manim, Animatix employs a rich multi-strategy morphing system. **The CPU (Rust) acts as the "Strategy Director," while the GPU remains a highly optimized "Dumb Executor."**

## 4.1 CPU Strategy Orchestration
During scene compilation, the Rust host evaluates the source and target primitives and selects the best transition strategy (or obeys user modifiers like `strategy: fade_transform`, `path_arc`, or `stretch`). 

If a strategy requires calculating intermediate geometry (like matching text glyphs or aligning vertex counts), the CPU pre-computes it. If a strategy requires per-pixel mathematical blending or curved trajectories, the CPU packs these into `morph_params` for the GPU to evaluate.

## 4.2 Morphing Strategies

### Strategy A: Parametric (`strategy: parametric`)
- **Trigger:** Morphing between compatible mathematical primitives (e.g., Rect to Rect, Circle to Rounded Rect).
- **Execution:** Rust creates a *single* `SdfInstance` containing both State A and State B parameters. The fragment shader evaluates a single mathematical SDF function with interpolated properties, resulting in zero-ghosting geometric transitions.

### Strategy B: Point-Match (`strategy: point_match`)
- **Trigger:** Morphing between generic paths, or morphing an SDF shape into a path.
- **Execution:** If an SDF shape is involved, the CPU auto-tessellates it into a vertex mesh. The CPU normalizes the vertex count between source and target, outputting a double-buffered `MeshVertex` array. The vertex shader linearly (or radially via `path_arc`) moves the points.

### Strategy C: Fade Transform (`strategy: fade_transform`)
- **Trigger:** Cross-fade scaling between any incompatible shapes (e.g., Image to Path).
- **Execution:** Rust creates *two* separate instances (one for A, one for B) and manually updates their `opacity` and `scale` uniforms on the CPU frame-by-frame. The GPU simply draws static shapes with changing opacities, avoiding complex shader branching.

### Strategy D: Cross-Fade (`strategy: cross_fade`)
- **Trigger:** Fading completely incompatible elements in-place.
- **Execution:** Identical to Fade Transform, but without bounds-scaling.

### Strategy E: Match Shapes (`strategy: match_shapes`)
- **Trigger:** Morphing compound shapes (Text to Text, SVG to SVG).
- **Execution:** CPU deconstructs the shapes, matches sub-components (glyphs/paths) based on geometry or identity, and dispatches individual `point_match` or `parametric` instructions for each piece. Unmatched pieces are assigned `fade_transform`.

---

# 5. Host Orchestration (Rust)

## Render Pass Manager
1. **Categorize & Pre-process:** Detect morph transitions, auto-tessellate SDFs if targeting a mesh, and align vertex buffers for `point_match`.
2. **Update Buffers:** Push updated uniforms, dynamically sized instance buffers, and updated morph progress/`morph_params`.
3. **Execute Passes:**
   - Draw SDF instances (Instanced quads via `draw_indirect`).
   - Draw Mesh indices.
   - Execute Pattern and Post-Process passes.

---

# 6. Data Structures (GPU Layouts)

To support Manim-style modifiers without bloating the pipeline, structs feature generic `shape_params` and `morph_params` vectors.

## 6.1 SDF Instance (GPU)
**Layout:**
```wgsl
position: vec2<f32>,            // 8 bytes
size: vec2<f32>,                // 8 bytes (Aligned 16)
uv_rect: vec4<f32>,             // 16 bytes (x, y, w, h for text atlas) (Aligned 32)
shape_params: vec4<f32>,        // 16 bytes (e.g., x: radius, y: start_angle) (Aligned 48)
fill_color: vec4<f32>,          // 16 bytes (Aligned 64)
stroke_color: vec4<f32>,        // 16 bytes (Aligned 80)
stroke_width: f32,              // 4 bytes
glow_radius: f32,               // 4 bytes
opacity: f32,                   // 4 bytes
shape_type: u32,                // 4 bytes (Aligned 96)
target_position: vec2<f32>,     // 8 bytes
target_size: vec2<f32>,         // 8 bytes (Aligned 112)
target_shape_params: vec4<f32>, // 16 bytes (Aligned 128)
target_shape_type: u32,         // 4 bytes
shape_blend: f32,               // 4 bytes
_padding1: vec2<f32>,           // 8 bytes (Alignment) (Aligned 144)
morph_params: vec4<f32>,        // 16 bytes (x: path_arc, y: stretch, z: flags, w: pad) (Aligned 160)
```
**Total Size:** 160 bytes (16-byte aligned)

## 6.2 Mesh Vertex (GPU)
**Layout:**
```wgsl
position: vec2<f32>,            // 8 bytes
target_position: vec2<f32>,     // 8 bytes (Aligned 16)
uv: vec2<f32>,                  // 8 bytes
_padding1: vec2<f32>,           // 8 bytes (Aligned 32)
color: vec4<f32>,               // 16 bytes (Aligned 48)
target_color: vec4<f32>,        // 16 bytes (Aligned 64)
morph_params: vec4<f32>,        // 16 bytes (x: path_arc, etc.) (Aligned 80)
```
**Total Size:** 80 bytes (16-byte aligned)

## 6.3 Morph Uniforms (GPU)
**Layout:**
```wgsl
morph_progress: f32,            // 4 bytes
shape_blend: f32,               // 4 bytes
easing_type: u32,               // 4 bytes
time: f32,                      // 4 bytes
```
**Total Size:** 16 bytes (16-byte aligned)

---

# 7. Performance Considerations

1. **Bounded Quads:** SDF shapes *must* be rendered as tightly fitting quads rather than full-screen passes to preserve fill-rate and avoid catastrophic overdraw.
2. **CPU Offloading:** Morphing logic like `fade_transform` and vertex normalization is executed on the CPU, ensuring the GPU shaders stay fast and branchless.
3. **Buffer Management:** Use double-buffered vertex layouts for morphing paths to avoid per-frame CPU-to-GPU memory uploads.

---

# 8. Extensibility

**New primitives can be added by:**
1. Defining the new mathematical formulation in `base_pass.wgsl` (if SDF) or adding a tessellation routine in Rust (if Mesh).
2. Mapping the properties to the generic `shape_params` vector in the instance buffer without changing the memory layout.