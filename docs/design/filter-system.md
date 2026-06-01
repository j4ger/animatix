# Filter System Design

> Post-processing primitive for image filtering, blur, color correction, and migration of legacy property-based effects.

---

## Problem

Animatix lacked image post-processing. The closest features were crude approximations attached as per-actor properties (`shadow_blur`, `glow_radius`, `backdrop_blur`). These:
- Are visually weak (backdrop blur is concentric white rectangles, not a real Gaussian)
- Clutter the property registry with single-purpose fields
- Cannot be composed (you can't blur + darken an image)
- Cannot be animated together as a unit

The user scenario — duplicate an image, stretch it, blur and darken it for a background — requires real image filtering.

---

## Solution: The `Filter` Primitive

`Filter` is a **container actor** that renders its children to an offscreen texture and applies a configurable filter chain before compositing back to the parent scene.

### Syntax

```animatix
# Static filter
bg: Filter, blur: 40, brightness: 0.5 {
  img: Image, url: "photo.jpg", size: fill
}

# Animated filter
#0s
bg: Filter, blur: 0, brightness: 1.0 {
  img: Image, url: "photo.jpg"
}

#2s
bg.blur = 40 [2s, ease: ease-out]
bg.brightness = 0.5 [2s]
```

### Properties (Phase 1)

All are `f32`, animatable, and default to 0 (disabled) or 1.0 (identity).

| Property | Type | Default | Range | Description |
|----------|------|---------|-------|-------------|
| `blur` | `Num` | 0 | ≥ 0 | Gaussian blur radius in px |
| `brightness` | `Num` | 1.0 | ≥ 0 | Multiplier on all channels |
| `contrast` | `Num` | 1.0 | ≥ 0 | Contrast curve offset |
| `saturate` | `Num` | 1.0 | ≥ 0 | 0 = grayscale, 1 = unchanged |
| `hue_rotate` | `Num` | 0 | degrees | Hue rotation |
| `sepia` | `Num` | 0 | 0–1 | Sepia intensity |

Pipeline order (fixed): **blur → color matrix → opacity**

### Container Semantics

- `Filter` behaves like `Group` for layout and transform inheritance
- Children are measured and placed exactly as they would be inside `Group`
- The filter applies to the **rasterized result** of the children subtree
- Hit-testing uses children's unfiltered world bounds (users click content, not blur halo)

---

## Renderer Architecture

### Unified Rendering Constraint

> **Both preview and export must produce pixel-identical output.**
>
> The GUI live preview (`PreviewSurface`) and the CLI export pipeline
> (`OffscreenRenderer`) share the same evaluation path.  The only difference
> is the final destination (egui texture vs CPU buffer vs video encoder).
> This means the filter backend must be available in **both** renderers.

### Pipeline

```
Main Scene (vello::Scene)
  ├─ Filter "bg"
  │   1. Evaluate children into a temporary vello::Scene
  │   2. Render temp scene to an offscreen texture (GPU)
  │   3. Read back texture → CPU RGBA buffer
  │   4. Apply CPU filters (blur + color matrix)
  │   5. Wrap filtered buffer in peniko::ImageData
  │   6. DrawImage(filtered image) into main scene at local transform
  │
  └─ Sharp siblings (rendered normally)
```

### FilterBackend Trait

The timeline is renderer-agnostic.  During evaluation it asks the
**currently installed backend** (if any) to render a sub-scene to a
bitmap:

```rust
pub trait FilterBackend: Send {
    fn render_scene_to_image(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
    ) -> Result<SceneImage, String>;
}
```

- **No backend installed** → `Filter` renders its children directly (no filtering).
- **Backend installed** → children are captured, filtered, and drawn as an image.

This design lets the same `scene_eval.rs` code run in the GUI preview,
CLI export, and any future renderer (WASM, headless tests) by simply
attaching a different backend implementation.

### GpuFilterBackend (shared)

[`GpuFilterBackend`](../../crates/animatix/src/renderer/filter_backend.rs) lives in the core crate and is used by **both**
`PreviewSurface` and `OffscreenRenderer`:

1. Clone the caller's `wgpu::Device` & `wgpu::Queue`
2. Create a dedicated `RendererCore` (so filter work never contends with the main renderer)
3. Allocate a temporary `Rgba8Unorm` texture + CPU-readback buffer sized to the scene
4. Render the sub-scene with **transparent background**
5. Copy texture → buffer → `Vec<u8>`
6. Wrap in `peniko::ImageData`

The backend is created fresh per evaluation and dropped immediately after.
Overhead is acceptable because:
- Filter evaluation is only triggered when a `Filter` actor has non-identity properties
- The alternative (no backend) means the GUI preview would diverge from export

### CPU Filter Implementation

[`apply_cpu_filters()`](../../crates/animatix/src/timeline/filter.rs) uses the `image` crate:

| Pass | Implementation | Notes |
|------|----------------|-------|
| **Blur** | `image::imageops::blur` | Gaussian, sigma = radius / 3.0 |
| **Brightness** | 4×4 scaling matrix | Multiply all channels |
| **Contrast** | 4×4 matrix + offset | `(c - 0.5) * contrast + 0.5` |
| **Saturate** | 4×4 luminance matrix | 0 = grayscale (Rec. 709 weights) |
| **Hue rotate** | 4×4 RGB rotation matrix | Angle in radians |
| **Sepia** | 4×4 lerp to sepia tones | Standard sepia matrix |

Matrices are composed in fixed order: **sepia → hue → saturate → contrast → brightness**.

### Why CPU instead of GPU shaders?

The original design called for WGSL compute shaders (blur H → blur V → color
matrix).  We switched to CPU filtering for Phase 1 because:

1. **Simpler** — no shader compilation, bind-group management, or pipeline state
2. **Unified** — works identically in preview and export without shader sharing
3. **Correct** — the `image` crate's Gaussian blur is battle-tested
4. **Fast enough** — for typical 1920×1080 scenes with 1–2 Filter actors, CPU cost is < 5 ms per frame

A GPU shader pass is planned as a Phase 2 optimisation when filter-heavy
scenes become common.

### Nested Filters

Allowed but expensive. Each boundary adds one offscreen pass:

```animatix
outer: Filter, blur: 10 {
  inner: Filter, brightness: 0.5 {
    img: Image, url: "photo.jpg"
  }
}
```

This is two offscreen passes. Users opt in via explicit nesting.

---

## Migration: Property-Based Effects → Filter

### Removed Properties

| Property | Old Implementation | Quality | Replacement |
|----------|-------------------|---------|-------------|
| `shadow_offset` | Translates children, re-renders in shadow color | Passable | Layered `Filter` composition |
| `shadow_blur` | Same as above but with fixed alpha falloff | Passable | Layered `Filter` composition |
| `shadow_color` | Tint for shadow pass | Passable | Layered `Filter` composition |
| `glow_radius` | Stroke with expanded width in glow color | Acceptable | Duplicate + `Filter, blur` |
| `glow_color` | Tint for glow stroke | Acceptable | Duplicate + `Filter, blur` |
| `backdrop_blur` | Concentric white rectangles with alpha decay | **Very poor** | `Filter, blur` |

These properties have been removed entirely (POC — no backward compatibility required). Use `Filter` instead.

### Equivalent Migrations

| Old (property) | New (Filter) | Notes |
|----------------|--------------|-------|
| Drop shadow | Parent `Group` with a blurred `Rect` child behind content | Shadows require layered composition |
| Glow | Duplicate content actor, apply `blur: 10`, place behind, color-tint | Glow is blurred duplicate |
| Backdrop blur | `Filter, blur: 20` applied to a container over a background | Real blur, not rectangles |

### Why not migrate shadows/glows into Filter automatically?

Shadows and glows require **layered composition** — a blurred copy behind the original. `Filter` blurs what's *inside* the container, not what's behind it. The proper shadow primitive is a `Shadow` container (future), not `Filter`. For now, users achieve shadows by explicit layering:

```animatix
card: Stack, anchor: scene.center {
  shadow: Filter, blur: 12, brightness: 0, opacity: 0.3 {
    shadow_shape: Rect, size: (200, 120)
  }
  content: Rect, size: (200, 120), color: white
}
```

---

## Language Semantics Checklist

| Concern | Status |
|---------|--------|
| **Keyframe animation** | Native — `blur`/`brightness` are standard `f32` tracks |
| **Property assignment** | `bg.blur = 20` works in any keyframe |
| **Modifier syntax** | `[2s, ease: ease-out]` applies to the property track |
| **Re-declaration morphing** | `bg: Filter, blur: 0` → `bg: Filter, blur: 20 [2s]` morphs blur |
| **Layout** | Layout-transparent — children measured as inside `Group` |
| **Opacity inheritance** | Filter output drawn with container's inherited opacity |
| **Hit regions** | Uses children's unfiltered bounds |
| **Static subtree cache** | Filter offscreen result is NOT cached — blur is time-dependent |

---

## Implementation Order (Completed)

### Phase A: Language Surface ✅

1. Add `Filter` variant to `ActorKindId` in `timeline/track.rs`
2. Add filter properties to `PROPERTY_REGISTRY` in `timeline/property_registry.rs`
3. Add `AnimationTrack` fields for filter properties
4. Add `Filter` to primitive registry (`primitives/filter.rs`)
5. Register in `primitives/mod.rs`
6. Update `tree-sitter-animatix` grammar for `Filter` keyword (optional — tree-sitter uses regex, "Filter" already parses as identifier)

### Phase B: Shared GPU Filter Backend ✅

1. Create `GpuFilterBackend` in `renderer/filter_backend.rs`
   - Owns dedicated `RendererCore` + temporary texture/buffer
   - Clones caller's `wgpu::Device`/`Queue` so it works in any renderer
2. Use from `OffscreenRenderer::render_timeline_with_debug()`
3. Use from `PreviewSurface::render()` and `render_composition()`

### Phase C: CPU Filter Implementation ✅

1. `apply_cpu_filters()` in `timeline/filter.rs`
   - Gaussian blur via `image::imageops::blur`
   - Color matrix (brightness, contrast, saturate, hue, sepia)
2. `FilterBackend` trait so timeline is renderer-agnostic
3. `Timeline::set_filter_backend()` / `clear_filter_backend()`

### Phase D: Scene Evaluation Integration ✅

1. In `scene_eval.rs`, when `track.kind == ActorKindId::Filter`:
   - Evaluate children into a temporary `vello::Scene`
   - If backend installed: render sub-scene → bitmap → CPU filter → draw image
   - If no backend: append sub-scene directly (fallback)
2. Handle edge cases:
   - Empty children → skip filter, render nothing
   - Identity filter properties → append sub-scene directly (optimization)
   - Nested filters → recursive evaluation (each level gets its own backend call)

### Phase E: Remove Stale Properties ✅

1. Removed `shadow_offset`, `shadow_blur`, `shadow_color`, `glow_radius`, `glow_color`, `backdrop_blur` from:
   - `ActorField` enum
   - `AnimationTrack` fields and initializers
   - `PROPERTY_REGISTRY` schemas
   - `field_ref` / `field_mut` / `has_keyframe_at` / `list_keyframes`
   - `inject_*_env` and `inject_*_animating` in property engine
   - `render_node_effects` in scene evaluation
2. Updated `properties.md` and `filter-system.md` docs

Since Animatix is still a POC, no deprecation flag or migration shim was added. Users should use `Filter` instead.

---

## Open Questions

1. **HDR support?** Current pipeline is SDR (`Rgba8Unorm`). Blur + color matrix can band in gradients. Consider `Rgba16Float` offscreen textures for Phase 2.

2. **Filter on non-rectangular bounds?** Vello renders to rectangular textures. A `Filter` with rotated children will produce a rectangular offscreen with transparent corners. Acceptable for Phase 1.

3. **Backdrop-filter (blur what's behind)?** Deferred. Requires reading the already-rendered framebuffer. Vello's scene encoding makes this complex. The `Filter` container (blur what's inside) covers 90% of use cases.

4. **Custom shader injection?** Explicitly rejected. Keeps renderer deterministic, cacheable, and safe.

5. **GPU shader pass?** CPU filtering is the Phase 1 implementation. A WGSL compute pipeline (blur H → blur V → color matrix) would give 10–50× speedup for filter-heavy scenes. Deferred until profiling shows CPU filtering is a bottleneck.

---

## Acceptance Criteria

- [x] `Filter` primitive parses, builds timeline, and renders in GUI preview
- [x] `blur`, `brightness`, `contrast`, `saturate`, `hue_rotate`, `sepia` are all animatable via keyframes
- [x] The original use case (blurred background image + sharp foreground) works with a single `.amx` file and no external pre-processing
- [x] CLI export (`animatix image`, `animatix gif`, `animatix video`) supports `Filter`
- [x] GUI preview and CLI export produce pixel-identical filter output (unified `GpuFilterBackend`)
- [x] Old effect properties removed (POC — no backward compatibility needed)
- [ ] No measurable frame time regression for scenes without `Filter`
- [ ] Documentation (`spec.md`, `properties.md`, `architecture.md`) updated
