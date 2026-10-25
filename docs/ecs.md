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
EntityId(u32) ── entities: EntityStore { meta: Vec<EntityMeta>, label maps }
                   │
                   ├── hierarchy:    RequiredStore<SceneHierarchyComponent>
                   ├── geometry:     RequiredStore<GeometryTracks>
                   ├── style:        RequiredStore<StyleTracks>
                   ├── shape:        RequiredStore<ShapeComponent>
                   ├── text:         RequiredStore<TextTracks>
                   ├── highlight:    RequiredStore<HighlightTracks>
                   ├── property_plan:RequiredStore<PropertyPlan>
                   ├── tagged_tracks:RequiredStore<TaggedTracksComponent>
                   ├── legend:       ComponentStore<LegendTracks>
                   ├── effects:      ComponentStore<EffectChainTrack>
                   ├── payload:      ComponentStore<PrimitivePayload>
                   ├── container_layout: ComponentStore<ContainerMetadata>
                   ├── coordinate:   ComponentStore<CoordinateFrameComponent>
                   ├── persistence:  ComponentStore<PersistenceComponent>
                   ├── auto_color:   ComponentStore<AutoColorComponent>
                   ├── child_order:  ComponentStore<ChildOrderComponent>
                   └── transcript:   ComponentStore<TranscriptComponent>
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
- **Frame caches are not components.** `bounds_slot`, `shape_command_memo`,
  `pending_shape_command_bounds`, `vector_paths_memo`, `shape_path_memo`,
  `shape_type_switches` belong to `EvalCaches`, indexed by `EntityId.index()`.
  A component is authored/build data; a cache is disposable per-frame state.

### 2.2 Components

| Component | Data | Presence |
|---|---|---|
| `EntityMeta` | id, label, actor_type, first_seen_ms, visible, locked, camera_follow, parallax, hidden_by_default | optional (reads fall back to `EntityMeta::DEFAULT`) |
| `DeclarationSpan` | `Span` of the actor declaration (absent when the parser had none) | optional |
| `SceneHierarchyComponent` | `parent: Option<EntityId>`, `children: Vec<EntityId>` | every entity |
| `RootSet` | `Vec<EntityId>` (insertion order) | resource |
| `GeometryTracks` | position/size/layout_size/rotation/scale/transform/placement/callout/connector/volume | every entity |
| `StyleTracks` | color/opacity/stroke/fill/gradient/blend/morph_options | every entity |
| `ShapeComponent` | shape type, from/to, arc, points, commands, vector paths | every entity |
| `TextTracks` | text content, typography, paths, metrics, split reveal | every entity |
| `HighlightTracks` | equation highlight color/opacity/padding/radius/blend | every entity (empty default) |
| `PropertyPlan` | extension/registry property slots | every entity |
| `TaggedTracksComponent` | tagged-union property tracks by canonical name | every entity |
| `LegendTracks` | legend entries | optional |
| `EffectChainTrack` | effect stages for Filter/Mask/Glass scopes | optional |
| `PrimitivePayload` | Plot/Svg/Image payload | optional |
| `ContainerMetadata` | layout type/gap/padding/align/cols/child order (`EntityId`s) | optional containers |
| `CoordinateFrameComponent` | domain/scale/padding/grid/ticks/tick labels | optional (plot hosts; sole source for hosted-child coordinate resolution) |
| `PersistenceComponent` | `persistent: bool` | optional |
| `AutoColorComponent` | auto-color slot | optional |
| `ChildOrderComponent` | `PropertyTrack<Vec<EntityId>>` | optional |
| `TranscriptComponent` | speech transcript | optional |

`caps` is not stored: `World::caps(id)` derives it from the actor type, and
extension primitives install an explicit override with `World::set_caps`.
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

Adding a resource instead of a component is a decision to document in the PR:
"looked up by label/order, not by entity id".

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
serialization.

## 5. Snapshots and serialization

`CarryBag`/`CarryEntry` serialize one entry per actor. There is no aggregate
runtime snapshot struct: each property tier rides the bag as its own field
(`CarryEntry::style`, `geometry`, `shape`, `text`, `highlight`,
`property_plan`, `payload`, …), each collapsed to a single t=0 keyframe by its
per-tier collapse helper (`snapshot_style_at`, `snapshot_geometry_at`, …).

- The container-layout component rides the carry bag as a whole
  (`CarryEntry::container_layout`) rather than through the per-property
  snapshot, so a re-rooted container keeps its layout in the successor scene.
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
