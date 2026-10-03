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

The current unstable ABI snapshot is 9 and has exactly one install entry. The
snapshot is not a compatibility version: plugins must be rebuilt from the same
source tree as the host whenever it changes. It can register
external properties with full tooling metadata, native expression functions,
primitives, actions, service values with optional destructors, and
plugin-authored post-processing effects. Native
primitive descriptors carry `NATIVE_CAP_*` capability flags, declared property
names, a `NATIVE_RESIZE_MODE_*` value so the GUI, actions, and generic
property writer can route them without string matching. Native primitives have
optional `build`, `evaluate`, `handle_assignment`,
`finalize_container_build`, `clip_path`, and `equation_fragment` callbacks. The
host builds children through the same
timeline path as built-ins and then calls finalize, so native containers no
longer need to fake their way through a built-in `ActorKindId`. Extension
tracks use the neutral `ActorKindId::Extension`; build, assignment, and frame
evaluation resolve them through `actor_type` and the active primitive registry.
`clip_path` lets a native primitive act as a `Mask`'s `clip_shape` by emitting
path commands through `NativeClipPathCtx::append_path`; `equation_fragment`
lets it contribute content and highlight styling to a parent `Equation` through
`NativePrimitiveEquationFragmentCtx`. Both are optional: omitting them (or
returning `NATIVE_STATUS_UNSUPPORTED`) opts the primitive out, matching the
built-in defaults.
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

### Easing codes

Easing crosses the boundary as a single `u32`, in both directions: the host
reports the assignment's curve in `NativeAssignmentContext::easing`, and a
plugin names the curve it wants as `write_keyframe`'s last argument.

| Code | Constant | Source name |
|---|---|---|
| 0 | `NATIVE_EASING_LINEAR` | `linear` |
| 1 | `NATIVE_EASING_IN` | `ease-in` |
| 2 | `NATIVE_EASING_OUT` | `ease-out` |
| 3 | `NATIVE_EASING_IN_OUT` | `ease-in-out` |
| 4 | `NATIVE_EASING_BOUNCE_IN` | `bounce-in` |
| 5 | `NATIVE_EASING_ELASTIC` | `elastic` |
| 6 | `NATIVE_EASING_BACK` | `back` |
| 7 | `NATIVE_EASING_EXPO` | `expo` |
| 8 | `NATIVE_EASING_EXPO_OUT` | `expo-out` |
| 9 | `NATIVE_EASING_EXPO_IN_OUT` | `expo-in-out` |
| `u32::MAX` | `NATIVE_EASING_UNSUPPORTED` | — (not a curve) |

A code outside the table is rejected with `NATIVE_STATUS_TYPE_ERROR` rather than
applied as linear. `spring(damping, frequency)` and
`cubic-bezier(p1x, p1y, p2x, p2y)` carry arguments one `u32` cannot hold, so the
host reports `NATIVE_EASING_UNSUPPORTED` for them instead of naming a different
curve; a plugin must read that as "no curve was specified", not as a curve to
interpolate. Giving the parameterized curves codes needs a payload slot beside
the `u32`, which is an ABI bump.

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

### Plugin-authored effects (ABI 9)

A native plugin can author post-processing effects by shipping **WGSL source
text and a parameter schema** — never a GPU handle. The host compiles and runs
the shader with its own device, owns every texture, and marshals parameters
into the declared uniform layout, so no GPU type ever crosses the FFI
boundary.

Register effects from `animatix_plugin_install` through
`NativePluginApi.register_effect(host, NativeEffectDescriptor)`:

- `name` is the authored type name (`soft: Pixelate` inside a `Filter` scope).
  It must not collide with a built-in effect or a previously registered plugin
  effect; duplicates are rejected.
- `params` are declared at their **uniform offsets verbatim**: scalars and
  booleans must be 4-byte aligned, `vec2` 8-byte aligned, `vec4` 16-byte
  aligned. The host computes the uniform buffer size (largest offset + size,
  padded to 16) and rejects misaligned declarations. Each parameter declares
  an `identity` value — when every parameter of a stage is at identity the
  host skips the stage entirely, so a pass-through default costs nothing.
- `passes` are ordered compute passes, each with its own WGSL text and entry
  point (conventionally `main`). The binding layout is fixed
  (`docs/effects.md` §4): input texture, output storage texture, author
  parameters, host `EffectContext`, and a linear sampler.
- `support_px` is the conservative spatial support used to pad regions of
  interest.

Validation and trust: the uniform layout is validated at registration
(misaligned or unknown-kinded parameters reject the effect with a
`NATIVE_STATUS_TYPE_ERROR`); WGSL itself is compiled at first render, where
errors surface as runtime diagnostics and the stage is skipped. wgpu validates
syntax but not termination — effects are **trusted-authoring**, not a
sandbox, and they are **GPU-only** (no backend means the stage is skipped with
a warning, exactly like built-ins).

Every string handed to `register_effect` must outlive the call; leaking them
is expected (registration happens once per plugin load, so the allocation is
bounded by the number of effects). Rollback of a partially failed install
unregisters the plugin's effects.

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
