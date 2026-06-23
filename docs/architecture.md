# Animatix Architecture

## Overview

Animatix is a layout-first animation system with three core components:

1. **Parser** (Chumsky-based) — Converts `.amx` source into an AST
2. **Timeline** — Compiles AST into animated property tracks
3. **Composition** — Orchestrates multi-scene timelines with transitions
4. **Renderers** (Vello/WGPU, PPM, Frame sequences) — Rasterizes evaluated scenes

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

### Type Checking

After parsing and module expansion, the gradual type checker validates component instantiation properties and action invocation arguments against parameter type annotations. Unannotated parameters accept any value. The checker produces `DiagnosticCode::TypeMismatch` errors that flow into both CLI and LSP diagnostics.

### Timeline Compilation

`Timeline::build_with_diagnostics(...)` is the main compilation entry for single-scene files. For multi-scene files, `Composition::build(...)` orchestrates per-scene timelines.

### Composition (Multi-Scene)

The `Composition` type in `composition.rs` manages multiple scenes:

- **Build**: Extracts scenes from AST, builds per-scene `Timeline` instances, resolves `play` edges
- **Time mapping**: `Composition::evaluate(global_time_s)` → `(scene_name, local_time_s, transition_blend)`
- **Edge resolution**: Follows explicit `play` edges; falls back to declaration order
- **Cycle detection**: Reports diagnostics on `play` edge cycles
- **Routing**: `BuildTarget` enum automatically detects single vs multi-scene and dispatches

See [§16 Multi-Scene Composition](#16-multi-scene-composition) below.

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
- **Geometry tier**: `position`, `motion_offset`, `size`, `layout_size`, `rotation`, `scale`, `transform`, `placement_mode`, `position_binding`
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

- **Row/Col**: Taffy-backed linear layout with `gap`, `padding`, and cross-axis `align`
- **Grid**: Taffy-backed grid with explicit `cols`, `gap`, and `padding`
- **Stack**: Special-cased; all admitted children share the same origin (supports `padding`)
- **Group**: Scene-graph grouping only; no layout algorithm

### Layout Measurement

Layout consumes a dedicated `layout_size` track per child:
- Shapes seed from authored geometry
- Text/Math/Code seed from measured glyph bounds
- Image seeds from intrinsic or authored size

Children without seeded `layout_size` are excluded from layout admission. Legacy `size` still exists for rendering compatibility.

**Warning:** Children of layout containers with explicit `at`/`position` emit `AbsolutePositionOnLayoutManagedChild` warnings at build time. The `transform` property is the correct mechanism for visual offsets inside managed layouts — it applies a local affine transform without removing the child from the layout flow.

### Dynamic Layout

When `config { dynamic_layout: true }` is enabled, admitted children are re-sampled from `layout_size` per frame and positions are recomputed. Membership remains static (build-time admission only); `gap`, `padding`, `align`, `cols` do not animate.

### GUI Reorder Interaction

The GUI supports canvas drag-to-reorder for layout-managed children:
1. **Drag start** on a layout-managed child enters `Reorder` mode instead of `Move`
2. **Mouse tracking** projects the cursor onto the container's main axis and computes an insertion index against sibling center positions
3. **Visual feedback** shows a ghost at the original position and an accent-blue drop line at the insertion point
4. **Drop** emits a `child_order` property edit targeting the container; the edit pipeline updates `ContainerMetadata` and persists the new order to source via AST mutation
5. **Inspector** also exposes up/down arrow buttons for each child when a container is selected

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

Shipped morph modifiers: `strategy: auto|match|fade`, `path_arc`, `stretch`.

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

### Post-Processing (Filter)

`Filter` is a **container primitive** that renders its children to an offscreen texture, applies post-processing filters, and composites the result back into the parent scene.

```animatix
bg: Filter, blur: 40, brightness: 0.5 {
  img: Image, url: "photo.jpg", size: fill
}
```

**Filter properties** (all `f32`, animatable):

| Property | Default | Description |
|----------|---------|-------------|
| `blur` | 0 | Gaussian blur radius in px |
| `brightness` | 1.0 | Multiplier on all channels |
| `contrast` | 1.0 | Contrast curve offset |
| `saturate` | 1.0 | 0 = grayscale, 1 = unchanged |
| `hue_rotate` | 0 | Hue rotation in degrees |
| `sepia` | 0 | Sepia intensity (0–1) |

Pipeline order: **blur → color matrix → opacity**. Nested filters are allowed but each level adds one offscreen pass.

#### Current CPU-Based Pipeline

```
Evaluate children → vello::Scene (sub-scene)
  ↓
GpuFilterBackend::render_scene_to_image()
  → GPU render to temporary texture
  → texture → CPU RGBA buffer (readback)
  ↓
apply_cpu_filters() (image crate)
  → Gaussian blur (imageops::blur)
  → 4×4 color matrix (brightness, contrast, saturate, hue-rotate, sepia)
  ↓
peniko::ImageData → drawn into parent scene at local transform
```

**Key design decisions:**
- **Unified backend** — `GpuFilterBackend` lives in the core crate and is used by both `PreviewSurface` (GUI) and `OffscreenRenderer` (CLI export). This guarantees pixel-identical output.
- **Renderer-agnostic timeline** — `scene_eval.rs` checks `Timeline::filter_backend` (a `RefCell<Option<Box<dyn FilterBackend>>>`). If no backend is installed, `Filter` falls back to rendering children directly (no filtering).
- **Identity fast-path** — If all filter properties are at identity (`blur == 0`, `brightness == 1.0`, etc.), the sub-scene is appended directly without any offscreen pass.
- **Nested filters** — Each nesting level triggers its own offscreen pass. Expensive but explicit.

#### File Layout

| File | Role |
|------|------|
| `primitives/filter.rs` | `FilterPrimitive` definition (container, icon, dispatch) |
| `timeline/filter.rs` | `FilterBackend` trait + `apply_cpu_filters()` |
| `renderer/filter_backend.rs` | `GpuFilterBackend` — GPU render + CPU readback |
| `timeline/scene_eval.rs` | `render_node_children()` detects `ActorKindId::Filter`, builds sub-scene, samples properties, dispatches backend |
| `timeline/track.rs` | `AnimationTrack` holds `filter_blur`, `filter_brightness`, etc. property tracks |
| `timeline/property_registry.rs` | Registers filter properties in `PROPERTY_REGISTRY` |

#### Performance Notes

| Scenario | Current (CPU) | Target (GPU) | Notes |
|----------|---------------|--------------|-------|
| 1 Filter, 1080p, blur=20 | ~15 ms | ~0.5 ms | Readback dominates |
| 3 nested Filters, 1080p | ~45 ms | ~1.5 ms | Three readbacks |
| No filters (identity) | 0 ms | 0 ms | Fast-path skips all work |
| Export 300 frames, 1 filter | ~4.5 s | ~150 ms | Parallel rendering benefits |

Memory: Each `GpuFilterBackend` owns one temporary texture pair. A 4K RGBA8 texture is ~33 MB. Two ping-pong textures = ~66 MB per backend instance. The backend is created per evaluation call (for `OffscreenRenderer`) or shared (for `PreviewSurface`), so peak memory is bounded.

#### GPU Shader Filter Pass (Phase 8.6)

The CPU pipeline does a full GPU→CPU readback per filter actor, then runs `image` crate operations on the host. For scenes with multiple filters or large resolutions, this is a bottleneck:

- **Readback latency** — `copy_texture_to_buffer` + `map_async` stalls the GPU queue.
- **CPU blur cost** — `imageops::blur` is O(σ²·wh) and single-threaded per call.
- **Color matrix cost** — A full pixel loop in Rust is ~1–5 ms for 1080p.

**Target:** **10–50× speedup** by keeping the entire filter chain on the GPU.

##### Target Pipeline

```
Evaluate children → vello::Scene (sub-scene)
  ↓
GpuFilterBackend::render_scene_to_texture()
  → GPU render to temporary texture A (no readback)
  ↓
WGSL compute shader chain (ping-pong between A ↔ B)
  → blur horizontal pass  (texture A → texture B)
  → blur vertical pass    (texture B → texture A)
  → color matrix pass     (texture A → texture B)
  ↓
Draw final texture directly into parent vello::Scene as image
```

**Critical change:** No CPU readback until the final export encoder needs it.

##### Shader Design

**Blur (Separable Gaussian)** — two 1D compute passes:

```wgsl
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;
@group(0) @binding(2) var dst: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(3) var<uniform> params: BlurParams; // radius, direction

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    // Sample line of pixels along blur direction, weight by Gaussian kernel
    // Write to dst
}
```

- Kernel size = `ceil(radius * 3) * 2 + 1` (3σ coverage).
- For `radius == 0`, skip the pass entirely.
- Use `textureSampleLevel` with bilinear weights to reduce taps.

**Color Matrix** — single full-screen compute pass:

```wgsl
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<uniform> mat: ColorMatrix;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let texel = textureLoad(src, gid.xy, 0);
    let rgba = mat * vec4<f32>(texel.rgb, 1.0);
    textureStore(dst, gid.xy, vec4<f32>(rgba.rgb, texel.a));
}
```

The 4×4 matrix is pre-multiplied on the CPU from individual transforms (same math as `apply_color_matrix` in `timeline/filter.rs`).

##### Implementation Status

| Phase | Scope | Readback? | Status |
|-------|-------|-----------|--------|
| **8.6a** | GPU compute filters, still readback to `peniko::ImageData` | Yes (once per filter) | ✅ Implemented |
| **8.6b** | Zero-readback composite via custom fullscreen pass | No | ✅ Implemented |

**8.6a** removes the CPU blur/color matrix cost. The WGSL shaders (`filter_blur.wgsl`, `filter_color_matrix.wgsl`) are embedded in `renderer/filter_backend.rs` as inline compute pipelines. `GpuFilterBackend::render_scene_to_image_gpu_filtered()` runs the full GPU pipeline (render → blur H → blur V → color matrix → readback) and is called from `scene_eval.rs` for every `Filter` actor.

**8.6b** eliminates the final CPU readback by storing filtered GPU textures as `PendingComposite` entries on the `GpuFilterBackend`. After the main Vello scene is rendered to the output texture, each pending composite is blitted on top via `FullscreenBlitPipeline` with alpha blending. This avoids the GPU→CPU→GPU round-trip entirely.

The zero-readback path activates only when the filter actor is the last child in every ancestor container (safe Z-ordering). For filters that aren't last-in-render-order, the existing readback path is used as a fallback. This is determined by `scene_eval.rs::can_post_composite_filter()`.

**Implementation details:** See `renderer/filter_backend.rs` for the actual `GpuFilterBackend` struct and compute pipeline setup. The trait method `FilterBackend::render_scene_to_image_gpu_filtered()` has a default implementation that falls back to CPU filtering; `GpuFilterBackend` overrides it with the GPU path. `scene_eval.rs` calls the GPU method unconditionally when a `Filter` actor needs processing — non-GPU backends automatically fall back to the CPU path.

**The Vello texture problem:** Vello's `Scene::draw_image` requires CPU-owned `peniko::ImageData`. To bypass this, the zero-readback path uses `FullscreenBlitPipeline` — a custom fullscreen render pass in `RendererCore` that samples the filtered GPU texture directly, bypassing Vello's scene encoding for the composite step. This avoids the GPU→CPU→GPU round-trip entirely for filters that are last-in-render-order.

##### Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| WGSL math differs from Rust color matrix | Visual regression | Unit-test matrix equivalence; allow ±1 tolerance |
| Large blur radius causes timeout | Crash / TDR | Cap `kernel_radius` at 128; fallback to CPU for extreme radii |
| Storage texture not supported on old adapter | Pipeline creation fail | Detect at init, fallback to CPU path |
| Ping-pong texture memory pressure | OOM on 4K scenes | Allocate at first use, not at init; reuse across frames |
| Vello API changes break texture binding | Compile fail | Pin Vello rev; monitor upstream |

##### Open Questions

1. **Vello texture binding** — Vello's `Scene` does not natively support binding external GPU textures as image brushes. The GPU path may need a custom composite step outside of `vello::Scene` encoding, or upstream changes to Vello/peniko.
2. **HDR / wide-gamut** — Current pipeline is `Rgba8Unorm`. Should the filter intermediate use `Rgba16Float` for higher precision color matrix math?
3. **Dynamic resolution** — Filter textures are allocated at scene resolution. Should they be cropped to the filter actor's bounding box for large scenes with small filters?

---

## 7. Expression Evaluation

Expressions are evaluated via `evaluate_expr` using an `Environment` (`Rc<RefCell<HashMap<String, Value>>>`).

Key expression variants for compound values:
- `Expr::Tuple` — Tuple/vector literal `(x, y)`. Fixed-size: Vec2, Vec4.
- `Expr::List` — List literal `{a, b, c}`. Variadic array, always inferred as `List<T>`.

Built-ins: `sin`, `cos`, `lerp`, `rand`, `format`. Closures use arrow syntax `(x) => x^2`.

The plotting system (`PlotCurve` with `kind: cartesian|polar|parametric|implicit`) samples closure `func` at discrete points with adaptive refinement.

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
    field: ActorField,          // Which storage tier to WRITE (build-time)
    group: Option<GroupMembership>, // For compound cross-property resolution
    read_source: ReadSource,    // How to READ (frame-time env injection)
}
```

The `PROPERTY_REGISTRY` is a static sorted slice. Lookup is O(log n) binary search.

### ReadSource — separating write from read

A property's `field` says where to write at BUILD time (parsing, keyframing).
`read_source` says where to read at FRAME time (env injection for `always` blocks,
`_animating_*` flags). Most properties use the same storage for both, but some differ:

| Variant | Meaning | Example |
|---------|---------|---------|
| `Field(f)` | Read from same field as write target | `rotation`, `opacity` |
| `Alias(f)` | Write target is a group handler; read from `f` | `at` → `Position` |
| `Component { field, index, scale }` | Extract scalar from Vec2 field | `width` = `Size.x × 2` |
| `None_` | Not readable at frame time | `anchor`, `offset` |

### Frame-time env injection

Every frame, `inject_property_into_env()` iterates the registry and injects every
`INJECTABLE` property into the evaluation environment (`{label}.{name}`) along
with its `_animating_{name}` flag (see §5 Reactive). The read_source dispatches
between direct field reads, aliases, and component extraction — no special cases.

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
- **ContainerLayout**: `gap`, `padding`, `align`, `cols` → container metadata

---

## 10. Non-Interpolatable Property Transitions

### The Problem

Most animatable properties in Animatix (f32, Vec2, Color, etc.) implement the
`Interpolate` trait, which allows `PropertyTrack<T>` to compute in-between
values automatically between keyframes. However, some property types cannot
meaningfully implement `Interpolate`:

- **Closures / function bodies** — `(x) => sin(x * freq)` — there is no
  meaningful way to "lerp" two AST expression trees.
- **Arbitrary AST nodes** — structural rather than numeric values.
- **External resource handles** — image URLs, file paths that require loading.

### The Solution: Side-Channel Pattern

Instead of forcing these types into the `Interpolate` model, Animatix uses a
**parallel side-channel** for transitions. The key idea is to store transition
metadata (time range, easing, from/to values) in a separate `Vec<YourTransition>`
field on `AnimationTrack`, completely outside the standard `PropertyTrack<T>`
keyframe system.

At frame evaluation time, the evaluation code checks for active transitions,
evaluates both the `from` and `to` sources independently, and blends their
*outputs* by the eased progress value — rather than interpolating the sources
themselves.

### Example: `func` Transitions

The `func` property on `PlotCurve` is the primary example of this pattern:

```rust
// On AnimationTrack (dispatch.rs):
pub func_transitions: Vec<FuncTransition>,
```

```rust
// FuncTransition (plot.rs):
pub struct FuncTransition {
    pub start_ms: u64,
    pub end_ms: u64,
    pub easing: Easing,
    pub from: FuncSource,
    pub to: FuncSource,
}
```

At render time, [`sample_procedural_plot_at`](../crates/animatix/src/timeline/plot.rs):
1. Finds the active transition via `active_at(time_ms)`.
2. Constructs a `PlotFuncRef::Blended { from, to, progress }`.
3. Evaluates both functions at each sample point and lerps: `from + (to - from) * progress`.

### When to Use This Pattern

| Approach | When to Use |
|----------|-------------|
| **Standard `PropertyTrack<T>`** | Type implements `Interpolate` (all numeric types, strings, colors, etc.) |
| **Side-channel transitions** | Type cannot implement `Interpolate` (closures, AST nodes, resource handles) |

### Implementation Checklist

To add transitions for a new non-interpolatable property type:

1. Define a transition struct with `start_ms`, `end_ms`, `easing`, `from`, `to`.
2. Define a source enum with variants for raw values and mid-transition blends.
3. Add a `Vec<YourTransition>` field to `AnimationTrack`.
4. Include transition end times in `max_keyframe_time()`.
5. Include non-empty transitions in `has_any_keyframes()`.
6. At frame evaluation, find the active transition and blend the *outputs*
   of `from` and `to` by eased progress.

---

## 11. Colorscheme System

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

## 12. Source Write-Back (GUI)

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
- `source_edit/` — semantic edit API (`SetProperty`, `InsertProperty`, `InsertKeyframe`, `InsertAction`, `InsertActor`)

**Benefits over old byte-span surgery:** no span invalidation, no re-parsing, robust property aliasing.

**Trade-offs:** formatting is normalized; inline comments after properties are preserved via `Property.trailing_comment`, but blank lines and indentation style are not.

For the full formatting rules, see [`spec.md`](spec.md) §Appendix A: Source Formatting Specification.

### Insertion Mechanism (Phase 8.5)

The GUI provides a unified insertion palette (`/` key) for inserting primitives, actions, and snippets. All insertions go through `SourceEdit` → AST mutation → re-serialization (no raw text surgery).

**Three layers:**
1. **`SourceEdit`** — semantic edit types (`InsertActor`, `InsertAction`)
2. **`InsertionRequest`** — bridge between palette UI and `SourceEdit`
3. **`InsertionPalette`** — fuzzy-searchable overlay populated from `PRIMITIVES`, action registry, and analyzer snippets

**Key design properties:**
- **Auto-extensible** — adding a primitive to `PRIMITIVES` or action to the registry automatically surfaces it in the palette
- **Context-aware** — palette defaults to actions in keyframe cells, primitives in code cells
- **Timeline-safe** — existing keyframes' absolute times never shift; new keyframes inherit the preceding style (relative/absolute)

**Six insertion rules:**
1. Exact time, never nearest — create a keyframe if none exists at the target time
2. Cursor-in-cell wins over playhead
3. Style inheritance — new keyframes match the preceding keyframe's style
4. Absolute times are sacred — existing events keep their absolute times
5. No micro-fragmentation — within 50ms of existing keyframe, append instead
6. Visual confirmation — status bar explains what happened

### SourceEdit Design Gaps

Some GUI operations bypass `SourceEdit` and directly mutate `raw_statements`. These are structural edits (insert/remove/rearrange multiple statements) that have no corresponding `SourceEdit` variant. Each duplicates the commit-source boilerplate (`stmts_to_source` + `replace_text` + `is_dirty` + `source_index`).

| Operation | Handler | Bypasses `SourceEdit`? | Notes |
|-----------|---------|----------------------|-------|
| Delete actor | `document_controller::handle_delete_selected_actors` | No — uses `SourceEdit::DeleteActor` | Fixed 2026-06-05 |
| Duplicate actor | `document_controller::handle_duplicate_actor` | Yes — direct `stmts.insert` | No `DuplicateActor` variant |
| Paste actors | `document_controller::paste_actors` | Yes — direct `stmts.insert` + keyframe clone/rename/shift | No `PasteActors` variant |
| Ungroup | `handlers/actor::handle_ungroup_selected_actors` | No — uses `Reparent` + `DeleteActor` | Fixed 2026-06-05 |
| Reorder scenes | `handlers/scene::handle_reorder_scenes` | No — uses `SourceEdit::ReorderScenes` | Fixed 2026-06-05 |

**Design decision:** Keep `SourceEdit` for surgical edits (property, keyframe, single-actor operations). Structural operations (delete, duplicate, paste, ungroup) stay as direct mutations with a shared commit helper. Forcing them into `SourceEdit` variants would make the enum unwieldy and leak GUI concerns (clipboard, label uniqueness) into the edit layer. Revisit if edit serialization or scripting support is needed.

---

## 13. Module & Component System

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

## 14. Analyzer & LSP

Language intelligence is shared via `animatix-analyzer`:

```
animatix-syntax (parser, AST, diagnostics)
    ↓
animatix-analyzer (SymbolTable, Completer, Diagnostics, Hover, Definitions)
    ↓
animatix-gui (direct calls)    animatix-lsp (tower-lsp, JSON-RPC)
```

- **No I/O** in analyzer — pure computation on `&str` or `&[Stmt]`
- **Dual parsers**: chumsky for semantic AST, tree-sitter for position-based queries
- **LSP capabilities**: completion, hover, goto-definition, document symbols, diagnostics
- **Clean boundary**: `animatix-analyzer` depends only on `animatix-syntax`, not the full runtime engine

---

## 15. Primitive Architecture

Adding a new primitive requires **3 touch points** via the `Primitive` trait:

```rust
// primitives/triangle.rs
pub struct TrianglePrimitive;
pub const TRIANGLE: TrianglePrimitive = TrianglePrimitive;

impl Primitive for TrianglePrimitive {
    fn type_name(&self) -> &'static str { "Triangle" }
    fn category(&self) -> ActorCategory { ActorCategory::Shape }
    fn is_shape(&self) -> bool { true }

    fn build(&self, ctx: &mut BuildCtx, label: &str, props: &[Property],
             modifiers: &[Modifier], children: &[InlineItem]) -> Result<(), Vec<Diagnostic>> {
        ctx.timeline.process_inline_actor_decl(self.type_name(), label, props,
                                               modifiers, ctx.time_ms, ctx.parent_label);
        Ok(())
    }

    fn render(&self, ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
        let path = build_triangle_path(ctx.state.size);
        Some(vec![build_vello_path(path, ctx.style)])
    }

    fn default_props(&self, scene: &SceneDimensions) -> Vec<Property> { vec![...] }
}
```

Steps:
1. Create `primitives/<name>.rs` implementing `Primitive`.
2. Add `&name::CONST` to the `PRIMITIVES` array in `primitives/mod.rs`.
3. Add variants to `ActorKindId` / `ShapeKind` enums in `timeline/track.rs` (still needed for match arms).

Registry, dispatch, icon mapping, and GUI defaults are auto-generated from `PRIMITIVES`.

### When to group primitives

Not every visual variation needs its own primitive. The rule of thumb:

- **Same property schema + same rendering path + only internal sampling logic differs** → use a single primitive with a `kind` property.
- **Different property schema or fundamentally different rendering** → separate primitive.

**Example — plot curves:** The former `CartesianPlot`, `PolarPlot`, `ParametricPlot`, and `ImplicitPlot` primitives all exposed `func`, `x_domain`, `y_domain`, `t_domain`, `tolerance`, `max_depth`, and `resolution`. They differed only in how the closure was sampled. These were merged into `PlotCurve` with a `kind` property. This keeps `ActorKindId` lean and avoids `PROPERTY_REGISTRY` bloat.

**Counter-example — `VectorField`:** It exposes `func` that returns a 2-D vector, plus `density` / `grid_size`, and renders arrows rather than a single stroke path. It stays as a separate primitive.

**Counter-example — `NumberPlane`:** NumberPlane is a standalone coordinate system that auto-generates axes, grid lines, and tick marks. Unlike `Graph`, it does not host child plots. `Graph` is a coordinate container for hosting child actors (`PlotCurve`, etc.) with optional grid/ticks.

---

## 16. Multi-Scene Composition

Core concepts:
- `# SceneName` declares a scene; `play SceneName [transition, duration]` declares edges.
- `Composition::build()` creates per-scene `Timeline` instances, resolves `play` edges, detects cycles, and warns on orphan scenes and multiple play targets.
- `Composition::evaluate(global_time_s)` maps global time → `(scene_name, local_time_s, transition_blend)` with eased progress.
- `BuildTarget` auto-routes single-scene vs multi-scene for CLI/GUI.

**Config merge:** The shared prelude (imports, `pub let`, top-level `config`) is prepended to every scene body before timeline compilation. Scene-scoped config keys (`colorscheme`, `dynamic_layout`) override the prelude; composition-scoped keys (`resolution`, `strict_types`) are ignored with a warning. See [`spec.md`](spec.md) §Config Merge Semantics for the full key-scope table.

**Diagnostics:**
- `DuplicateSceneName` — error on repeated scene names
- `PlayTargetNotFound` — error when `play` references a missing scene
- `PlayCycleDetected` — error when play edges form a cycle
- `MultiplePlayTargets` — error when a scene has >1 `play` statement (first wins)
- `OrphanScene` — warning when a scene is not the target of any `play` edge
- `InvalidConfigValue` — warning when a scene config sets a composition-scoped key

**GUI:** The sidebar scene list supports drag-to-reorder, context menus (duplicate, delete, set active), and per-scene inspector (duration, start time, background color, transition target/type/duration/easing). `SourceEdit` variants cover: `ReorderScenes`, `SetPlayTarget`, `SetTransition`, `SetSceneDuration`, `RenameScene`, `AddScene`, `DeleteScene`, `DuplicateScene`, `ExtractScene`, `MoveToScene`.

---

## 16.1 Scene Persistence Architecture

Scene persistence uses a **build-time carry bag** mechanism to transport actors across scene transitions.

### Carry Bag

A `CarryBag` (`timeline/persistence.rs`) is a collection of `CarryEntry` objects, each containing:
- A snapshot of the actor's `AnimationTrack` at the scene's end time (all keyframes collapsed to t=0)
- Recursive snapshots of child actors (for containers)
- The persistence flag (sticky — propagates automatically until `remove`)
- Optional auto-color slot index (for `color: auto` actors)

### Build Process

1. **Parse scenes**: Extract scene declarations and play edges.
2. **Compute walk order**: Topological sort of scenes via play edges (with cycle detection).
3. **First-pass build**: Each scene is compiled without carry injection.
4. **Walk-order carry loop** (`Composition::build`, step 3.5): For each scene (index ≥ 1):
   - Compute carry bag from predecessor timeline at its exit time.
   - If bag is non-empty, rebuild the scene's timeline with `build_with_carry`, which calls `inject_carry_bag` before processing statements.
5. **Carry injection** (`inject_carry_bag`): Inserts carried tracks into `timeline.tracks`, adds to `root_nodes`, seeds `persistence_flags`, propagates `container_metadata`, and restores `auto_color_assignments`.

### Snapshot Semantics

`snapshot_track_at(track, time_ms)` collapses all keyframes of each property track to a single t=0 keyframe holding the sampled value at `time_ms`. Non-animated metadata (`kind`, `procedural_plot`, `svg_paths`, `text_paths`, `image`) is preserved by clone. `func_transitions` are cleared (they represent live animation transitions, not static state).

### Layout Re-rooting

When a layout-managed child is carried, its position binding is rewritten to `Absolute` using the world-space position computed from the source scene's layout engine via `actor_world_affine`. This decouples the carried actor from the source container so it renders correctly in the destination scene without an active layout pass.

### Auto-Color Preservation

Actors declared with `color: auto` receive an integer slot in `timeline.auto_color_assignments`. When carried, the slot is stored in `CarryEntry.auto_color_slot` and re-injected into `dest.auto_color_assignments`, ensuring the actor keeps the same auto-cycle color across scenes. The `next_auto_color_index` is bumped to `max(existing, slot + 1)` to prevent slot collisions with newly declared actors in the destination scene.

### Transition Rendering

No changes to the GPU compositor. During a fade transition, the carried actor is present in both the outgoing and incoming scene textures at identical world positions; the blend produces no visual artifact for that actor.

### Diagnostics

- `PersistIgnoresDuration` — `persist` given a duration modifier (ignored)
- `PersistLayoutManagedChild` — persisting a layout-managed leaf directly
- `PersistTargetNotCarried` — persist in last scene or single-scene file
- `CarryAmbiguousPredecessor` — scene has multiple predecessors (diamond topology)
- `PersistAfterRemove` — `persist` follows `remove` for the same actor in the same scene

---

## 17. File Structure

```
crates/
├── animatix-syntax/       # Syntax layer — parser, AST, module system
│   └── src/
│       ├── ast.rs         # AST types
│       ├── parser/        # Chumsky parser (split into submodules)
│       ├── diagnostics.rs # Diagnostic types
│       ├── easing.rs      # Easing function registry
│       ├── source_index.rs# Source location mapping
│       ├── to_source.rs   # AST re-serialization
│       ├── formatter.rs   # Source formatting
│       ├── transition_registry.rs
│       ├── icon_glyphs.rs
│       ├── typecheck.rs   # Gradual type checker
│       ├── walk.rs        # Shared AST traversal primitives
│       └── module/        # Module system (discovery, expand, rewrite)
│
├── animatix/              # Runtime engine — timeline, renderer, primitives
│   └── src/
│       ├── lib.rs         # Re-exports syntax modules
│       ├── composition.rs # Multi-scene composition engine
│       ├── timeline/      # Timeline compilation, actions, morphing, plotting
│       ├── renderer/      # Vello/WGPU rendering pipeline
│       ├── primitives/    # Actor primitive system
│       ├── ir.rs          # Re-export: timeline modifier runtime IR
│       └── vm.rs          # Re-export: timeline modifier runtime VM
│
├── animatix-analyzer/     # Shared language intelligence (depends on syntax)
├── animatix-lsp/          # LSP server (tower-lsp)
├── animatix-gui/          # Desktop GUI (eframe/egui)
├── animatix-macros/       # Proc macros
└── tree-sitter-animatix/  # Tree-sitter grammar
```

### Shared Walk Layer

The `walk.rs` module in `animatix-syntax` provides shared AST traversal primitives
(`walk_stmts`, `walk_expr`, `walk_inline_items`, etc.). These use a visitor pattern
(`FnMut(&T) -> ()`).

**Not all walk sites can use these primitives.** The following patterns are
incompatible:

- **Value-returning recursion**: Functions that walk and return a value (e.g.,
  `format_expr` returns `String`, `infer_expr_type` returns `PropertyType`)
- **Owned tree transformation**: Functions that take ownership and produce new
  trees (e.g., `inline_custom_actions`)

These sites use guardrail tests (in `format_core.rs` and `apply.rs`) to ensure
variant coverage is reviewed when new AST variants are added.

## 18. Crate Split (Completed 2026-06-02)

`animatix-syntax` was extracted from the core `animatix` crate. `animatix-analyzer` now depends only on `animatix-syntax`, eliminating WGPU/Vello from the LSP compile graph.

### Modules in `animatix-syntax`

`ast`, `parser`, `module/`, `diagnostics`, `easing`, `source_index`, `to_source`, `transition_registry`, `icon_glyphs`

### Modules That Stay in `animatix`

`timeline/`, `composition`, `renderer/`, `primitives/`, `ir` (re-export), `vm` (re-export)

### Dependency Changes

| Crate | Before | After |
|-------|--------|-------|
| `animatix-syntax` | — | `chumsky`, `tracing` |
| `animatix` | `chumsky` + 20+ deps | `animatix-syntax` + runtime deps |
| `animatix-analyzer` | `animatix`, `chumsky` | `animatix-syntax` only |
| `animatix-gui` | `animatix` | `animatix` + `animatix-syntax` |

---

*For language details, see [`spec.md`](spec.md). For work items, see [`roadmap.md`](roadmap.md).*
