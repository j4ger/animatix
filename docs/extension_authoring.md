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
registry instead of a core `ActorKindId` variant.

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

## Current Limits

- Custom primitives can build and evaluate through the registry. A few child
  processing paths (`Filter`, `Mask`, `Equation`) still special-case built-in
  containers instead of using a generic capability hook.
- Extension properties are descriptor-driven. Built-in property metadata is
  still split between the runtime `PROPERTY_REGISTRY` and
  `animatix-syntax::schema`, so a new built-in property currently needs both
  tables.
- GUI builds use a per-document extension context. The insertion palette reads
  the timeline's primitive registry, and the inspector/keyframe table show
  extension properties from the actor plan. LSP does not load runtime extension
  contexts.
- CLI builds through `ExtensionContext`, but there is no dynamic plugin file
  loader yet; plugins must be compiled in and installed through
  `PluginLoader`.
