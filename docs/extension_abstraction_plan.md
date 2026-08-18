# Extension Abstraction Plan

Status: superseded by `docs/unified_extension_design.md`

This plan replaces the previous open-ended "make primitives/properties
dynamic" discussion with a concrete, benchmark-gated migration. The target is
not to eliminate every `match`; it is to eliminate every match that grows with
the number of primitives or properties.

## Goal

An external primitive/property can be added through a registry and descriptor
without editing core enums such as `ActorKindId`, `ActorField`, `ShapeKind`,
`PROPERTY_REGISTRY`, or the scene evaluation dispatch.

Core engine keeps a finite set of value kinds and render commands. Common
properties remain directly typed for performance.

## Target Architecture

```text
Registry<PrimitiveDescriptor>
    type_name
    display_name
    category / icon / capabilities
    allowed_properties: &[PropertyDescriptor]
    default_props
    build
    evaluate
    handle_assignment
    resize_mode
    layout_hints

Registry<PropertyDescriptor>
    property_id
    name
    value_kind
    default_value
    flags
    read / write / interpolate strategy
    gui_editor

AnimationTrack
    common: CommonTracks
    custom: SmallVec<[PropertySlot; N]>

PropertyPlan
    common_slots
    custom_slots
```

Design principles:

1. Property names are resolved to `PropertyId` at build time. Frame-time paths
   do not perform string lookup.
2. Common properties stay direct fields (`geometry`, `style`, etc.).
3. Custom properties use a finite `DynTrack` enum, not `Any` or per-property
   trait objects.
4. Primitive dispatch happens at actor granularity, not property granularity.
5. Built-ins and extensions use the same descriptor path after migration.
6. Static frame/subtree caches remain available to extension primitives.
7. Syntax/typechecker/LSP consume a shared schema, not a runtime registry.

## Phases

### Phase 0: Baseline and Match Inventory

- Create benchmark baseline for build/evaluate, property interpolation,
  `always` scenes, static scenes, and extension-shaped dynamic properties.
- Inventory all match sites over `ActorKindId`, `ActorField`, `ValueType`,
  `ShapeKind`, and primitive string dispatch.
- Classify each site as hot path, cold path, essential finite core, or
  accidental match that should be removed.
- Define the guard test that adding an external primitive/property does not
  require editing core files.

Acceptance:

- Baseline numbers are captured.
- Match inventory is recorded.
- Every remaining core enum match has a stated reason or migration owner.

### Phase 1: Shared Schema Layer

- Define `PropertyDescriptor` and primitive metadata in a schema crate or
  `animatix-syntax`, not in the runtime crate.
- Migrate `PROPERTY_REGISTRY`, `typing::property_type()`, analyzer property
  diagnostics, GUI inspector metadata, and completion sources to shared
  descriptors.
- Assign stable `PropertyId` values.
- Keep runtime behavior unchanged.

Acceptance:

- One schema edit updates typechecker, completion, hover, and inspector.
- All existing tests pass.
- No new runtime dependency is added to analyzer/LSP.

### Phase 2: Dynamic Property Runtime

- Add `PropertyId`, `PropertySlot`, `DynTrack`, and `PropertyPlan`.
- Add custom property storage to `AnimationTrack` while keeping common direct
  fields.
- Replace `ActorField`-only dispatch with descriptor-driven accessors:
  common fields map to direct storage, custom fields map to slots.
- Keep `PropertyValue` interpolation as a finite enum match.
- Implement schema-aware serialization/persistence.

Acceptance:

- External properties work in build, assignment, keyframes, `always` env
  injection, inspector, and completion.
- Static scene benchmark remains near baseline.
- Dynamic property benchmark target: <= 1.5x current typed property path.

### Phase 3: Dynamic Primitive Runtime

- Introduce `PrimitiveHandle` / opaque actor identity.
- Extend `Primitive` trait into a full descriptor.
- Migrate `scene_eval.rs` legacy `ActorKindId` dispatch to descriptor
  capabilities.
- Migrate layout, legend, callout, media, persistence, and actions from
  `ActorKindId` matches to capabilities.
- Register built-ins through the same registry.

Acceptance:

- Adding a new external primitive requires no core enum edit.
- Existing primitives pass behavior tests.
- Static scene benchmark target: <= 1.2x current baseline.
- `scene_eval.rs` no longer has a large `ActorKindId` dispatch.

### Phase 4: GUI/LSP/Typechecker Integration

- GUI insertion palette consumes primitive registry.
- Inspector, property groups, keyframe table, and default props consume
  property descriptors.
- LSP completion/hover and analyzer diagnostics consume shared schema plus
  extension registry.
- Keep `.amx` source syntax stable.

Acceptance:

- External primitives/properties are usable end-to-end in GUI and LSP.
- Existing source files round-trip unchanged.

### Phase 5: Extension API and Plugin Loader

- Implement `ExtensionContext`:
  - register primitive/property/action/function
  - provide/get services
  - on/emit lifecycle events
  - dispose and scoped contexts
- Add `Timeline::build_with_context()` and `Composition::build_with_context()`.
- Wire CLI/GUI per-document contexts.
- Add plugin disposer semantics for rebuild/hot reload.
- Optionally add `.amx` plugin manifests.

Acceptance:

- A sample plugin registers primitive + property + action + function + service
  without core edits.
- Disposal removes all registered capabilities.
- Hot reload does not leak registry entries or listeners.

### Phase 6: Performance and Hardening

- Optimize `PropertyPlan` layout and access.
- Keep static caches for non-dynamic extension primitives.
- Add dirty/static capability flags for dynamic primitives.
- Add benchmark regression gates.
- Add guard tests for registry completeness and schema drift.
- Document extension authoring and migration guide.

Acceptance:

- Static scenes remain same order of magnitude as baseline.
- Typical dynamic scenes stay within 1.2-1.5x current typed path.
- Registry growth does not linearly slow single-actor evaluation.

## Performance Strategy

- No String hash lookup in frame-time property access.
- `PropertyPlan` is built once and shared via `Arc`.
- Common properties are direct fields.
- Custom properties use a finite `DynTrack` enum.
- Property interpolation uses finite `PropertyValue` match, not trait objects.
- Primitive dynamic dispatch is one call per actor per frame, not per property.
- Static scene cache is preserved via `can_cache_static_frame` capability.
- Benchmarks gate every phase.

## Risk Register

| Risk | Mitigation |
|------|------------|
| Large refactor regression | Phased work; old APIs remain or dual-run during migration |
| Performance regression | Phase 0 baseline plus benchmark gates |
| Persistence format change | Schema version and migration; keep current format first |
| Syntax/runtime dependency cycle | Shared schema lives in `animatix-syntax` or a schema crate |
| External primitive capability gaps | Define capability matrix before primitive migration |
| Plugin state/hot reload leaks | ExtensionContext disposers and per-build registries |
| Over-abstraction | Forbid `Any` values; keep finite value-model core |

## Execution Log

- Plan created.
- Phase 0: match inventory script added.
- Phase 0: match inventory report generated at
  `docs/extension_abstraction_inventory.md`.
- Phase 0: benchmark baseline captured with existing benches:
  - `property_track_evaluate`: ~894 ps
  - `interpolate_f32`: ~586 ps
  - `interpolate_vec4`: ~7.42 ns
  - `timeline_evaluate_0s`: ~106.3 ns
  - `timeline_evaluate_1s`: ~105.1 ns
  - `timeline_evaluate_2s`: ~105.3 ns
- Phase 1: shared `animatix-syntax::schema` module added.
- Phase 1: `typing::property_specs()` moved to schema; typechecker property
  metadata now consumes the shared schema.
- Phase 1: stable `PropertyId` and `PropertySpec` added; ids are assigned in
  declaration order and covered by tests.
- Phase 1: runtime `PROPERTY_REGISTRY` migration still pending; shared
  primitive specs now match runtime specs and are covered by a drift guard.
- Phase 1: built-in property schema remains additive relative to the runtime
  registry; external properties use shared `PropertyValueKind` descriptors.
- Phase 2: runtime `PROPERTY_REGISTRY` now exposes `property_id` /
  `property_schema_by_id` for its own registry index; shared-schema built-in
  property ids are still declaration-order and not yet unified.
- Phase 2: `timeline::plan::PropertyPlan` and `DynTrack` prototype added;
  frame-time access is by sorted id and finite value-kind dispatch.
- Phase 2: benchmark added for `property_plan_lookup_and_sample`: ~6.2 ns
  versus direct typed `property_track_evaluate` ~860 ps on this machine.
- Phase 2: `AnimationTrack` now owns a `PropertyPlan` and rebuilds it after
  actor kind resolution in the main build path.
- Phase 2: `property_engine` gained public plan-backed write/read helpers with
  eased keyframes; extension slots are created lazily through `ensure_slot`.
- Phase 2: `timeline` re-exports `write_property_plan_slot` /
  `read_property_plan_slot` as the extension-facing property API.
- Phase 2: `write_property_field` now routes registry-backed tagged properties
  through `PropertyPlan`; `read_property_value` and keyframe query helpers also
  fall back to plan slots.
- Phase 2: special `legend` / `callout_place` tagged fields still use their
  existing typed paths.
- Phase 2: `ExtensionContext` now registers `ExtensionPropertySpec` descriptors;
  declarations, assignments, and keyframes write through
  `PropertyPlan`/`DynTrack` without string hash lookup at frame time.
- Phase 2: external properties are injected into frame environments and actor
  re-declarations preserve extension slots across plan rebuilds.
- Phase 3: shared `PrimitiveSpec` / `PrimitiveCapabilities` added to schema.
- Phase 3: built-in primitives are exposed through shared schema specs.
- Phase 3: `PrimitiveRegistry` layers custom primitives over built-ins with
  duplicate detection and schema conversion.
- Phase 3: `Timeline::build_with_primitive_registry()` injects the registry
  into actor declaration processing; a custom primitive can now build through
  the normal timeline path without adding a core actor type.
- Phase 3: `AnimationTrack` stores the source `actor_type`, and `scene_eval`
  resolves custom primitives through `Timeline.primitive_registry` before the
  static built-in fallback.
- Phase 3: integration test builds a custom `Gauge`, evaluates the timeline,
  and confirms the custom primitive's `evaluate()` participates in rendering.
- Phase 3: remaining manual child-processing paths for `Filter`, `Mask`, and
  `Equation` are container/subtree concerns rather than primitive dispatch.
- Phase 4: GUI insertion palette now reads primitives through the active
  timeline's `PrimitiveRegistry` instead of the static `PRIMITIVES` slice.
- Phase 4: shared schema gained `builtin_primitive_specs()`; analyzer
  completion and hover now consult it in addition to the static builtins table.
- Phase 4: GUI `DocumentSession` now owns a per-document `ExtensionContext` and
  rebuilds through the context-aware `BuildTarget` path.
- Phase 4: GUI property inspector and keyframe table surface registered
  extension properties from the actor plan.
- Phase 5: `ExtensionContext` added with primitive/action/function/service
  registration APIs.
- Phase 5: `Timeline::build_with_context()` installs extension functions and
  the context's primitive registry into the build.
- Phase 5: custom extension actions are now dispatched by
  `process_action_with_extensions` during timeline build.
- Phase 5: `ExtensionContext` gained explicit removal APIs for primitives,
  actions, functions, and services; `PrimitiveRegistry::remove` backs disposal.
- Phase 5: `ExtensionScope` automatically disposes all registrations made
  through it when dropped.
- Phase 5: `ExtensionPlugin` + `PluginLoader` added; plugins install into a
  context and return disposers.
- Phase 5: `BuildTarget::from_ast_with_context()` added; both single-scene and
  multi-scene composition builds now propagate the extension context.
- Phase 5: `Composition::build_with_font_context_and_asset_cache_and_extension_context`
  and carry-aware context builds added.
- Phase 5: CLI render/check paths now build through `ExtensionContext` and
  `BuildTarget::from_ast_with_context`; plugin CLI arguments still pending.
- Phase 5: stable native plugin ABI added in `crates/animatix-plugin-api`; the
  `plugin-loading` feature adds `NativePlugin` loading via `libloading`.
- Phase 5: CLI `--plugin` accepts `*.amx-plugin.toml` manifests and native
  library paths, installs loaded plugins into per-build contexts, and feeds
  merged manifests to analyzer diagnostics.
- Phase 5: CLI `plugin describe <library>` regenerates a TOML
  `ExtensionManifest` from runtime primitive/property descriptors, so analyzer
  metadata and the demo manifest stay derived from the plugin instead of being
  hand-maintained.
- Phase 5: `crates/animatix-plugin-demo` is a workspace `cdylib` sample that
  registers an external property and a native expression function through the
  stable ABI.
- Phase 6: extension authoring guide added at
  `docs/extension_authoring.md`.
- Phase 6: `scripts/extension-bench.sh` added to run property interpolation and
  timeline eval baselines.
- Phase 6: script supports `--max-plan-ns` threshold gate; verified with
  `--quick --max-plan-ns 10000`.
- Phase 3: `Primitive` now exposes shared schema capabilities;
  `PrimitiveDescriptor` and schema specs derive from it instead of string
  matching in the descriptor.
- Phase 3: `Primitive` gained a `child_processing()` capability hook.
  `Filter`, `Mask`, and `Equation` declare their subtree strategy, and
  `scene_eval::render_node_children` dispatches through that hook instead of
  matching `ActorKindId` variants.
- Phase 6: runtime `PrimitiveRegistry::specs()` and
  `animatix-syntax::schema::builtin_primitive_specs()` are aligned and covered
  by a drift guard test (display names, icons, categories, advanced flags, and
  capabilities).
- Phase 6: extension property end-to-end tests cover custom primitives,
  built-in actors, declaration keyframes, assignment transitions, and frame
  environment injection.
- Phase 6: CI no longer runs the removed tree-sitter parser sync job; a quick
  `extension-bench.sh --max-plan-ns 10000` gate now runs on pull requests.
