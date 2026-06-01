# Filter System Design

> Post-processing primitive for image filtering, blur, color correction, and migration of legacy property-based effects.

---

## Problem

Animatix currently lacks image post-processing. The closest features are crude approximations attached as per-actor properties (`shadow_blur`, `glow_radius`, `backdrop_blur`). These:
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

### Pipeline

```
Main Scene (vello::Scene)
  ├─ Filter "bg"
  │   1. Create/allocate offscreen texture (size = child subtree bbox)
  │   2. Render children into offscreen vello::Scene
  │   3. Run filter compute shader:
  │        blur pass H → blur pass V → color matrix pass
  │   4. DrawImage(filtered texture) into main scene at local transform
  │
  └─ Sharp siblings (rendered normally)
```

### Shader Design

**Two-pass separable Gaussian blur** (horizontal then vertical):

```wgsl
// blur_h.wgsl / blur_v.wgsl
// 9-tap kernel, sigma = radius / 3.0
// weights computed from Gaussian distribution, normalized
```

**Color matrix pass** (single fullscreen quad):

```wgsl
// color_matrix.wgsl
// Build 4×5 matrix from brightness/contrast/saturate/hue_rotate/sepia
// Apply as: pixel = mat4x4 * pixel + offset
```

### Offscreen Texture Management

- `FilterTargetPool` owned by `PreviewSurface` and export renderers
- Key: `(width, height, format)`
- Reuse across frames; clear on acquire
- Budget: 4 buffers, LRU eviction
- Texture format: `Rgba8Unorm` or `Rgba16Float` for HDR

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

### Current State (to be deprecated)

| Property | Current Implementation | Current Quality |
|----------|------------------------|-----------------|
| `shadow_offset` | Translates children, re-renders in shadow color | Passable |
| `shadow_blur` | Same as above but with fixed alpha falloff | Passable |
| `shadow_color` | Tint for shadow pass | Passable |
| `glow_radius` | Stroke with expanded width in glow color | Acceptable |
| `glow_color` | Tint for glow stroke | Acceptable |
| `backdrop_blur` | Concentric white rectangles with alpha decay | **Very poor** |

### Migration Plan

**Step 1: Implement `Filter`** (this document)

**Step 2: Deprecate property-based effects**

Emit deprecation diagnostics when the following properties are used:
- `shadow_offset`, `shadow_blur`, `shadow_color`
- `glow_radius`, `glow_color`
- `backdrop_blur`

Diagnostic text:
```
'{property}' is deprecated. Use the 'Filter' primitive instead:
  old: actor.shadow_blur = 10
  new: wrap: Filter, blur: 10 { actor: Image, ... }
```

**Step 3: Remove after deprecation window**

Remove from `PROPERTY_REGISTRY`, `AnimationTrack`, and `scene_eval.rs`. Target: 2 releases after deprecation.

### Equivalent Migrations

| Old (property) | New (Filter) | Notes |
|----------------|--------------|-------|
| `shadow_offset: (4, 4), shadow_blur: 8, shadow_color: black` | Parent `Group` with a blurred `Rect` child behind content | Shadows require layered composition |
| `glow_radius: 10, glow_color: gold` | Duplicate content actor, apply `blur: 10`, place behind, color-tint | Glow is blurred duplicate |
| `backdrop_blur: 20` | `Filter, blur: 20` applied to a container over a background | Real blur, not rectangles |

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

## Implementation Order

### Phase A: Language Surface (1 day)

1. Add `Filter` variant to `ActorKindId` in `timeline/track.rs`
2. Add filter properties to `PROPERTY_REGISTRY` in `timeline/property_registry.rs`
3. Add `AnimationTrack` fields for filter properties
4. Add `Filter` to primitive registry (`primitives/filter.rs`)
5. Register in `primitives/mod.rs`
6. Update `tree-sitter-animatix` grammar for `Filter` keyword (optional — tree-sitter uses regex, "Filter" already parses as identifier)
7. Update `spec.md` and `properties.md`

### Phase B: Renderer — Offscreen Infrastructure (2 days)

1. Add `FilterTargetPool` to `PreviewSurface`
   - `acquire(width, height) -> FilterTarget`
   - `release(FilterTarget)`
2. `FilterTarget` = `wgpu::Texture` + `wgpu::TextureView` + size
3. Integrate pool into `PreviewSurface::new()` and resize path

### Phase C: Renderer — Filter Shader (2 days)

1. Write `blur_h.wgsl` — horizontal Gaussian blur
2. Write `blur_v.wgsl` — vertical Gaussian blur
3. Write `color_matrix.wgsl` — brightness/contrast/saturate/hue/sepia
4. Compile shaders at `PreviewSurface` init
5. Create `FilterPass` struct that owns bind groups + pipeline state

### Phase D: Scene Evaluation Integration (2 days)

1. In `scene_eval.rs`, when `track.kind == ActorKindId::Filter`:
   - Evaluate children normally into a temporary `vello::Scene`
   - Measure children's bounds (reuse `node_local_bounds` + `transform_rect_bbox`)
   - Allocate offscreen texture via pool
   - Render temporary scene to texture using vello's existing render path
   - Run filter shader passes (blur H → blur V → color matrix)
   - Draw resulting texture into main scene via `scene.draw_image()`
2. Handle edge cases:
   - Empty children → skip filter, render nothing
   - `blur: 0` and identity color matrix → skip shader, draw children directly (optimization)
   - Nested filters → recursive offscreen passes (each level gets its own texture)

### Phase E: CLI Export (1 day)

1. `encode/` renderers also need `FilterTargetPool`
2. Share pool between preview and export via trait or ref-counted pool
3. Update `render_video_composition`, `render_gif_composition`, `render_image`

### Phase F: Deprecation + Migration (1 day)

1. Add deprecation diagnostic for old effect properties
2. Write migration examples in docs
3. Mark properties as deprecated in `PROPERTY_REGISTRY` (new flag `DEPRECATED`)
4. Update GUI inspector to hide deprecated properties

**Total: ~9 days**

---

## Open Questions

1. **HDR support?** Current pipeline is SDR (`Rgba8Unorm`). Blur + color matrix can band in gradients. Consider `Rgba16Float` offscreen textures for Phase 2.

2. **Filter on non-rectangular bounds?** Vello renders to rectangular textures. A `Filter` with rotated children will produce a rectangular offscreen with transparent corners. Acceptable for Phase 1.

3. **Backdrop-filter (blur what's behind)?** Deferred. Requires reading the already-rendered framebuffer. Vello's scene encoding makes this complex. The `Filter` container (blur what's inside) covers 90% of use cases.

4. **Custom shader injection?** Explicitly rejected. Keeps renderer deterministic, cacheable, and safe.

---

## Acceptance Criteria

- [ ] `Filter` primitive parses, builds timeline, and renders in GUI preview
- [ ] `blur`, `brightness`, `contrast`, `saturate`, `hue_rotate`, `sepia` are all animatable via keyframes
- [ ] The original use case (blurred background image + sharp foreground) works with a single `.amx` file and no external pre-processing
- [ ] Old effect properties (`shadow_blur`, `glow_radius`, `backdrop_blur`) emit deprecation diagnostics
- [ ] CLI export (`animatix image`, `animatix gif`, `animatix video`) supports `Filter`
- [ ] No measurable frame time regression for scenes without `Filter`
- [ ] Documentation (`spec.md`, `properties.md`, `architecture.md`) updated
