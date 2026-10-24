# Timeline ECS Architecture

**Status:** migration in progress. Phase 0 (guardrails + baseline) is in place;
Phases 1–7 are tracked in the status table at the bottom of this file.

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
| `EntityStore` | `Vec<EntityMeta>` + `HashMap<String, EntityId>` + `Vec<String>` | entity header; append-only |
| `RequiredStore<C>` | `Vec<C>` | components every entity has (default-initialised on spawn) |
| `ComponentStore<C>` | `Vec<Option<C>>` | components only some entities have |

**Transitional state (Phase 1–5).** `World` still owns a `TrackStore` whose dense
element is `AnimationTrack`; the header and the not-yet-migrated tiers live there
until their domain moves. `SceneHierarchyComponent`, `World::roots`, and the
build-time `pending_parents`/`pending_roots` buffers are the first ECS stores.
`add_node` records the relation before the primitive creates the child's track,
so the link is buffered by label and adopted (as an id) by
`World::spawn_with_track`; `World::finish_hierarchy()` resolves anything created
by direct `TrackStore` insertion. The buffer is empty after a completed build and
is not runtime storage.

Rules:

- **Append-only.** `TrackStore` has no removal API today and the GUI rebuilds a
  whole `Timeline` for structural deletes, so entity slots are never reused and
  `EntityId.index()` stays valid for the life of the timeline. `World::clear()`
  is for a whole rebuild only. There is no free list or generation counter.
- **Aligned stores.** `World::spawn` grows every store to `entities.len()` and
  fills required stores with `C::default()`. `spawn_replacing(label, ...)`
  clears the entity's optional components and resets required components,
  matching the old `TrackStore::insert` replacement semantics used by carry
  injection.
- **Required means present.** An entity that exists has every `RequiredStore`
  entry. Tests pin this (`every_entity_has_required_components`).
- **One source of truth per domain.** A field moves to a component and is
  deleted from `AnimationTrack` in the same change. No dual writes across a
  merge boundary.
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
| `EntityMeta` | id, label, actor_type, caps, first_seen_ms, visible, locked, camera_follow, parallax, hidden_by_default | optional (reads fall back to `EntityMeta::DEFAULT`) |
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

`hidden_by_default` and `first_seen_ms` moved with the other header fields:
the entrance-action helpers (`lift_hidden_by_default*`) now take
`&mut AnimationTrack` and `&mut EntityMeta` side by side. The remaining
`EntityMeta` columns (`id`, `label`, `actor_type`, `caps`) are still on
`AnimationTrack` during Phase 5; `visible`, `locked`, `camera_follow`,
`parallax`, `hidden_by_default`, `first_seen_ms` are live in the component.

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
let size = e.geometry().size.get(time_ms, DEFAULT_LAYOUT_HALF_SIZE);
let children = e.hierarchy().children.clone();

if let Some(mut e) = timeline.entity_mut("card") {
    e.geometry_mut().size.ensure(default).add_keyframe(t, v, Easing::Linear);
}
```

- Required-component accessors are infallible; optional ones return `Option`.
- `Timeline::entity(label)` is the boundary conversion from the language's
  label to `EntityId`. Inside a system, pass `EntityId` and use
  `entity_by_id`.
- Code that needs two components mutably at once uses the narrow
  `World::split_*_mut(id)` helpers rather than fighting the borrow checker with
  interior mutability.
- `AnimationTrack` is not part of the end-state API. During the migration the
  `EntityRef` accessors forward to `AnimationTrack` fields, so a call site can
  adopt the accessor before the backing store moves. When the domain moves, the
  accessor changes and the field disappears; call sites do not change again.

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

`CarryBag`/`CarryEntry` serialize an `EntitySnapshot` per actor. The snapshot is
the only aggregate struct left after Phase 6; it exists for serde, tests and
cross-scene carry, not for runtime storage.

- `snapshot_entity_at(world, id, time_ms)` samples every property track to a
  single t=0 keyframe (the old `snapshot_track_at` behaviour, extended to all
  components and `DynTrack` slots).
- The container-layout component rides the carry bag as a whole
  (`CarryEntry::container_layout`) rather than through the per-property
  snapshot, so a re-rooted container keeps its layout in the successor scene.
- The same holds for a plot host's coordinate frame
  (`CarryEntry::coordinate_frame`): a persistent `Graph` keeps its domains,
  scales and padding in the successor scene, so its child plots resolve there.
  Domain values are scene-relative, so carry needs no `EntityId` remap.
- `restore_entity` is two-pass: create every entity and component first, then
  link hierarchy / child order by label so ids are resolved once.
- `CarryBag` stays label-keyed on the wire; `EntityId` is an in-memory handle
  and is never part of the `.amx` source or `PropertyPlan` serialization.
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
| P5 | header fields (decl span, visibility, lock, camera, hidden, first-seen) + legend + effects done; payload/highlight/text/shape/style/geometry/plan pending | in progress |
| P6 | delete `AnimationTrack`/`TrackStore`, migrate GUI, snapshot DTO | pending |
| P7 | perf + full gate matrix + docs close-out | pending |

**Tier-migration prerequisite.** `TrackFieldRef` / `TrackFieldMut` construction
and the property read/write path now go through `TrackFields` / `World`
(`World::read_property_value` and `World::write_property_field`), so the
registry-backed tiers can move one at a time. The remaining blocker is call
sites: the `AnimationTrack`-taking free wrappers still exist for compatibility.
`payload` is outside that table and additionally needs a render/evaluate
context field for the image/SVG/plot primitives.

Direction for unlocking the rest: a borrowed `TrackFields<'a>` /
`TrackFieldsMut<'a>` bundle (geometry/style/shape/text/highlight/tagged/payload
refs) now exists in `timeline/world/fields.rs`, assembled by `World::fields(id)`
and `World::fields_mut(id)`. `TrackFieldRef` / `TrackFieldMut` construction has
moved onto the bundle (`TrackFields::field_ref` / `TrackFieldsMut::field_mut`),
and both property directions now go through it:
`TrackFields::read_property_value` / `World::read_property_value(id, field, t)`
and `write_property_fields` / `World::write_property_field(id, …)`. Keyframe
introspection now has the same shape (`TrackFields::property_has_keyframes` /
`property_keyframe_count` / `property_keyframe_times` /
`property_keyframe_easing`, mirrored on `World`). The `AnimationTrack`-taking
free functions are forwarding wrappers, so existing call sites keep compiling
while they migrate per domain; deleting those wrappers is what removes the last
path that assumes the tiers live on the track.

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
5. Extend `snapshot_entity_at` / `restore_entity` and the round-trip test.
6. Add a test for the invariant: component presence, snapshot round-trip, and
   frame-cache invalidation on write.
7. Update this table and `docs/architecture.md` §2; if it is a new resource
   instead, record why in §2.3.

## 8. File-size ratchet

`scripts/file-size.sh` enforces the split programme:

- production source (`*.rs`, `*.js`, `*.css`, `*.html`) ≤ **800** lines;
- test source (`*/tests/*`, `tests.rs`) ≤ **1500** lines;
- exemptions: `crates/animatix-plugin-api/src/lib.rs` (one C ABI surface),
  wasm-bindgen/esbuild output (`web/pkg*`, `web/embed/amx-player.js`),
  `web/tools/node_modules`, `dist/`.

`scripts/file-size-allowlist.tsv` freezes every legacy offender at its current
line count. The ratchet only turns one way: a row is deleted when the file is
split; raising a row or adding a new one fails review. `scripts/ci.sh gate
file-size` runs it in the `Fast Checks` job. The split campaign took the list
from 64 rows to zero. The last file, `timeline/build/actor.rs`, became
`build/actor/{mod,decl,helpers,extensions,prepare}.rs`: `process_actor_decl`
stays the sequential entry point in `decl.rs`, while early dispatch, prepare
state, size-spec parsing, position binding and container finalize moved to
`prepare.rs`, and path generation/keyframe insertion/extension and plot
helpers moved to their own modules.

Splitting rules: pure moves plus `pub use` re-exports, no behaviour change in
the same commit; `foo.rs` becomes `foo/mod.rs` + modules named after the domain
they hold, and `mod.rs` keeps only declarations, re-exports and the entry point.
