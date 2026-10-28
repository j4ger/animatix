# Timeline ECS Architecture

**Status:** complete. Phases 1–7 are tracked in the status table at the bottom
of this file; this document is the normative description of the timeline's
entity/component storage as it now exists.

This document is the normative design for the timeline's entity/component
storage. `docs/architecture.md` §2 is the summary; when the two disagree, this
file wins.

---

## 1. Why

The timeline started as a per-actor monolith (`AnimationTrack`) plus a growing
set of label-keyed side tables on `Timeline`:

```text
Timeline {
    tracks: TrackStore,                  // dense Vec<AnimationTrack>
    container_metadata: BTreeMap<String, ContainerMetadata>,
    child_orders:        BTreeMap<String, PropertyTrack<Vec<String>>>,
    persistence_flags:   BTreeMap<String, bool>,
    auto_color_assignments: BTreeMap<String, usize>,
    transcripts:         HashMap<String, SpeechTranscript>,
    coordinate_frames:   BTreeMap<EntityId, CoordinateFrameComponent>,
    ...
}
```

Two identities existed for every actor (label and `EntityId`), the hierarchy was
stored twice (`children` + `children_ids`), and the frame path could fall back to
string lookups. That makes entity data hard to split into modules, invites drift
between the two representations, and keeps a hash on paths that should be an
array index.

The target is a small, dependency-free ECS: **entities are stable integer ids,
components are typed data stores indexed by id, systems are free functions or
`Timeline` methods that take ids, and labels exist only at the language /
serialization / UI boundary.**

## 2. Model

```text
EntityId(u32) ── entities: EntityStore { label ↔ id maps }
                   │
                   ├── Required stores:
                   │   ├── hierarchy:       RequiredStore<SceneHierarchyComponent>
                   │   ├── actor_types:     RequiredStore<String>
                   │   ├── caps:            RequiredStore<ActorCaps>
                   │   ├── behavior:        RequiredStore<BehaviorId>
                   │   ├── geometry:        RequiredStore<GeometryTracks>
                   │   ├── style:           RequiredStore<StyleTracks>
                   │   ├── property_plan:   RequiredStore<PropertyPlan>
                   │   ├── tagged_tracks:   RequiredStore<TaggedTracksComponent>
                   │   └── memo cells:      shape_memos / pending_shape_bounds /
                   │                        bounds_slots (see §2.1 "memo cells")
                   │
                   └── Optional stores:
                       ├── text:            ComponentStore<TextTracks>
                       ├── shape:           ComponentStore<ShapeTracks>
                       ├── highlight:       ComponentStore<HighlightTracks>
                       ├── plot_payload:    ComponentStore<PlotPayload>
                       ├── svg_payload:     ComponentStore<SvgPayload>
                       ├── image_payload:   ComponentStore<ImagePayload>
                       ├── container_layout:ComponentStore<ContainerMetadata>
                       ├── child_order:     ComponentStore<ChildOrderComponent>
                       ├── coordinate:      ComponentStore<CoordinateFrameComponent>
                       ├── persistence:     ComponentStore<PersistenceComponent>
                       ├── auto_color:      ComponentStore<AutoColorComponent>
                       ├── transcript:      ComponentStore<TranscriptComponent>
                       ├── legend:          ComponentStore<LegendTracks>
                       ├── effects:         ComponentStore<EffectChainTrack>
                       ├── meta:            ComponentStore<EntityMeta>
                       ├── decl_span:       ComponentStore<Span>
                       └── caps_overrides:  ComponentStore<ActorCaps>
```

`World` owns the entity store and the component stores. `Timeline` owns the
`World` plus scene-level resources (environment, colorscheme, camera, assets,
font context, layout engine, `audio_segments`, …) and the frame caches.

### 2.1 Storage contract

| Store | Layout | Use |
|---|---|---|
| `EntityStore` | `HashMap<String, EntityId>` + `Vec<String>` | entity identity (label ↔ id); append-only |
| `RequiredStore<C>` | `Vec<C>` | components every entity has (default-initialised on spawn) |
| `ComponentStore<C>` | `Vec<Option<C>>` | components only some entities have |

An entity's identity *is* its slot index: a label is live exactly while its
index is below `EntityStore::len()`, so there is no dense element to keep in
sync and no free list. All per-entity data lives in `World`'s component stores.
`SceneHierarchyComponent`, `World::roots`, and the build-time
`pending_parents`/`pending_roots` buffers carry the scene graph. `add_node`
records the relation before the primitive creates the child's entity, so the
link is buffered by label and adopted (as an id) by `World::spawn`;
`World::finish_hierarchy()` resolves anything created outside `World::spawn`.
The buffers are empty after a completed build and are not runtime storage.

Rules:

- **Append-only.** The GUI rebuilds a whole `Timeline` for structural deletes,
  so entity slots are never reused and `EntityId.index()` stays valid for the
  life of the timeline. `World::clear()` is for a whole rebuild only. There is
  no free list or generation counter.
- **Aligned stores.** `World::spawn` grows every store to `entities.len()` and
  fills required stores with `C::default()`. `spawn_replacing(label, ...)`
  clears the entity's optional components and resets required components,
  matching the old insert-replacement semantics used by carry injection.
- **Required means present.** An entity that exists has every `RequiredStore`
  entry. Tests pin this (`every_entity_has_required_components`).
- **One source of truth per domain.** Per-entity state exists in exactly one
  component store; a new label-keyed `Timeline` map or a duplicate field is a
  design bug. No dual writes across a merge boundary.
- **Mutation invalidates the frame cache.** Every writable entry point
  (`Timeline::world_mut`, `entity_mut`, `spawn_entity`, component helpers)
  calls `invalidate_frame_cache` first. Build-time writes go through the same
  entry points; there is no cache to invalidate yet, but the invariant is
  uniform.
- **Frame caches are not components — but memo cells live in `World`.**
  Timeline-level caches (frame cache, transform/static-subtree flags,
  precise-bounds table, env-injection plan, layout caches) belong to
  `EvalCaches` and reset on `Clone`. Per-node memo cells (`bounds_slots`,
  `shape_memos`, `pending_shape_bounds` on `World`;
  `vector_paths_epoch`/`shape_path_memo`/`shape_type_switches` on
  `ShapeTracks`) must be reachable from `&World`, because frame-path systems
  write memos through a shared borrow — so they sit next to the data they
  memoize as interior-mutable cells. A component is authored/build data; a
  cache or memo cell is disposable state invalidated by
  `invalidate_frame_cache`. Consequence: `Timeline::clone` copies the memo
  cells with the `World` (unlike `EvalCaches`, which resets). Measured
  callers are cold paths only (composition's per-scene snapshot, GUI
  export/undo targets), so the copy cost is accepted rather than paying a
  hand-written `Clone` that must mirror every future store.

### 2.2 Components

| Component | Data | Presence |
|---|---|---|
| `EntityMeta` | id, label, actor_type, first_seen_ms, visible, locked, camera_follow, parallax, hidden_by_default | optional (reads fall back to `EntityMeta::DEFAULT`) |
| `DeclarationSpan` | `Span` of the actor declaration (absent when the parser had none) | optional |
| `ActorCaps` | category, child_processing, shape, text, flags | every entity (`RequiredStore<ActorCaps>`) |
| `BehaviorId` | 0..36 for built-ins, dynamically assigned for extensions | every entity (`RequiredStore<BehaviorId>`) |
| `SceneHierarchyComponent` | `parent: Option<EntityId>`, `children: Vec<EntityId>` | every entity (`RequiredStore<SceneHierarchyComponent>`) |
| `RootSet` | `Vec<EntityId>` (insertion order) | resource |
| `GeometryTracks` | position/size/layout_size/rotation/scale/transform/placement/callout/connector/volume | every entity (`RequiredStore<GeometryTracks>`) |
| `StyleTracks` | color/opacity/stroke/fill/gradient/blend/morph_options | every entity (`RequiredStore<StyleTracks>`) |
| `ShapeTracks` | shape type, from/to, arc, points, commands, vector paths | optional (`ComponentStore<ShapeTracks>`; falls back to `default_shape`) |
| `TextTracks` | text content, typography, paths, metrics, split reveal | optional (`ComponentStore<TextTracks>`; falls back to `default_text`) |
| `HighlightTracks` | equation highlight color/opacity/padding/radius/blend | optional (`ComponentStore<HighlightTracks>`; falls back to `default_highlight`) |
| `PropertyPlan` | extension/registry property slots | every entity (`RequiredStore<PropertyPlan>`) |
| `TaggedTracksComponent` | tagged-union property tracks by canonical name | every entity (`RequiredStore<TaggedTracksComponent>`) |
| `PlotPayload` | plot curves, surfaces, fields, domains, axis options | optional (`ComponentStore<PlotPayload>`) |
| `SvgPayload` | parsed SVG tree and asset source metadata | optional (`ComponentStore<SvgPayload>`) |
| `ImagePayload` | texture asset handle, dimensions, image source metadata | optional (`ComponentStore<ImagePayload>`) |
| `ContainerMetadata` | layout type/gap/padding/align/cols/child order (`EntityId`s) | optional containers (`ComponentStore<ContainerMetadata>`) |
| `CoordinateFrameComponent` | domain/scale/padding/grid/ticks/tick labels | optional (plot hosts; sole source for hosted-child coordinate resolution) |
| `PersistenceComponent` | `persistent: bool` | optional (`ComponentStore<PersistenceComponent>`) |
| `AutoColorComponent` | auto-color slot | optional (`ComponentStore<AutoColorComponent>`) |
| `ChildOrderComponent` | `PropertyTrack<Vec<EntityId>>` | optional (`ComponentStore<ChildOrderComponent>`) |
| `TranscriptComponent` | speech transcript | optional (`ComponentStore<TranscriptComponent>`) |
| `LegendTracks` | legend entries | optional (`ComponentStore<LegendTracks>`) |
| `EffectChainTrack` | effect stages for Filter/Mask/Glass scopes | optional (`ComponentStore<EffectChainTrack>`) |
| `ActorCaps` (overrides) | explicit capability overrides for extension primitives | optional (`ComponentStore<ActorCaps>`, `caps_overrides`) |

`caps` is stored as `RequiredStore<ActorCaps>` initialized at `spawn`/`set_identity`
from the catalog, making frame-time checks true `Copy` field reads without catalog
scans; `caps_overrides` remains available for extension overrides with `World::set_caps`.
`behavior` is stored as `RequiredStore<BehaviorId>` initialized at `spawn`/`set_identity`,
providing $O(1)$ indexable lookup of `BehaviorDef` in `BehaviorRegistry`.
`TextTracks`, `ShapeTracks`, and `HighlightTracks` are optional component stores populated
only when an entity's behavior family requires them (`ShapeTracks` for vector shapes, `TextTracks`
for text/code/math/equation/captions, `HighlightTracks` for equation actors). Fallback static default
tracks (`default_shape`, `default_text`, `default_highlight`) serve unchanged read accessors when absent.
The former monolithic `PrimitivePayload` enum is split into dedicated optional component stores
(`ComponentStore<PlotPayload>`, `ComponentStore<SvgPayload>`, `ComponentStore<ImagePayload>`),
removing variant wrapping overhead and enabling independent access.
All `EntityMeta` fields are live: `World::actor_type(id)` and `World::label(id)`
are the identity accessors, and `visible`, `locked`, `camera_follow`,
`parallax`, `hidden_by_default` and `first_seen_ms` read through
`World::meta(id)`.

### 2.3 Resources (deliberately not components)

- `Environment` / `env_base` / `env_pool`: the language reads actors by label.
- Modifier `Overrides`: keyed by expression targets, including synthesized
  array labels (`base__n`); it is per-frame evaluation state.
- `audio_segments`: ordered export list consumed by `animatix-render` as
  `&[AudioSegment]`; there is no per-entity lookup.
- `background_color`, `camera`, colorschemes, asset cache, font context,
  layout engine/caches, `variable_tracks`, `action_events`, config/duration,
  modifier programs.
- `referenced_roots` / `wildcard_actors` / `referenced_properties`: the
  build's expression-analysis tables, keyed by label because expressions name
  actors by label; they seed the frame-env injection plan.
- `modifier_written_targets` / `frame_written_vars`: which actor labels and
  variables modifier blocks write — the label set is authored in the source.
- `block_scope`: `let` names bound per `Stmt::Block` expansion.
- Layout admission (`ContainerLayoutChild`) and resolved positions
  (`LayoutPositions`) are keyed by `EntityId` end to end; labels appear only
  at the GUI/action boundary.

Adding a resource instead of a component is a decision to document in the PR:
"looked up by label/order, not by entity id".

### 2.4 Behavior Layer: Family Systems and Extension Adapter

The behavior layer decouples actor identity and lifecycle from monolithic trait objects. Every entity in `World` has a required `BehaviorId` component that maps in $O(1)$ time to an immutable `BehaviorDef` stored in `BehaviorRegistry`.

#### BehaviorId and BehaviorDef
- `BehaviorId(u16)`: A compact, Copy-friendly integer handle. Indices `0..36` are statically reserved for built-in primitives in canonical order; runtime extension primitives receive dynamically allocated indices `>= 36`.
- `BehaviorDef`: Stores the immutable definition of an actor behavior:
  - `id: BehaviorId`
  - `name: Box<str>` (authored type name, e.g. `"Rect"`, `"Text"`, `"Graph"`)
  - `caps: ActorCaps` (derived capability projection)
  - `family: BehaviorFamily` (coarse architectural grouping: `Shape`, `Text`, `Media`, `Plot`, `Container`, `Effect`, `Equation`, `Captions`, `Annotation`, `Group`)
  - `child_processing: ChildProcessingKind` (child evaluation strategy)
  - `source: BehaviorSource` (`Family(FamilyBehavior)` for built-ins, `Extension(Arc<dyn Primitive>)` for plugins)

#### Family Systems (`timeline/behavior/builtin/`)
Built-in primitives are organized into family systems under `timeline/behavior/builtin/`:
1. **`shape`**: Vector shape geometry rendering and state evaluation (`Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`).
2. **`text`**: Typographic rendering and font compilation (`Text`, `Code`, `Math`, `Typst`, `Counter`).
3. **`media`**: Asset and streaming payloads (`Image`, `Svg`, `Audio`).
4. **`plot`**: Mathematical function and coordinate plotting (`Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`).
5. **`container`**: Layout-constrained flex containers (`Row`, `Col`, `Grid`, `Stack`) and structural grouping (`Group`).
6. **`effect`**: GPU shader compositing and offscreen filter scopes (`Filter`, `Glass`, `Mask`).
7. **`equation`**: Math equation layout containers and fragment animations (`Equation`, `Fragment`).
8. **`captions`**: Speech-synchronized subtitles and cues (`Captions`, `Cue`).
9. **`annotation`**: Targeted UI and diagram elements (`Callout`, `Legend`, `Connector`).
10. **`group`**: Structural scene-graph hierarchy grouping without layout constraints.

#### Direct Function Pointers vs. Extension Adapter
- **Built-in primitives**: Execute via direct, unboxed function pointers stored in `FamilyBehavior`:
  - `EvaluateFn`: `fn(&EvaluateCtx, Option<&mut TextCompileCtx>) -> Result<Option<Vec<RenderCommand>>, RenderError>`
  - `BuildExtraFn`: `Option<fn(&mut Timeline, label, props, modifiers, children, time_ms, parent_label, diagnostics) -> Result<(), Vec<Diagnostic>>>`
  Dispatching built-in evaluation through direct function pointers eliminates dynamic trait object dispatch on the inner frame loop.
- **Extension primitives**: External plugins implement the `Primitive` trait and are registered at runtime via `BehaviorRegistry::register_extension`. They are stored in `BehaviorSource::Extension(Arc<dyn Primitive>)`. When `BehaviorDef::evaluate` is called, it adapts the plugin by invoking its `Primitive::evaluate` method, providing complete backward compatibility with plugin ecosystems while keeping the internal engine hot path monomorphic and inlineable.

#### Retention of Render Recursion
While entity data and evaluation contexts are flattened into ECS component stores, the scene evaluation walk (`render_actor_node_by_id`, `evaluate_node_by_id`) deliberately retains hierarchical tree recursion rather than adopting a flat linear entity loop. This design is retained for three architectural reasons:
1. **Static Subtree Caching**: Entire subtrees with no animated properties or active modifier targets are cached into pre-rendered static `vello::Scene` buffers. Recursion allows the evaluator to test the static subtree flag at a parent node and blit the cached scene in a single operation, skipping child traversals entirely.
2. **Camera and Coordinate Scopes**: Ancestor coordinate transformations, camera projections, and layout offsets accumulate along the tree hierarchy. Recursive descent naturally manages the transformation stack (`parent_transform * local_transform`) and opacity blending without intermediate stack allocations.
3. **Filter and Offscreen Composite Order**: Effects like `Filter`, `Mask`, and `Glass` establish compositing scopes that require rendering all descendant children into an offscreen target before applying GPU shader pipelines and blitting the result back to the parent target. Recursive structure strictly preserves painter's algorithm composite ordering and enables zero-readback optimizations (e.g. `can_post_composite_filter`) for tail nodes.

## 3. Access layer

`EntityRef<'w>` (Copy) and `EntityMut<'w>` are the only entity access surface for
call sites outside `world/`:

```rust
let e = timeline.entity("card").unwrap();
let size = timeline.world().geometry(e.id()).size.get(time_ms, DEFAULT_LAYOUT_HALF_SIZE);
let children = e.hierarchy().children.clone();

if let Some(mut e) = timeline.entity_mut("card") {
    e.meta_mut().locked = true;
    timeline.world_mut().geometry_mut(e.id()).size.ensure(default)
        .add_keyframe(t, v, Easing::Linear);
}
```

- Required-component accessors are infallible; optional ones return `Option`.
- `Timeline::entity(label)` is the boundary conversion from the language's
  label to `EntityId`. Inside a system, pass `EntityId` and use
  `entity_by_id`.
- Code that needs two components mutably at once uses the narrow
  `World::split_*_mut(id)` helpers rather than fighting the borrow checker with
  interior mutability.
- Tier data (geometry/style/shape/text/highlight/tagged/payload) is reached
  through `World` id accessors or the `TrackFields`/`TrackFieldsMut` bundle
  (`World::fields(id)`); `EntityRef`/`EntityMut` expose the components that
  are meaningful per view. There is no aggregate per-actor struct.

## 4. Systems

| System | Location | Input |
|---|---|---|
| Build | `timeline/build/*` | `&mut World`, build resources, diagnostics |
| Property write | `timeline/property_engine.rs`, `assignments/*` | `&mut World` + `EntityId` |
| Frame evaluation | `timeline/scene_eval/*`, `primitives/*` | `&World` + `EvalCaches` |
| Layout | `timeline/layout.rs`, `taffy_layout.rs` | `&World` (hierarchy + container + geometry) |
| Persistence | `timeline/persistence.rs`, `snapshot/*` | `&World` → `EntitySnapshot` |
| GUI | `crates/animatix-gui` | `EntityRef` for reads; edits via `SourceEdit` (AST) |

Evaluation must never allocate a label `String` or hash a label per node. The
only label touch points are build, expression env construction, diagnostics and
serialization. Two label-keyed structures deliberately remain at frame
boundaries: `SceneProgram::items`/`precise_bounds` (the observable frame
description names actors by label for tooling and the GUI, built once per
evaluated frame, never consulted by the per-node walk) and modifier
`Overrides` (keyed by authored expression targets, including synthesized
array labels).

## 5. Snapshots and serialization

`CarryBag`/`CarryEntry` serialize one entry per actor. There is no aggregate
runtime snapshot struct: each property tier rides the bag as its own field
(`CarryEntry::style`, `geometry`, `shape`, `text`, `highlight`,
`property_plan`, `tagged`, `payload`, …), each collapsed to a single t=0
keyframe by its per-tier collapse helper (`snapshot_style_at`,
`snapshot_geometry_at`, …).

- The container-layout component rides the carry bag as a whole
  (`CarryEntry::container_layout`) rather than through the per-property
  snapshot, so a re-rooted container keeps its layout in the successor scene.
- The animated child-order track rides the bag as labels
  (`CarryEntry::child_order`): the end state of a completed `swap`/`reorder`
  lives on that track, not on `ContainerMetadata::child_order`, so skipping it
  would visually undo the reorder at a `play` edge. Injection remaps the
  labels back to destination ids.
- `CarryEntry::caps` carries an extension primitive's capability override
  (injection's `set_identity` would otherwise re-derive default caps), and
  `CarryEntry::meta` carries the declaration-authored `EntityMeta` subset
  (`visible`, `camera_follow`, `parallax`, `hidden_by_default` — not `locked`
  or `first_seen_ms`, which are editor/per-scene state).
- The same holds for a plot host's coordinate frame
  (`CarryEntry::coordinate_frame`): a persistent `Graph` keeps its domains,
  scales and padding in the successor scene, so its child plots resolve there.
  Domain values are scene-relative, so carry needs no `EntityId` remap.
- `inject_entry` is two-pass: create every entity and component first, then
  link hierarchy / child order by label so ids are resolved once.
- `CarryBag` stays label-keyed on the wire; `EntityId` is an in-memory handle
  and is never part of the `.amx` source, the carry wire format, or
  `PropertyPlan` serialization.
- New snapshot fields carry `#[serde(default)]`. The `persistence.rs` and
  `extension_context.rs` round-trip tests are extended, not replaced.

## 6. Migration status

| Phase | Scope | Status |
|---|---|---|
| P0 | `docs/ecs.md`, file-size ratchet, AGENTS/architecture docs, baseline | done |
| P1 | `world/` stores, `EntityStore`, `EntityRef`/`EntityMut`, `Timeline.world` | done |
| P2 | hierarchy + roots (bounds slots deferred to P5h) | done |
| P3a | persistence / auto-color / transcript stores + `EntityId` hit regions | done |
| P3b | container-layout + child-order stores | done |
| P4 | coordinate-frame store, no env fallback, carry across scenes | done |
| P5 | header fields + legend + effects + payload + highlight + geometry/style/shape/text tiers + tagged + plan done | done |
| P6 | delete `AnimationTrack`/`TrackStore` (identity: id only), migrate GUI, snapshot DTO | done |
| P7 | perf + full gate matrix + docs close-out | done |
| Stage 1 | Actor capabilities componentization (`ActorCaps`), `BehaviorId`/`BehaviorDef` table, eliminate `ActorKind` | done |
| Stage 2 | `BehaviorDef` frame dispatch, split payload stores (`PlotPayload`, `SvgPayload`, `ImagePayload`), optional `shape`/`text`/`highlight` stores | done |
| Stage 3 | Built-in primitive migration to family systems, extension adapter (`BehaviorSource::Extension`), deprecation notes, normative docs | done |

**P6 close-out notes.** `TrackStore` became the maps-only `EntityStore`
(`entity.rs`): a label is live exactly while its index is below `len()`, and
there is no dense element to keep in sync. `AnimationTrack` was deleted along
with `timeline/track/mod.rs`; its former surface now lives where it belongs:

- tier helpers on the tier structs (`GeometryTracks::layout_size_*`,
  `TextTracks::{ascent_get,descent_get,baseline_get,set_metrics,evaluate_paths*}`,
  `ShapeTracks::{render_type_name,evaluate_vector_paths*,shape_path_memoized}`);
- plan sampling on `PropertyPlan` (`sample_property_named`, `sample_*_prop`);
- registry iteration and keyframe aggregation on `World`
  (`max_keyframe_time`, `has_any_keyframes`, `read_property_value(_or_default)`,
  `property_keyframe_*`, and the property-name boundary in
  `world/keyframes.rs` backed by `property_registry::field_for_property`);
- frame-path property injection and effective reads
  (`property_engine::read`) take `EntityId` + `&World`, with
  `ReadSource::read(id, world, t)` in the registry;
- `CarryEntry` carries `actor_type` plus the per-tier snapshots; the empty
  `{id}` placeholder field is gone from the wire format (serde ignores the
  legacy key, so old bags deserialize).

The access layer is `EntityId` + `World` everywhere; `EntityRef`/`EntityMut`
remain the label→id boundary views for call sites outside `world/`.

## 7. Adding a component

1. Define `pub struct XComponent` in `timeline/components/<domain>.rs`
   (`Clone`, `Debug`; `Serialize/Deserialize` behind `feature = "serde"` when
   it belongs in a snapshot).
2. Choose the store: `RequiredStore<X>` if every entity must have it, otherwise
   `ComponentStore<X>`. Add it to `World` and to `World::spawn` resize/populate.
3. Add `EntityRef::x()` (and `EntityMut::x_mut()` when writable). Keep the
   accessor `#[inline]`.
4. Remove the old field/table and its label lookup in the same change; update
   every call site to the accessor. `rustc` is the checklist.
5. Extend the per-tier snapshot helper (`snapshot_*_at`) / `inject_entry` and
   the round-trip test.
6. Add a test for the invariant: component presence, snapshot round-trip, and
   frame-cache invalidation on write.
7. Update this table and `docs/architecture.md` §2; if it is a new resource
   instead, record why in §2.3.

## 8. File-size ratchet

`scripts/file-size.sh` enforces the split programme as **two tiers, both
ratchets**:

- **Ceiling** — production source (`*.rs`, `*.js`, `*.css`, `*.html`) ≤ **800**
  lines, test source (`*/tests/*`, `tests.rs`) ≤ **1500**. A file over its
  ceiling must be frozen in `scripts/file-size-allowlist.tsv` (legacy rows;
  the split campaign took the list from 64 rows to zero, so it is empty
  today). Rows are deleted when a file is split; raising a row or adding a
  new one fails review.
- **Watch** — production ≤ **600** lines, tests ≤ **1200**. The forward
  guard: a file that crosses the watch threshold must be frozen at that size
  in `scripts/file-size-watch.tsv` in the same commit, and may not grow past
  its frozen row afterwards — growth beyond 600 lines is always a deliberate,
  reviewed act, never silent drift. A row is deleted when the file drops back
  under the threshold; raising one requires a commit that justifies the
  growth. This tier is what keeps files from ever *reaching* the ceiling, so
  the split campaign does not have to be re-run.
- Exemptions (both tiers): `crates/animatix-plugin-api/src/lib.rs` (one C ABI
  surface), wasm-bindgen/esbuild output (`web/pkg*`, `web/embed/amx-player.js`),
  `web/tools/node_modules`, `dist/`.

`scripts/ci.sh gate file-size` runs both tiers. `--seed-watch` rewrites the
watch list from the current tree (only when a deliberate, reviewed change
moves the threshold).

Splitting rules: pure moves plus `pub use` re-exports, no behaviour change in
the same commit; `foo.rs` becomes `foo/mod.rs` + modules named after the domain
they hold, and `mod.rs` keeps only declarations, re-exports and the entry point.
