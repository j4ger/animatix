# Extension Authoring

This document describes the current extension surface. Built-ins keep typed hot
paths internally, but external primitives, properties, actions, functions, and
services go through the same registry/descriptor path used by the demo plugin.

## Extension Context

`ExtensionContext` is the per-build container for:

- custom primitives
- timeline functions (`fn` without a return type; formerly `action`)
- native expression functions
- typed services

```rust
let mut ctx = ExtensionContext::new();

ctx.register_primitive(Arc::new(MyPrimitive))?;
ctx.register_action(Box::new(MyAction));
ctx.register_function("double", |args, _env| {
    let Some(Value::Num(n)) = args.first() else {
        return Err(EvalError::TypeMismatch("double expects one number".into()));
    };
    Ok(Value::Num(n * 2.0))
});
ctx.provide("theme", my_theme);

let report = Timeline::build_with_context(&ast, &namespaces, Arc::new(ctx));
```

## Primitive Registry

`PrimitiveRegistry` layers custom primitives over the built-in static set.

```rust
let mut registry = PrimitiveRegistry::new();
registry.register(Arc::new(MyPrimitive))?;

let report = Timeline::build_with_primitive_registry(&ast, &namespaces, Arc::new(registry));
```

A custom primitive implements the existing `Primitive` trait. The `actor_type`
stored on `AnimationTrack` lets `scene_eval` resolve it back through the
registry instead of a core `ActorKindId` variant. Container primitives can
override `child_processing()` when their subtree renderer needs a dedicated
strategy (`Generic`, `Filter`, `Mask`, or `Equation`).

## Extension Properties

External properties are registered through the same context as primitives and
actions:

```rust
ctx.register_property(
    "Gauge",
    "level",
    animatix_syntax::schema::PropertyValueKind::F32,
    true, // injectable into frame environments
)?;

let report = Timeline::build_with_context(&ast, &namespaces, Arc::new(ctx));
```

Once registered, declarations, assignments, and keyframes such as
`g: Gauge, level: 42` and `g.level = 80 [1s]` are stored in the actor's
`PropertyPlan` without string lookup at frame time. Injectable properties are
also available to `always` blocks as `g.level`.

## Property Plans

`PropertyPlan` and `DynTrack` are the registry-driven property storage
prototype. Public helpers are exposed from `timeline`:

```rust
let id = property_id("position").expect("position is registered");
write_property_plan_slot(&mut track, id, PropertyKind::Vec2, value, 0, 1000, Easing::Linear);
let value = read_property_plan_slot(&track, id, 500);
```

Extension properties can be created lazily:

```rust
track.property_plan.ensure_slot(PropertyId(9001), PropertyKind::String);
```

## Plugins

`ExtensionPlugin` lets a plugin install multiple capabilities and return a
disposer.

```rust
struct MyPlugin;

impl ExtensionPlugin for MyPlugin {
    fn name(&self) -> &'static str {
        "my-plugin"
    }

    fn install(&self, ctx: &mut ExtensionContext) -> Result<PluginDisposer, PluginError> {
        ctx.register_function("hello", |_args, _env| Ok(Value::Str("world".into())));
        Ok(Box::new(|ctx: &mut ExtensionContext| {
            ctx.remove_function("hello");
        }))
    }
}

let mut loader = PluginLoader::new();
loader.register(Box::new(MyPlugin));
let disposers = loader.install_all(&mut ctx)?;
```

`PluginLoader` also exposes `list()`, `get(name)`, `replace(name, plugin)`,
`replace_shared`, and `remove(name)` so the GUI/CLI can share lifecycle control
without reimplementing install/rollback logic.

Disposers are for in-place unload and partial-failure rollback. Hosts that
atomically replace the whole context, like `DocumentPluginManager`, must not
invoke old disposers against a new context; dropping the old context releases
its registered capabilities.

## Native Plugins

The `plugin-loading` feature in `animatix` adds a native `cdylib` loader. The
unstable in-repo ABI snapshot lives in `crates/animatix-plugin-api`; host and
plugin exchange only `repr(C)` structs and function pointers, so plugins do not
share Rust trait objects or internal runtime types with the host.

A plugin exports:

- `animatix_plugin_abi_version() -> u32`
- `animatix_plugin_name() -> *const c_char`
- `animatix_plugin_install(api, host) -> i32`

The current unstable ABI snapshot is 7 and has exactly one install entry. The
snapshot is not a compatibility version: plugins must be rebuilt from the same
source tree as the host whenever it changes. It can register
external properties with full tooling metadata, native expression functions,
primitives, actions, and service values with optional destructors. Native
primitive descriptors carry `NATIVE_CAP_*` capability flags, declared property
names, a `NATIVE_RESIZE_MODE_*` value so the GUI, actions, and generic
property writer can route them without string matching. Native primitives have
optional `build`, `evaluate`, `handle_assignment`, and
`finalize_container_build` callbacks. The host builds children through the same
timeline path as built-ins and then calls finalize, so native containers no
longer need to fake their way through a built-in `ActorKindId`. Extension
tracks use the neutral `ActorKindId::Extension`; build, assignment, and frame
evaluation resolve them through `actor_type` and the active primitive registry.
Evaluate callbacks receive a host context with `get_property`, `get_service`,
`append_path`, `append_text`, `append_image`, and `append_highlight`; the demo
primitive reads its keyframed `glow` property and emits paths, text, and a
highlight layer that render through the normal scene-evaluation path.
`append_text` takes a `NATIVE_TEXT_KIND_*` value so one primitive can choose
`Text`, `Code`, or `Typst` rendering. Native image commands can pass a URL that
is resolved from the timeline's cached image assets, or pass null to reuse the
actor's currently loaded image. Explicit URLs are normalized against the
document directory first and workspace root second; an explicit URL that is not
already cached returns an error instead of silently falling back to the actor
image. Native actions register full signatures and execute with targets, args,
modifiers, time, and a host `write_keyframe` API. The `write_keyframe` API (and
the primitive assignment callback) can only keyframe *extension* properties the
plugin itself registered; writing a built-in property returns
`NATIVE_STATUS_UNSUPPORTED` (the primitive assignment path then falls through to
the generic engine, while a native action surfaces it as a diagnostic). Native
functions receive a host context that can read frame-environment values and
services. Expression callbacks exchange `NativeValue` values: `Num`, `Bool`,
`U32`, `Vec2`, `Vec3`, `Vec4`, `Color`, `String`, and `List`. Objects, closures,
and native function values return a type error.

```bash
cargo build -p animatix-plugin-demo
animatix check demo.amx --plugin crates/animatix-plugin-demo/demo.amx-plugin.toml
animatix check demo.amx --plugin target/debug/libanimatix_plugin_demo.so
```

The in-repo demo plugin (`crates/animatix-plugin-demo`) showcases the whole
surface on one `Pulse` primitive: a keyframed `Num` property (`glow`), a
manifest-driven `Enum(ring, dot, cross)` property (`mode`, a dropdown in the
GUI inspector), `Str`/`Vec2` properties (`caption`, `origin`, `image_url`), a
`Text` + `Code` text pair, a highlight layer, a best-effort `append_image`
stamp resolved from the asset cache, a two-argument native function (`scale`),
a native action (`throb`) that writes a `glow` keyframe through
`write_keyframe`, and a typed service with a destructor. A runnable scene,
`examples/projects/plugin_pulse.amx`, uses the plugin and is gated by
`scripts/check_examples.sh` (which builds the plugin first and passes the
manifest to `check`).

A manifest passed to `--plugin` also feeds the analyzer, so unknown extension
types/properties are suppressed during `check` and `lint`. Manifest entries are
parsed into the shared `PrimitiveDescriptor`/`PropertyDescriptor` schema, so
completions and hover metadata use the same shapes as runtime tooling. Manifest
property descriptors keep `id: None`; runtime ids are allocated only when the
plugin or in-process extension registers into `ExtensionRegistry`. If the
manifest has a `library` field, the CLI loads that native library relative to
the manifest. Property `type` strings can use the primitive kinds (`Num`,
`Bool`, `Color`, `Vec2`, etc.) or an enum form such as
`Enum(left, right, top)`; enum properties render as manifest-driven dropdowns
in the GUI inspector and accept bare variant identifiers (`mode: ring`) as well
as quoted strings (`mode: "ring"`) in declarations and assignments.

Manifests can be regenerated from a native library instead of hand-maintained.
The CLI and GUI both use `animatix-plugin-tooling::generate_manifest_toml`,
which installs the library into a scratch `ExtensionContext`, reads its runtime
primitive/property descriptors, and serializes them through the same manifest
schema:

```bash
cargo build -p animatix-plugin-demo
animatix plugin describe target/debug/libanimatix_plugin_demo.so \
  --output crates/animatix-plugin-demo/demo.amx-plugin.toml
```

When `--output` is used, the recorded `library` field is made relative to the
manifest file; without it the manifest is printed to stdout.

```toml
library = "../../target/debug/libanimatix_plugin_demo.so"

[[primitives]]
type_name = "Gauge"

[[properties]]
actor_type = "Rect"
name = "glow"
type = "Num"
```

Primitives, actions, and services now share one native ABI path with
properties and functions. The host keeps each loaded `Library` alive through the
registered callbacks and the disposer returned by install.

## Current Limits

- Built-in property metadata is descriptor-driven in
  `animatix-syntax::schema`, while `PROPERTY_REGISTRY` is the typed runtime
  binding table (field, read source, flags, defaults). The two sources are
  intentionally split and protected by bidirectional drift guards, so a new
  binding must still be registered in both descriptor and binding layers.
- GUI builds use a per-document extension context managed by
  `DocumentPluginManager`. Discovery searches explicit plugin paths, the
  document directory, then the workspace root; the manager keeps a
  last-known-good context and atomically swaps candidates. Explicit plugin
  paths are persisted in workspace settings. Background rebuilds carry a plugin
  epoch, so results from an older context are discarded after a plugin reload.
  The background rebuild worker reuses the same context Arc instead of loading
  native libraries again. The plugin status dialog shows manifests, loaded
  libraries, capability counts, errors, reload controls, explicit paths, and a
  `plugin describe`-style manifest generator. The insertion palette includes
  extension actions, the inspector/keyframe table show extension properties
  from the actor plan, and the editor feeds the merged manifest to the
  analyzer, so completions and hover match the runtime plugin. LSP stays
  runtime-free and uses the same shared discovery module from the document
  directory.
- CLI accepts `--plugin` manifests and native libraries. Native plugins
  register properties, expression functions, primitives, actions, and services
  from a dynamic library; analyzer/LSP still derive static metadata from the
  manifest.
