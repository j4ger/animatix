# Unified Extension Architecture

Status: complete

This document refines the extension abstraction plan into one concrete target:
a single descriptor/registry path for built-ins, in-process extensions, and
native plugins. It records feasibility, change surface, and the phased
execution order.

## Current State

Registration and dispatch go through one registry shape. The complete extension
pass closed the remaining runtime seams: native primitives render through unstable ABI snapshot 5
path/text/image/highlight commands, image commands resolve cached URLs against
document/workspace base paths, manifest descriptors no longer guess runtime
property ids, custom primitives dispatch through the active registry,
capability flags drive container/plot/text dispatch, primitive metadata is
borrowed instead of leaked, native services carry destructors, extension tracks
use the neutral `ActorKindId::Extension` instead of category-derived built-in
kinds, the CLI can regenerate analyzer manifests from loaded runtime
descriptors, and the GUI manages plugins through a workspace-aware
last-known-good `DocumentPluginManager`.

| Concern | Built-ins | Extensions |
|---|---|---|
| Primitive metadata/dispatch | `PrimitiveRegistry` + `Primitive` trait | same `PrimitiveRegistry` custom entries |
| Property metadata | `schema::PropertySpec` + runtime `PROPERTY_REGISTRY` | `PropertyRegistry` descriptors/bindings |
| Property storage | typed `AnimationTrack` fields | `PropertyPlan` + `DynTrack` |
| Runtime lookup | `find_primitive()` / `Timeline::primitive_registry` | `Timeline::primitive_registry` |
| Tooling metadata | `schema::builtin_primitive_specs()` | shared `PrimitiveDescriptor`/`PropertyDescriptor` manifests |
| Native plugins | n/a | C ABI snapshot 5 primitive/action/service registration |

`PROPERTY_REGISTRY` remains the runtime binding table (typed field, read
source, flags, defaults) while shared schema owns descriptors (name, actor
types, type, id); drift guards require both directions to stay aligned.

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

- `PropertyDescriptor` is the shared schema object: optional runtime id, owned
  name, actor types, type annotation, value kind, and injection flag. Runtime
  registries always set the id; manifests leave it `None` because ids are
  allocated by `ExtensionRegistry`, never guessed by a manifest.
- Runtime keeps a `PropertyBinding` for built-ins that maps the descriptor to
  `ActorField`, read source, group, flags, and default. Extensions use
  `PropertyPlan` slots. The engine still has a fast path for built-ins, but
  tooling and registration share one descriptor.
- `AnimationTrack` can later migrate all property storage to `PropertyPlan`
  while keeping direct fields as generated hot-path caches.

### Native Plugin Layer

The unstable native ABI should grow to a complete callback table, not a Rust trait
object crossing the library boundary:

```rust
#[repr(C)]
struct AnimatixPluginApiV3 {
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
Native evaluate callbacks receive a host context with an `append_path` command
table, so they can emit real vector render commands. Service values carry an
optional destructor and keep the plugin library alive until disposal.
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
| GUI document lifecycle | auto-discover manifests and native plugins beside the document |
| analyzer/LSP | consume shared descriptor plus manifests |
| `animatix-plugin-api` | add unstable ABI snapshot 5 callback table |
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
- Phase C implemented: `PropertyRegistry` now owns built-in
  `PropertyEntry`/`PropertyBinding` rows plus extension specs, and
  `Timeline::property_descriptors()` reads from that unified registry.
- Phase D implemented: `find_primitive()` is backed by a global built-in
  `PrimitiveRegistry`, and timeline build/eval paths use
  `Timeline::primitive_registry` directly.
- Phase E implemented: `animatix-plugin-api` grew a v3 callback table for
  primitives, actions, and services; the native host wraps them in
  `Primitive`/`BuiltinAction` adapters and the demo plugin registers all five
  capability kinds through the same install path.
- Phase F implemented: GUI inspector/keyframe table read extension properties
  through unified `PropertyDescriptor`s from `Timeline`, and analyzer/LSP
  manifests parse into shared `PrimitiveDescriptor`/`PropertyDescriptor`
  objects used by completions and hover.
- Phase G passed: fmt, clippy, workspace tests, no-video build/test, rustdoc,
  docs/example/bench gates, and eparts feature checks are all green.
- Hardening pass: native primitives emit vector commands through the host
  evaluate context; manifest property ids are `None` instead of guessed;
  custom primitive routing uses the active registry's built-in prefix;
  primitive metadata no longer uses `Box::leak`; services own destructors and
  library lifetime.
- Manifest generation pass: `animatix plugin describe <library>` installs the
  plugin into a scratch `ExtensionContext`, filters to extension
  primitive/property descriptors, and serializes the analyzer manifest with
  the same TOML schema consumed by `--plugin`.
- Unstable ABI snapshot 5 pass: native descriptors carry precise tooling types, function
  metadata, service metadata, primitive capabilities, declared property names,
  and resize mode; the demo manifest round-trips all of it.
- Native text pass: `append_text` accepts `Text`, `Code`, and `Typst` renderer
  kinds, and `Type::Enum(...)` lets native/manifest properties expose named
  choices to the inspector.
- Asset URL pass: `AssetCache` normalizes relative URLs against the document
  directory and optional workspace root, and explicit native image URLs that
  are not cached return an error instead of falling back to the actor image.
- Runtime polish pass: primitive family classification accepts any
  `&dyn Primitive`, plot hosting uses a capability instead of `"Graph"`
  string dispatch, recursive container expansion shares one
  capability/child-processing definition, and `declared_property_names` avoids
  repeated linear membership checks in the generic property writer.
- Plugin lifecycle pass: `PluginLoader` exposes list/replace/remove APIs, and
  `DocumentPluginManager` provides workspace-aware discovery, atomic
  last-known-good swaps, background-rebuild context reuse, error surfacing, and
  manual/automatic reload.
- GUI integration pass: `animatix-gui` enables `plugin-loading` by default,
  installs manifests/libraries through `DocumentPluginManager`, feeds the
  merged manifest into editor completions/hover, inserts extension actions from
  the timeline, renders manifest-driven property editors, and exposes a plugin
  status/authoring dialog.
- Capability dispatch pass: layout container expansion, plot `func`
  assignments, draw-in text routing, equation highlight parent resolution, and
  primitive family classification all consult registry capabilities instead of
  growing with `ActorKindId` matches.
- GUI integration pass: `animatix-gui` enables `plugin-loading` by default,
  installs sibling manifests/libraries into each document's extension context,
  and feeds the merged manifest into editor completions/hover.
- Render completeness pass: native image commands resolve URLs from the
  timeline asset cache, and path/text/image/highlight commands all render
  through the normal `RenderCommand` pipeline.

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

Implemented: action target expansion, plot `func` dispatch, draw-in text
handling, highlight equation parent lookup, and primitive family classification
now read registry capabilities.

Replace remaining `ActorKindId`/`PrimitiveDescriptor` string matches with
descriptor capabilities and child-processing strategies. Remove the static
`PRIMITIVES`-specific call sites.

Acceptance:

- Adding a built-in or extension primitive requires only one registration path.
- `scene_eval.rs` does not grow with primitive count.

### Phase 5: Unstable Native ABI

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
