# Animatix Architecture

## Overview

Animatix is a layout-first animation system with four core components:

1. **Parser** (Chumsky parser over a single lossless tokenizer) — Converts `.amx` source into an AST
2. **Timeline** — Compiles AST into animated property tracks
3. **Composition** — Orchestrates multi-scene timelines with transitions
4. **Renderers** (Vello/WGPU, PNG, Frame sequences) — Rasterizes evaluated scenes

---

## 1. File Processing Pipeline

### Source ↔ AST

```
.amx File → Chumsky semantic parser
         ↓
     AST (Expr, Stmt hierarchy)
         ↓
     to_source::stmts_to_source()  (re-serialization for GUI write-back)
```

The AST is round-trippable. The GUI inspector mutates the AST directly and re-serializes the entire file. Formatting is normalized during re-serialization. `parser::parse_canonical` is the semantic parser entry point used by module loading and the runtime. The analyzer tokenizes source and walks the same token stream plus AST for positions, completions, hover, and references.

### Module System

The `ModuleGraph` manages file dependencies: tracks `import` declarations, resolves relative paths, and collects `pub` exports (components via `pub component`, values via `pub let`).

### Type Checking

After parsing and module expansion, the gradual type checker validates component instantiation properties and action invocation arguments against parameter type annotations. Unannotated parameters accept any value. The checker produces `DiagnosticCode::TypeMismatch` errors that flow into both CLI and LSP diagnostics.

### Timeline Compilation

`Timeline::build_with_diagnostics(...)` is the main compilation entry for single-scene files. For multi-scene files, `Composition::build(...)` orchestrates per-scene timelines.

### Composition (Multi-Scene)

The `Composition` type in `composition/` manages multiple scenes:

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

- **PNG/WebP**: CPU-side RGBA buffer for single frames
- **Video/GIF**: Parallel frame rendering + FFmpeg muxing

### Post-Processing (Filter)

`Filter` is a **compositing scope**: it renders its content children to an
offscreen texture, applies its declared **effect chain** in order, and
composites the result back into the parent scene. Effects are labelled children
of the scope; they are not primitives, and they create no scene-graph node, no
layout entry, and no hit region.

```animatix
bg: Filter {
  soft: Blur, radius: 40
  warm: ColorGrade, contrast: 1.15, saturate: 0.9
  img: Image, url: "photo.jpg", size: fill
}
#1.5s
bg.soft.radius = 24 [1.5s, ease: ease-out]
```

The normative contract — pass layout, bind group, uniform rules, identity
semantics, ROI, failure policy, and plugin authoring — lives in
`docs/effects.md`. The author-facing surface is in `docs/spec.md`.

#### Effect catalog

Built-in effects are unit structs implementing the `Effect` trait (parameter
schema, WGSL passes, `pack`, and `support` in one file each), registered once in
the `EFFECTS` bootstrap array. Plugin effects wrap FFI-declared data
(`PluginEffectData`) in the same trait, so the renderer only ever sees
`&dyn Effect`.

| File | Role |
|------|------|
| `primitives/filter.rs` | `FilterPrimitive` — the container that owns a chain |
| `timeline/effects/mod.rs` | `Effect` trait, `EFFECTS` array, `EffectId`, parameter schema, dispatch (`effect` / `effect_for_type`), plugin registry |
| `timeline/effects/blur.rs` | `Blur` — separable Gaussian, two passes |
| `timeline/effects/color_grade.rs` | `ColorGrade` — 4×4 colour matrix composed on the host |
| `timeline/effects/chromatic_aberration.rs` | `ChromaticAberration` — sub-pixel channel split via the linear sampler |
| `timeline/effects/track.rs` | `EffectChainTrack` / `EffectStage` — animatable storage (`DynTrack`-backed parameters) |
| `timeline/effects/chain.rs` | `EffectChain`, `EffectRegion`, `PendingComposite`, `FilterBackend` |
| `timeline/build/effect.rs` | Lowering: effect children become stages on the owning scope's chain |
| `renderer/filter_backend.rs` | `GpuFilterBackend` — GPU render, N-pass dispatch, readback / zero-readback composite |
| `timeline/scene_eval.rs` | Renders the scope's content to a sub-scene, samples the chain, derives the ROI, dispatches the backend |

Adding a built-in effect is one new file plus one `EFFECTS` entry, an `EffectId`
variant, and an `effect_specs()` row (the analyzer's table lives in a crate the
runtime cannot depend on). There is no per-effect dispatch table to keep in
sync, and no core match arm to touch.

#### Pipeline

```
Evaluate content children → vello::Scene (sub-scene)
  ↓
GpuFilterBackend::render_scene_to_image_gpu_filtered()  (or …_to_pending_composite)
  → GPU render the sub-scene to texture A
  → per effect, per declared pass, ping-pong A ↔ B
    (one submitted encoder per pass — probe 009 synchronisation)
  → readback, or park a PendingComposite for a viewport-scoped blit
  ↓
Composite at the region origin (readback path) or blit (zero-readback path)
```

**Key properties:**

- **On-demand.** A stage whose `enabled` is false, or whose parameters all equal
  their identity values, is dropped when the chain is sampled. If every stage is
  dropped, the offscreen round-trip is skipped and the content children are
  appended directly.
- **Region of interest.** The processed region is the content bounds recorded
  during sub-scene evaluation, padded by the chain's worst-case spatial support
  and clamped to the scene — or an authored `bounds: (x, y, w, h)`. GPU textures
  stay at full scene capacity, so a region that grows or shrinks between frames
  never reallocates.
- **Renderer-agnostic timeline.** `scene_eval` receives a
  `&mut Option<&mut dyn FilterBackend>`. With no backend installed, the children
  render unfiltered and a `RenderFailure` diagnostic is recorded; there is no CPU
  fallback, because arbitrary WGSL has no host path.
- **Nested scopes** are allowed; each level adds one offscreen round-trip.
- **Zero-readback** requires the scope to be the last rendered element
  (`can_post_composite_filter`); otherwise the readback path composites at the
  region origin.

#### Open questions

1. **HDR / wide-gamut** — the intermediate is `Rgba8Unorm`, so every pass
   quantises to 8 bits (banding on gradients and `Levels`-style effects). Should
   the ping-pong targets move to `Rgba16Float`?
2. **Per-region intersection** — generalising `can_post_composite_filter` from
   "last rendered element" to "no later sibling intersects the ROI" would let
   mid-scene scopes use the zero-readback path too.
3. **Second input texture** — bloom/glow add-back, soft shadows, and a generic
   chain-level `mix` all need the original alongside the processed image. This is
   a deliberate ABI bump when a real effect needs it, not a speculative field
   (`docs/effects.md` §4.1).

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

`ValueType::Sum` models named variants with optional payloads. Parsing selects a variant by literal
or payload type, and `PropertyValue::Variant` stores the selected name plus payload. This lets
semantic properties such as `legend` share generic storage, persistence, animation, and inspector
editors instead of requiring a bespoke core enum and GUI enum per property. `ValueType::Enum` covers
fixed no-payload choices with the same generic editor path.

User-facing aliases (`type LegendMode = Bool | Str`) are parsed into `Stmt::TypeAlias` and resolved
by the type environment before component/action parameter checking, so aliases can compose primitives,
unions, lists, and other aliases.

### ReadSource — separating write from read

A property's `field` says where to write at BUILD time (parsing, keyframing).
`read_source` says where to read at FRAME time (env injection for `always` blocks,
animation-state flags via `env_keys::animating_flag`). Most properties use the same storage for both, but some differ:

| Variant | Meaning | Example |
|---------|---------|---------|
| `Field(f)` | Read from same field as write target | `rotation`, `opacity` |
| `Alias(f)` | Write target is a group handler; read from `f` | `at` → `Position` |
| `Component { field, index, scale }` | Extract scalar from Vec2 field | `width` = `Size.x × 2` |
| `None_` | Not readable at frame time | `anchor`, `offset` |

### Frame-time env injection

Every frame, `inject_property_into_env()` iterates the registry and injects every
`INJECTABLE` property into the evaluation environment (`{label}.{name}`) along
with its animation-state flag (see §5 Reactive). The read_source dispatches
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

The `func` property on plot actors is the primary example of this pattern:

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
    pub blend_mode: FuncBlendMode, // Output (default) or Opacity
}
```

At render time, [`sample_procedural_plot_at`](../crates/animatix/src/timeline/plot.rs):
1. Finds the active transition via `active_at(time_ms)`.
2. Constructs a `PlotFuncRef::Blended { from, to, progress }` for output blending.
3. Evaluates both functions at each sample point and lerps: `from + (to - from) * progress`.
4. For `blend: opacity`, builds the two endpoint path sets and cross-fades their alpha instead.

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
- Aliased module imports: `import "theme.amx" as theme` (see `spec.md` §11)

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

### Insertion Mechanism

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

### SourceEdit Coverage

All GUI structural edits now route through `SourceEdit` → AST mutation → re-serialization. This includes delete, duplicate, paste, ungroup, and scene reordering. The previously direct-mutated `DuplicateActor` and `PasteActors` operations are now `SourceEdit::DuplicateActor` and `SourceEdit::PasteActors` variants implemented in `source_edit/actor_edits.rs`.

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
animatix-syntax (parser, AST, diagnostics, ModuleGraph)
    ↓
animatix-analyzer (SymbolTable, Completer, Diagnostics, Hover, Definitions)
    ↓
animatix-gui (direct calls)    animatix-lsp (tower-lsp, JSON-RPC)
```

- **No I/O** in analyzer — the workspace facade runs `ModuleGraph` in
  `SourceAccess::SourcesOnly` mode, so only explicitly registered in-memory
  sources are resolved.
- **Shared resolved-program model**: `ModuleGraph` owns parsing, symbols,
  imports, namespaces, and source-map identity for both the runtime/module
  pipeline and the analyzer workspace. `Workspace` is a thin facade over that
  graph, so analyzer and runtime resolution cannot drift.
- **Canonical parser API**: Chumsky remains the semantic AST source of truth; the analyzer uses the lossless token stream plus AST for position queries
- **LSP capabilities**: completion, hover, goto-definition, document symbols, diagnostics
- **Clean boundary**: `animatix-analyzer` depends only on `animatix-syntax`, not the full runtime engine

---

## 15. Primitive Architecture

Adding a new built-in primitive requires these touch points (rendering is
trait-dispatched; the remaining steps exist because `ActorKindId` variants and
tooling tables are matched across the codebase and cannot be auto-generated
from the `PRIMITIVES` array):

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
3. Add a variant to `ActorKindId` (and `ShapeKind` for shapes) in `timeline/actor_kind.rs`.
4. Add an entry to `animatix-syntax::schema::builtin_primitive_specs()` and to
   `animatix-syntax::builtins::TYPES` + `type_documentation`.
5. If the primitive declares properties, add them to BOTH
   `animatix-syntax::schema::raw_property_specs()` and the runtime
   `timeline::property_registry::PROPERTY_REGISTRY` (pinned by a sync test).
6. Document in `docs/primitives.md` / `docs/spec.md`; add render/hit-region
   tests if the primitive draws.

The metadata registry (`ActorKindMeta`) is auto-generated from `PRIMITIVES`,
and `registry_specs_match_shared_schema_for_builtins` pins the runtime
metadata to the schema table — so built-ins and tooling cannot silently drift,
but the schema table itself remains hand-maintained.

Registry, dispatch, icon mapping, and GUI defaults are auto-generated from `PRIMITIVES`.

External primitives can avoid step 3 by registering through `PrimitiveRegistry`
or `ExtensionContext`; the timeline records the source `actor_type` and resolves
it back to the runtime primitive during scene evaluation. External properties
can likewise be registered as `ExtensionPropertySpec` values and are stored in
the actor's `PropertyPlan`/`DynTrack` slots.

### When to group primitives

Not every visual variation needs its own primitive. The rule of thumb:

- **Same property schema + same rendering path + only internal sampling logic differs** → use a single primitive with a `kind` property.
- **Different property schema or fundamentally different rendering** → separate primitive.

**Example — plot curves:** The former `CartesianPlot`, `PolarPlot`, `ParametricPlot`, and `ImplicitPlot` primitives all exposed `func`, `x_domain`, `y_domain`, `t_domain`, `tolerance`, `max_depth`, and `resolution`. They differed only in how the closure was sampled. These were merged into `PlotCurve` with a `kind` property. This keeps `ActorKindId` lean and avoids `PROPERTY_REGISTRY` bloat.

**Counter-example — `VectorField`:** It exposes `func` that returns a 2-D vector, plus `density` / `grid_size`, and renders arrows rather than a single stroke path. It stays as a separate primitive.

**Counter-example — `NumberPlane`:** NumberPlane is a standalone coordinate system that auto-generates axes, grid lines, and tick marks. Unlike `Graph`, it does not host child plots. `Graph` is a coordinate container for hosting child actors (`PlotCurve`, etc.) with optional grid/ticks.

### Callout Geometry Helper

Targeted `Callout` actors (those with a `target` property) need to compute the arrow attach point from another actor's bounds. This geometry is centralised in `timeline/callout_geometry.rs`:

- `derive_callout_geometry(input, resolver)` is the single source of truth for `from`, `to`, `label_point`, attach side, and standoff. Both the renderer and the GUI (preview handles, drag gestures) call this helper so they stay in sync.
- The `TargetResolver` trait (implemented by `SceneEval`) exposes only what primitives need: actor existence, kind, and world-space bounds/affine at a given time. Primitives no longer receive an unrestricted `&Timeline` reference.
- Bounds are resolved via the target's accumulated world transform (`actor_world_affine`), so actors nested inside scaled or rotated parents attach correctly. Precise shape/path/text bounds are a known deferred item (see Active Work in `docs/roadmap.md`).
- A missing target name is diagnosed at build time (`CalloutTargetNotFound`) so the error surfaces in the editor rather than producing per-frame render log spam.

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
│       ├── parser/        # Chumsky parser (consumes the token stream)
│       ├── token.rs       # Lossless tokenizer
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
│       ├── composition/   # Multi-scene composition engine (mod, build, time, tests)
│       ├── timeline/      # Timeline compilation, actions, morphing, plotting
│       ├── renderer/      # Vello/WGPU rendering pipeline
│       ├── primitives/    # Actor primitive system
│       └── ir.rs          # Re-export: timeline modifier runtime IR
│
├── animatix-analyzer/     # Shared language intelligence (depends on syntax)
├── animatix-lsp/          # LSP server (tower-lsp)
├── animatix-gui/          # Desktop GUI (eframe/egui)
└── eparts/                # Themed egui widget framework
```

### Shared Walk Layer

The `walk.rs` module in `animatix-syntax` provides shared AST traversal primitives
(`walk_stmts`, `walk_expr`, `walk_inline_items`, etc.). These use a visitor pattern
(`FnMut(&T) -> ()`).

**Not all walk sites can use these primitives.** The following patterns are
incompatible:

- **Value-returning recursion**: Functions that walk and return a value (e.g.,
  `format_expr` returns `String`, `infer_expr_type` returns `typing::Type`)
- **Owned tree transformation**: Functions that take ownership and produce new
  trees (e.g., `inline_custom_actions`)

These sites use guardrail tests (in `format_core.rs` and `apply.rs`) to ensure
variant coverage is reviewed when new AST variants are added.

## 18. Crate Split

`animatix-syntax` was extracted from the core `animatix` crate. `animatix-analyzer` now depends only on `animatix-syntax`, eliminating WGPU/Vello from the LSP compile graph.

### Modules in `animatix-syntax`

`ast`, `parser/`, `module/`, `diagnostics`, `semantic_diagnostics`, `easing`, `source_index`, `source_map`, `to_source`, `formatter`, `transition_registry`, `icon_glyphs`, `token`, `typecheck`, `typing`, `walk`

### Modules That Stay in `animatix`

`timeline/`, `composition`, `renderer/`, `primitives/`, `ir` (re-export)

### Dependency Changes

| Crate | Before | After |
|-------|--------|-------|
| `animatix-syntax` | — | `chumsky`, `tracing` |
| `animatix` | `chumsky` + 20+ deps | `animatix-syntax` + runtime deps |
| `animatix-analyzer` | `animatix`, `chumsky` | `animatix-syntax` only |
| `animatix-gui` | `animatix` | `animatix` + `animatix-syntax` |

---

*For language details, see [`spec.md`](spec.md). For work items, see [`roadmap.md`](roadmap.md).*

## 19. Design Decisions (Current)

### Frontend
- One lossless tokenizer (`crates/animatix-syntax/src/token.rs`) is the only lexical definition. The parser consumes its token stream; the analyzer, LSP, and GUI use the same tokens for positions and highlighting.

### Evaluation and IR
- Closures capture lexical scope at creation: `Value::Closure` carries a `CapturedEnv` that snapshots only the build-time override layer; the shared stdlib base is re-provided at render time.
- Nested plot blends are flattened into a linear weighted sum (`flatten_blend`), evaluating depth-N blends in O(N) per sample.
- `Value::NativeFn` is non-serializable by design; persistence errors on it instead of silently dropping stdlib functions.
- For-loop and index variables are scoped to their loop; closure capture-at-creation keeps that clearing safe.

### Plot and Graph Coordinates
- Graph domain/scale are static `GraphScaleConfig`; `size`/`at`/`padding` are dynamic `GraphGeometry`, so layout-driven graph geometry stays animatable.
- Per-graph `graph.map` / `graph.map_inverse` are `Value::NativeFn` closures that capture static config and read dynamic geometry from the frame environment.
- Func transitions blend function outputs per sample (`lerp(f(x), g(x), p)`) rather than morphing sampled paths; the per-sample cache is keyed per source body.

### Scene Persistence
- `persist` and `remove` reuse `Stmt::Action`, not new AST surface. Persisted state is a build-time carry bag of single-keyframe track snapshots taken at scene end time.
- Persistence is sticky until explicit `remove`; re-declaring a carried actor morphs from its carried state. Carried children re-root to `Absolute`, and colors carry as baked RGBA while preserving the auto-color slot index.

### Text and Charts
- Text content changes should interpolate cached glyph paths with `Fade`; typography changes (font/size/weight/spacing/color) recompile glyphs per frame. Typewriter `draw-in` is preferred for entrances.
- BarChart `data` and `bar_colors` use brace-list `{...}` syntax; build-time-static paths resolve through shared eval helpers.

### eparts Widget Framework
- Runtime theme lives in egui Memory and is read via `eparts::theme(ui)`; it stays `Copy`. Density and reduced-motion are sibling runtime preferences.
- eparts keeps one crate with feature-gated heavy surfaces (`theme-json`, table, charts, webview, i18n); default deps are `egui` + `egui-phosphor`.
- Widgets expose one primary entry point: `impl egui::Widget` (Tier 1) or `pub fn show()` (Tier 2). Cross-frame state lives in `ctx.data` or app-owned structs.
- Colors are only defined in the token layer; component-scoped slot taxonomy has no gradients in the core model. Density scales only spacing, row heights, and control dimensions (Default is identity; Compact is 0.875 rounded once).
- Overlay layering is a managed priority ladder (`Dialog < Popover < Tooltip`); default cursor is arrow, `PointingHand` only for links.

### GUI Shell
- Destructive document-replacement commands route through one Save/Discard/Cancel dialog and replay the follow-up via `pending_action`.
- Keyframe identity is scene-qualified `KeyframeId { scene, actor, property, time_ms }`; panels emit commands rather than mutating shared stores.
- Snap resolution returns corrected coordinates and writes visual feedback as a side effect, keeping geometry resolution separate from preview state.
