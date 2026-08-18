# Unified Extension Architecture

Status: proposed

This document refines the extension abstraction plan into one concrete target:
a single descriptor/registry path for built-ins, in-process extensions, and
native plugins. It records feasibility, change surface, and the phased
execution order.

## Current State

The workspace still has several parallel sources of truth:

| Concern | Built-ins | Extensions |
|---|---|---|
| Primitive metadata/dispatch | static `PRIMITIVES` + `Primitive` trait | `PrimitiveRegistry` custom entries |
| Property metadata | `schema::PropertySpec` + runtime `PROPERTY_REGISTRY` | `ExtensionContext::ExtensionPropertySpec` |
| Property storage | typed `AnimationTrack` fields | `PropertyPlan` + `DynTrack` |
| Runtime lookup | `find_primitive()` / `PrimitiveDescriptor::for_actor_type()` | `Timeline::primitive_registry` |
| Tooling metadata | `schema::builtin_primitive_specs()` | analyzer/LSP `ExtensionManifest` |
| Native plugins | n/a | C ABI v1 property/function registration |

The main consequence is that adding an external primitive still needs the
runtime `Primitive` trait and the extension registry, while adding a built-in
primitive needs the static list and descriptor helpers. Native plugins cannot
currently register primitives/actions/services at all.

## Target Model

The unified model keeps performance inside `AnimationTrack` but makes all
registration and lookup go through one shape:

```text
ExtensionRegistry
├── primitives: Registry<PrimitiveDescriptor, PrimitiveImpl>
├── properties: Registry<PropertyId, PropertyDescriptor>
├── actions:    Registry<ActionSignature, ActionImpl>
└── functions:  Registry<String, FunctionImpl>
```

Built-ins are seeded into the same registry at process/context initialization.
External Rust plugins and native plugins install through the same registration
API. After installation, the engine cannot tell the source apart.

### Primitive Layer

- `PrimitiveDescriptor` becomes the neutral descriptor: type name, display
  name, category, icon, capabilities, child-processing strategy, and owned
  property descriptors.
- `PrimitiveImpl` is the runtime implementation behind a descriptor. Built-ins
  and Rust extensions are `Arc<dyn Primitive>` implementations; native plugins
  become host-side adapters over C callbacks.
- `PrimitiveRegistry` owns a single list of `Arc<dyn Primitive>`. There is no
  separate static lookup path once built-ins are seeded through it.

### Property Layer

- `PropertyDescriptor` is the shared schema object: stable id, owned name,
  actor types, type annotation, value kind, and injection flag.
- Runtime keeps a `PropertyBinding` for built-ins that maps the descriptor to
  `ActorField`, read source, group, flags, and default. Extensions use
  `PropertyPlan` slots. The engine still has a fast path for built-ins, but
  tooling and registration share one descriptor.
- `AnimationTrack` can later migrate all property storage to `PropertyPlan`
  while keeping direct fields as generated hot-path caches.

### Native Plugin Layer

The stable ABI should grow to a complete callback table, not a Rust trait
object crossing the library boundary:

```rust
#[repr(C)]
struct AnimatixPluginApiV2 {
    register_primitive,
    register_property,
    register_action,
    register_function,
    primitive_build,
    primitive_evaluate,
    primitive_handle_assignment,
    primitive_process_children,
    // ...
}
```

Host-side adapter types turn those callbacks into `PrimitiveImpl` entries.
Manifests provide the same descriptors to analyzer/LSP/GUI without loading the
library.

## Feasibility

A complete migration is feasible but large. It must be phased because it
touches hot evaluation paths, persistence, GUI inspector metadata, analyzer
completions, and native ABI compatibility.

### Change Surface

| Area | Expected surface |
|---|---|
| `animatix-syntax::schema` | owned `PropertyDescriptor`; keep static `PropertySpec` as bootstrap view |
| `animatix::primitives` | unify `PrimitiveRegistry` storage; delete or reduce static lookup helpers |
| `animatix::timeline::property_registry` | add descriptor/binding split; migrate `PROPERTY_REGISTRY` entries |
| `animatix::timeline::dispatch` | route built-in property access through descriptor binding |
| `animatix::extension_context` | delegate all registration to `ExtensionRegistry` |
| GUI inspector/keyframe table | consume unified descriptor list |
| analyzer/LSP | consume shared descriptor plus manifests |
| `animatix-plugin-api` | add ABI v2 callback table |
| docs/authoring | document the single registration path |

### Risk

- Hot-path property access must stay direct for built-ins until the
  `PropertyPlan` fast path is benchmarked.
- Native primitive callbacks are the largest ABI surface; they must be versioned
  and tested with the demo plugin.
- Persistence and carry bags must keep stable IDs.
- LSP must remain runtime-free; it consumes descriptors/manifests only.

## Execution Status

- Phase 1 implemented: `PrimitiveRegistry` stores built-ins and extensions in
  one `Arc<dyn Primitive>` list. Built-ins are seeded through `register()` and
  remain non-removable through a built-in prefix.
- Phase 2 implemented: `animatix::property_descriptor::PropertyDescriptor` is a
  unified owned descriptor view, and `Timeline::property_descriptors()` merges
  built-in schema entries with extension properties.
- Phase A implemented: `PropertyDescriptor` now lives in
  `animatix-syntax::schema`; shared `PrimitiveDescriptor` and
  `ChildProcessingKind` were added; runtime keeps re-exports/helpers.
- Phase B implemented: the per-build capability container is now
  `ExtensionRegistry`; `ExtensionContext` remains as a compatibility alias,
  and `Timeline` stores `Arc<ExtensionRegistry>`.
- Phases 3-6 remain planned; they involve property binding migration,
  capability dispatch migration, native ABI v2, and tooling cleanup.

## Phases

### Phase 1: Unified Primitive Registry

Make `PrimitiveRegistry` store built-ins and extensions in one list, seeded
through the same registration path. Keep `find_primitive()` as a compatibility
helper backed by the same built-in registry.

Acceptance:

- `PrimitiveRegistry::new()` contains the same built-in set.
- `find()` and `iter()` behave identically.
- Existing primitive tests and workspace gates pass.

### Phase 2: Shared Property Descriptor

Add an owned `PropertyDescriptor` shared between schema and extension context.
Expose a merged descriptor view from `Timeline` so tooling can eventually stop
reading separate built-in and extension tables.

Acceptance:

- Built-in and extension property descriptors have the same struct/API.
- Runtime IDs remain stable.
- Analyzer/LSP continue to use schema/typing sources.

### Phase 3: Property Binding Migration

Split runtime `PropertySchema` into descriptor + binding. Generate the
runtime `PROPERTY_REGISTRY` compatibility table from the shared descriptor
where possible, and route built-in property access through the binding.

Acceptance:

- One property table edit updates analyzer/GUI/runtime where applicable.
- Persistence, interpolation, and frame injection tests pass.

### Phase 4: Capability Dispatch Migration

Replace remaining `ActorKindId`/`PrimitiveDescriptor` string matches with
descriptor capabilities and child-processing strategies. Remove the static
`PRIMITIVES`-specific call sites.

Acceptance:

- Adding a built-in or extension primitive requires only one registration path.
- `scene_eval.rs` does not grow with primitive count.

### Phase 5: Native ABI v2

Extend `animatix-plugin-api` with primitive/action/service callbacks. Wrap them
in host-side `Primitive` adapters so native plugins register into the same
registry.

Acceptance:

- Demo native plugin registers primitive + property + action + function +
  service.
- Disposal removes all capabilities and keeps loaded libraries alive until
  callbacks are gone.

### Phase 6: Cleanup and Tooling Unification

Remove duplicate static metadata paths, switch GUI/analyzer/LSP consumers to
the unified descriptors, and update benchmarks/docs.

Acceptance:

- `find_primitive`, runtime `PROPERTY_REGISTRY`, and shared schema no longer
  have competing sources of truth.
- Full commit gates pass.
