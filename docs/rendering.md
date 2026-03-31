# 1. Overview

This document specifies the rendering architecture for the Animatix animation engine. The system is built on wgpu (WebGPU) and utilizes a multi-pass rendering pipeline to support declarative primitives, morphing, and advanced styling features efficiently.

**Core Design Goals:**
1. **Performance:** GPU instancing for thousands of shapes.
2. **Flexibility:** Support for SDF (shapes) and Mesh (paths/SVG) primitives.
3. **Quality:** Anti-aliased edges via Signed Distance Fields (SDF).
4. **Extensibility:** Modular shader passes for effects (shadows, patterns).

---

# 2. Render Pipeline Architecture

The rendering process is divided into three distinct passes. Each pass renders to a specific texture target, which is then used as input for subsequent passes.

## Pass 1: Base Shapes (SDF + Mesh)
- **Purpose:** Render all fundamental geometry (fills, strokes, glow).
- **Input:** Instance buffers (SDF and Mesh), Uniforms.
- **Output:** Main Texture (Color + Alpha).
- **Technique:** GPU Instancing. SDF shapes use a full-screen quad with instance indexing. Mesh shapes use standard vertex/index buffers.

## Pass 2: Pattern Fills (Optional)
- **Purpose:** Render shapes requiring texture-based pattern fills.
- **Input:** Main Texture (as stencil/mask), Pattern Textures.
- **Output:** Pattern Texture.
- **Technique:** Render pattern quads masked by the shape geometry from Pass 1.

## Pass 3: Post-Process & Composite
- **Purpose:** Apply drop shadows, blur, and composite all passes.
- **Input:** Main Texture, Pattern Texture, Shadow Uniforms.
- **Output:** Swapchain / Final Video Frame.
- **Technique:** Full-screen quad shaders for blur and composite operations.

---

# 3. Shader Design (WGSL)

The shader system is modular. Common utilities are shared across passes.

## 3.1 Common Utilities (`common.wgsl`)

Contains math helpers, easing functions, and SDF definitions.

**Functions:**
- `ease_in_out(t)`: Standard easing curve.
- `rotate2d(pos, angle)`: 2D rotation matrix.
- `aa_edge(distance, width)`: Smoothstep for anti-aliasing.
- `sdf_circle(uv, radius)`: Signed distance for circle.
- `sdf_rect(uv, size)`: Signed distance for rectangle.
- `sdf_rounded_rect(uv, size, radius)`: Signed distance for rounded box.

## 3.2 Pass 1: Base Shapes (`base_pass.wgsl`)

Handles solid fills, strokes, and glow effects for SDF primitives.

**Struct Uniforms:**
```wgsl
resolution: vec2<f32>
time: f32
background_color: vec4<f32>
```

**Struct Instance:**
```wgsl
position: vec2<f32>
size: vec2<f32>
rotation: f32
fill_color: vec4<f32>
fill_enabled: u32
stroke_color: vec4<f32>
stroke_width: f32
stroke_enabled: u32
glow_radius: f32
glow_color: vec4<f32>
opacity: f32
shape_type: u32
corner_radius: f32
target_position: vec2<f32>
target_size: vec2<f32>
target_shape_type: u32
target_corner_radius: f32
shape_blend: f32
```

**Bindings:**
- Font Atlas Texture (sampled, for text primitives)
- Font Sampler

**Vertex Shader:**
- Generates a quad scaled to the shape's local bounding box (including stroke and glow) (6 vertices).
- Calculates instance ID from vertex index.
- Passes UVs and Instance ID to fragment shader.

**Fragment Shader:**
- Converts UV to local object space.
- Applies rotation and scaling.
- Evaluates SDF based on `shape_type`.
- Calculates fill alpha using `aa_edge`.
- Calculates stroke alpha using SDF expansion.
- Calculates glow using expanded SDF.
- Composites fill, stroke, and glow.
- Applies global opacity.

## 3.3 Pass 1: Mesh Primitives (`mesh_pass.wgsl`)

Handles paths, polygons, and SVGs using vertex buffers.

**Struct Vertex:**
```wgsl
position: vec2<f32>
color: vec4<f32>
```

**Struct Uniforms:**
```wgsl
morph_progress: f32
resolution: vec2<f32>
```

**Vertex Shader:**
- Interpolates position between start and end morph targets.
- Applies easing to `morph_progress`.
- Converts to clip space.

**Fragment Shader:**
- Outputs interpolated color.

## 3.4 Pass 2: Pattern Fills (`pattern_pass.wgsl`)

Handles texture-based fills.

**Struct Uniforms:**
```wgsl
pattern_scale: vec2<f32>
pattern_offset: vec2<f32>
pattern_rotation: f32
```

**Bindings:**
- Pattern Texture (sampled)
- Pattern Sampler
- Mask Texture (from Pass 1)

**Fragment Shader:**
- Samples pattern texture with transformed UVs.
- Multiplies by mask alpha from Pass 1.
- Outputs patterned color.

## 3.5 Pass 3: Post-Process (`post_process.wgsl`)

Handles shadows and final composition.

**Struct ShadowUniforms:**
```wgsl
shadow_offset: vec2<f32>
shadow_blur: f32
shadow_color: vec4<f32>
```

**Bindings:**
- Scene Texture (Pass 1 output)
- Pattern Texture (Pass 2 output)
- Shadow Uniforms

**Fragment Shader:**
- Samples scene texture with offset for shadow.
- Applies box blur or Gaussian blur kernel.
- Composites shadow underneath scene.
- Composites pattern texture on top.
- Outputs final color.

---

# 4. Morphing Implementation

Morphing is handled differently based on primitive type.

## 4.1 SDF Morphing (Shapes)

**Strategy:** Interpolate SDF parameters in the fragment shader.

**Implementation:**
- `shape_blend` property (0.0 to 1.0) is passed per-instance to allow independent morphing timelines.
- Instance buffer includes both source and target geometry parameters.
- Shader evaluates both source and target SDFs.
- Result is `mix(source_sdf, target_sdf, shape_blend)`.
- **Example:** Circle to Square blends radius and box dimensions.

## 4.2 Point-Match Morphing (Paths)

**Strategy:** Interpolate vertex positions in the vertex shader.

**Implementation:**
- CPU pre-computes dual geometry (start_positions, end_positions).
- Vertex buffer contains both sets of positions.
- Vertex shader mixes positions based on `morph_progress`.
- Requires equal vertex counts (CPU resamples if necessary).

## 4.3 Fade Morphing (Incompatible)

**Strategy:** Cross-fade opacity.

**Implementation:**
- Two separate actors (source and target).
- Source opacity decreases (1.0 -> 0.0).
- Target opacity increases (0.0 -> 1.0).
- Handled via standard uniform updates.

---

# 5. Host Orchestration (Rust)

The CPU side manages resources, batching, and pass submission.

## 5.1 Render Pass Manager

**Struct RenderPassManager:**
```rust
device: Device
queue: Queue
textures: { main, pattern, shadow }
pipelines: { base_sdf, base_mesh, pattern, post_process }
buffers: { uniforms, instances, vertices }
```

**Function `render_frame(scene, output_view)`:**
1. Categorize actors into batches (SDF, Mesh, Pattern, Shadow).
2. Update uniform buffers (time, resolution).
3. Update instance buffers (transform, style).
4. Create `CommandEncoder`.
5. Begin Pass 1 (Base Shapes).
   - Set pipeline.
   - Bind groups.
   - Draw SDF instances.
   - Draw Mesh indices.
6. Begin Pass 2 (Pattern Fills) [If needed].
   - Set pipeline.
   - Bind pattern textures.
   - Draw pattern instances.
7. Begin Pass 3 (Post-Process).
   - Set pipeline.
   - Bind main and pattern textures.
   - Draw full-screen quad.
8. Submit commands to queue.

## 5.2 Buffer Management

- **Uniform Buffers:** Updated every frame. Contains global state (time, resolution) and pass-specific constants.
- **Instance Buffers:** Updated when actors change state. Uses GPU instancing for SDF shapes (one draw call per shape type). Dynamic sizing (grows if scene complexity increases).
- **Vertex Buffers:** Static for fixed geometry (SVG, Path). Double-buffered for morphing (start/end positions).

## 5.3 Batching Strategy

To minimize pipeline switches, actors are grouped by style:
1. Group by Primitive Type (Circle, Rect, Path).
2. Group by Render Pass (Base, Pattern, Shadow).
3. Group by Shader Variant (Solid Fill, Gradient, Stroke).

---

# 6. Data Structures

## 6.1 SDF Instance (GPU)

**Layout:**
```wgsl
position: vec2<f32>
size: vec2<f32>
rotation: f32
fill_color: vec4<f32>
stroke_color: vec4<f32>
stroke_width: f32
glow_radius: f32
opacity: f32
shape_type: u32
corner_radius: f32
target_position: vec2<f32>
target_size: vec2<f32>
target_shape_type: u32
target_corner_radius: f32
shape_blend: f32
```
**Size:** 256 bytes (aligned)

## 6.2 Mesh Instance (GPU)

**Layout:**
```wgsl
transform: mat3x3<f32>
fill_color: vec4<f32>
stroke_color: vec4<f32>
vertex_offset: u32
vertex_count: u32
opacity: f32
```
**Size:** 96 bytes (aligned)

## 6.3 Morph Uniforms (GPU)

**Layout:**
```wgsl
morph_progress: f32
shape_blend: f32
easing_type: u32
time: f32
```
**Size:** 16 bytes (aligned)

---

# 7. Performance Considerations

1. **Instancing:** Use `draw_indirect` for SDF shapes to render thousands of objects in one call.
2. **Texture Binding:** Limit bind group changes. Group actors by texture usage.
3. **Buffer Updates:** Only update buffers when data changes. Use `mapped_at_creation` for small uniforms.
4. **Resolution Scaling:** Render shadows and patterns at lower resolution (e.g., 50%) to save bandwidth.
5. **Culling:** Skip off-screen actors during categorization phase on CPU.

---

# 8. Extensibility

**New primitives can be added by:**
1. Adding a case to the SDF switch statement in `base_pass.wgsl`.
2. Adding a new instance structure if data requirements change.
3. Registering the new primitive type in the Rust categorization logic.

**New effects can be added by:**
1. Creating a new render pass module.
2. Adding the pass to the `RenderPassManager` orchestration.
3. Exposing new uniform parameters in the scene file syntax.
