# Extension Authoring

This document describes the current extension surface. It is intentionally
incremental: the registry-driven primitive/property architecture is being
migrated in phases, so not every built-in behavior is descriptor-driven yet.

## Extension Context

`ExtensionContext` is the per-build container for:

- custom primitives
- custom actions
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

Scoped registration is also available:

```rust
{
    let mut scope = ctx.scope();
    scope.register_action(Box::new(MyAction));
    // All scope registrations are disposed when `scope` drops.
}
```

## Native Plugins

The `plugin-loading` feature in `animatix` adds a native `cdylib` loader. The
stable ABI lives in `crates/animatix-plugin-api`; host and plugin exchange only
`repr(C)` structs and function pointers, so plugins do not share Rust trait
objects or internal runtime types with the host.

A plugin exports:

- `animatix_plugin_abi_version() -> u32`
- `animatix_plugin_name() -> *const c_char`
- `animatix_plugin_install_v1(api, host) -> i32` and/or
  `animatix_plugin_install_v2(api, host) -> i32`

ABI v1 can register external properties and native expression functions. ABI v2
adds `NativePrimitiveV2`, action callbacks, and service values with optional
destructors, so one native install path can register the same capability kinds
as an in-process `ExtensionPlugin`. Native evaluate callbacks receive a host
context with `append_path`; the demo primitive emits vector paths that render
through the normal scene-evaluation path. Expression callbacks exchange
`NativeValueV1` values: `Num`, `Bool`, `Vec2`, `Vec3`, `Vec4`, and `Color`.
Strings, lists, closures, and other runtime values return a type error.

```bash
cargo build -p animatix-plugin-demo
animatix check demo.amx --plugin crates/animatix-plugin-demo/demo.amx-plugin.toml
animatix check demo.amx --plugin target/debug/libanimatix_plugin_demo.so
```

A manifest passed to `--plugin` also feeds the analyzer, so unknown extension
types/properties are suppressed during `check` and `lint`. Manifest entries are
parsed into the shared `PrimitiveDescriptor`/`PropertyDescriptor` schema, so
completions and hover metadata use the same shapes as runtime tooling. Manifest
property descriptors keep `id: None`; runtime ids are allocated only when the
plugin or in-process extension registers into `ExtensionRegistry`. If the
manifest has a `library` field, the CLI loads that native library relative to
the manifest.

```toml
library = "../../target/debug/libanimatix_plugin_demo.so"

[[primitives]]
type_name = "Gauge"

[[properties]]
actor_type = "Rect"
name = "glow"
type = "Num"
```

Primitives, actions, and services now share one native ABI v2 path with
properties and functions. The host keeps each loaded `Library` alive through the
registered callbacks and the disposer returned by install.

## Current Limits

- Built-in property metadata is descriptor-driven in
  `animatix-syntax::schema`, while `PROPERTY_REGISTRY` is the typed runtime
  binding table (field, read source, flags, defaults). The two sources are
  intentionally split and protected by bidirectional drift guards, so a new
  binding must still be registered in both descriptor and binding layers.
- GUI builds use a per-document extension context. The insertion palette reads
  the timeline's primitive registry, and the inspector/keyframe table show
  extension properties from the actor plan. LSP does not load runtime extension
  contexts.
- CLI accepts `--plugin` manifests and native libraries. Native ABI v2
  registers properties, expression functions, primitives, actions, and services
  from a dynamic library; analyzer/LSP still derive most static metadata from
  the manifest.
