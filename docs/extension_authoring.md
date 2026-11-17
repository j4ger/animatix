# Extension Authoring

Built-in, user-defined, and extension callables share one signature vocabulary
and argument binder. Built-ins and extensions publish `CallableDescriptor`s;
user functions derive the same parameter metadata from their declarations.
Timeline callables construct inert plans; only the host lowers those plans into
a scene. Extensions may also register primitives, properties, services, and GPU
effects.

## Extension Context

`ExtensionContext` holds in-process extension capabilities for a build.
Callable descriptors are the single contract used by argument binding,
validation, hover, completion, and runtime dispatch.

```rust
let mut ctx = ExtensionContext::new();
ctx.register_primitive(Arc::new(MyPrimitive))?;

let descriptor = CallableDescriptor {
    namespace: "demo".into(),
    name: "double".into(),
    params: vec![CallableParam::required("value", CallableType::Num)],
    return_type: CallableType::Num,
    phase: CallablePhase::BuildAndFrame,
    help: "Double a number.".into(),
    category: "Math".into(),
};
ctx.register_callable(descriptor, |args, _env| match args.first() {
    Some(Value::Num(value)) => Ok(Value::Num(value * 2.0)),
    _ => Err(EvalError::TypeMismatch("double expects a number".into())),
})?;
ctx.provide("theme", my_theme);

let report = Timeline::build_with_context(&ast, &namespaces, Arc::new(ctx));
```

A callable is invoked as `demo::double(21)` or
`demo::double(value: 21)`. Positionals must precede named arguments. Timeline
functions return `Value::TimelinePlan`; they must not mutate a `World`. An
explicit result type is checked against the produced value, and phase comes from
the descriptor rather than from source return annotation syntax. Use
`remove_callable("demo::double")` when unloading an in-place registration.

Module-backed builds that contain component methods should keep the expansion
metadata with the statements. `LoadedProgram::expand_components` returns an
`ExpandedProgram`; pass its `statements` and call
`BuildOptions::with_expanded(&expanded)` when building. Apply it after
`with_function_context` because it supplies the context compiled over expanded
statements. That metadata retains instance methods at the program root and
separate method registries for each scene, so composition never merges equal
instance labels from different scenes. Method templates keep their definition
module and indexed-instance binding for ordinary function lookup. Plan
expressions carry their own local instance method scopes. These tables support
compilation and resource lookup; they do not add per-entity method schedulers
to the runtime timeline.

## Primitive Registry

`PrimitiveRegistry` layers custom primitives over the built-in static set.

```rust
let mut registry = PrimitiveRegistry::new();
registry.register(Arc::new(MyPrimitive))?;

let report = Timeline::build_with_options(
    &ast,
    &namespaces,
    BuildOptions::default().with_primitive_registry(Arc::new(registry)),
);
```

A custom primitive implements `Primitive`. Its authored `actor_type` lets the
scene evaluator resolve it through the active registry. Container primitives
choose a child-processing strategy such as `Generic`, `Filter`, `Mask`, or
`Equation`.

## Extension Properties and Plans

Register a property with its host storage kind and frame-environment visibility:

```rust
ctx.register_property(
    "Gauge",
    "level",
    animatix_syntax::schema::PropertyValueKind::F32,
    true,
)?;
```

Authored declarations and transitions such as `g: Gauge, level: 42` and
`g.level = 80 [1s]` lower into the actor's `PropertyPlan`. Injectable
properties are also available in `always` expressions. Timeline callable
results are separate immutable plans that the host mounts into the current
scene; the plan stores local actor references and operations until mount time.

## In-process Plugins

`ExtensionPlugin` installs capabilities and returns a disposer. Register
callables with `register_callable(CallableDescriptor, callback)`; registration,
completion, and execution use the same descriptor. `PluginLoader` provides
`list`, `get`, `replace`, `replace_shared`, and `remove` lifecycle
operations. A disposer removes its registrations during in-place unload or
partial-failure rollback. When a host atomically replaces the entire context,
it drops the old context instead of invoking its disposer against the new one.

## Native Plugins

The `plugin-loading` feature adds a native `cdylib` loader. Host and plugin
exchange `repr(C)` structs and function pointers from
`crates/animatix-plugin-api`; no Rust trait objects cross the boundary. A plugin
exports `animatix_plugin_abi_version`, `animatix_plugin_name`, and
`animatix_plugin_install`.

The current unstable ABI snapshot is **10**. It is not a compatibility version:
rebuild every plugin against the same source tree as the host. This snapshot
replaces the separate native action and expression-function registration paths
with one `register_callable` entry and one manifest table, `[[callables]]`.
The old `[[actions]]` and `[[functions]]` manifest fields and action callback
API are removed; there is no compatibility adapter.

A callable descriptor provides `namespace`, `name`, ordered `params`,
`return_type`, `phase`, `help`, and `category`. Each parameter provides
`name`, `type`, an optional literal `default`, `optional`, `variadic`, and
`help`. The namespace `std` is reserved. Standard callables are prelude
visible and can also be explicitly called as `std::name(...)`; extension
callables must use their registered namespace, for example `demo::throb(...)`.

A manifest declaration uses the same descriptor shape:

```toml
[[callables]]
namespace = "demo"
name = "throb"
return_type = "Timeline"
phase = "Build"
category = "Effects"
help = "Return a glow pulse plan."

[[callables.params]]
name = "target"
type = "Actor"

[[callables.params]]
name = "duration"
type = "Duration"
default = 0.5
optional = true

[[callables.params]]
name = "strength"
type = "Num"
default = 1.0
optional = true
help = "Peak glow."
```

The host binds positional and named arguments into descriptor order before the
callback runs. `BuildAndFrame` callables must be pure value calculations;
`Timeline` results are build-only. The runtime validates each descriptor with
the same contract checker used for built-ins and rejects duplicate, malformed,
or mismatched signatures before registration.

### Native timeline plans

A native timeline callback returns a host-owned inert `TimelinePlan`. The
callback cannot access or mutate a `World`. The host supplies a
`NativeCallableContext` with read-only `get_env` and `get_service`,
`get_property_id` for a property registered by the plugin, and
`append_property_op` to record an operation.

`NativeValue` actor handles are opaque and valid only for that callback
invocation. They are issued for actor arguments (and actor values read from the
invocation environment); the plan writer rejects handles from a different call.
An omitted optional argument without a default arrives with the
`NATIVE_VALUE_NULL` tag; this is distinct from a callable result with the
`NATIVE_VALUE_UNIT` tag. Durations are represented in seconds. Each appended
operation names an actor
handle, the plugin property id, a value, local start and end seconds, and a
`NativeEasing` payload. The host validates that the property belongs to the
plugin and that the value and times match its type and range, then records the
operation in a local `TimelinePlanBuilder`. It never writes a track during the
callback. If any append fails, the host discards the complete returned plan,
even when the plugin ignores the error status.

A successful timeline callback sets its output tag to `NATIVE_VALUE_TIMELINE`;
the host owns the actual plan value and mounts it only after the call completes.
Other callbacks return ordinary tagged values. ABI tags include `Num`, `Bool`,
`U32`, vectors, `Color`, `String`, `List`, `Duration`, `Actor`,
`Timeline`, `Unit`, `Easing`, and `Null`. `NATIVE_VALUE_NULL` represents an
omitted optional argument or explicit null; `NATIVE_VALUE_UNIT` represents a
result with no value.
`NativeEasing` carries both a curve code and its parameters, including spring
and cubic Bezier curves.

The initial native plan writer is intentionally narrow: it records timed
operations on properties registered by the calling plugin. It does not expose
`World`, existing engine tracks, or a general entity handle. The host lowers a
validated plan into the current scene after the callback returns.

Primitive callbacks remain a distinct capability: `build`, `evaluate`,
`handle_assignment`, `finalize_container_build`, `clip_path`, and
`equation_fragment` operate through their primitive-specific host contexts.
The primitive assignment writer may update an extension property track. The
callable plan writer has no track or `write_keyframe` API. Native primitives
use `NATIVE_CAP_*` flags, declared property names, and
`NATIVE_RESIZE_MODE_*`; extension tracks keep the neutral
`ActorKindId::Extension` identity and resolve through the active registry.

Evaluation callbacks can read properties and services and append paths, text,
images, or highlights to host-owned render output. `append_text` chooses
`Text`, `Code`, or `Typst` with a `NATIVE_TEXT_KIND_*` value. Image URLs
resolve from the timeline's asset cache; explicit uncached URLs fail rather than
silently falling back to the actor image.

### Native demo and manifests

The in-repo plugin at `crates/animatix-plugin-demo` demonstrates a `Pulse`
primitive, extension properties, a `demo::scale(value, factor)` value
callable, a `demo::throb(target, duration, strength)` inert plan callable,
a typed service, and a plugin-authored `Pixelate` effect. Its source example is
`examples/projects/plugin_pulse.amx`.

`animatix plugin describe` reads the runtime descriptors and emits the same
manifest schema used by the analyzer and LSP:

```sh
cargo build -p animatix-plugin-demo
animatix plugin describe target/debug/libanimatix_plugin_demo.so \
  --output crates/animatix-plugin-demo/demo.amx-plugin.toml
animatix check examples/projects/plugin_pulse.amx \
  --plugin crates/animatix-plugin-demo/demo.amx-plugin.toml
```

Manifest property ids stay unset until runtime registration allocates them.
Property type strings can use primitive kinds or enum declarations such as
`Enum(left, right, top)`; enum values can be bare identifiers or quoted
strings in AMX source.

### Plugin-authored effects

A plugin can ship WGSL source and a parameter schema, never a GPU handle. The
host owns device, textures, and parameter marshalling. Effects declare aligned
uniform offsets, identity values, ordered compute passes, and conservative
`support_px`. The host validates layout at registration and compiles WGSL at
first render. These are trusted-authoring shaders, not a sandbox, and are GPU
only. Registration strings must outlive the registration call; rollback removes
effects owned by the failed plugin install.

## Current Limits

- Built-in property metadata is descriptor-driven in `animatix-syntax::schema`,
  while `PROPERTY_REGISTRY` is the typed runtime binding table. Both are
  protected by drift checks.
- GUI plugin contexts are per document and swapped atomically after a successful
  reload. The editor feeds merged manifests to the analyzer; LSP uses the same
  manifest discovery and stays independent of the runtime engine.
- CLI accepts `--plugin` manifests and native libraries. The analyzer and LSP
  consume descriptor metadata without loading native code.
