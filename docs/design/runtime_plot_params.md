# Runtime PlotCurve Parameters

## Goal

Resolve G6 by making `PlotCurve` function bodies see runtime parameter values during frame-time procedural re-sampling, so curves like `sin(freq * x)` can animate without duplicate curve declarations or cross-fades.

## Current State

`PlotCurve` already has the most important runtime machinery:

- `crates/animatix/src/timeline/plot.rs::ProceduralPlot` stores the plot kind, closure argument names, closure body AST, sampled domains, graph size, sampling quality, and stroke style.
- `crates/animatix/src/timeline/plot.rs::sample_procedural_plot()` receives a mutable `Environment`, binds the sampling variable (`x`, `t`, or `x`/`y` for implicit plots), evaluates `func_body`, and builds fresh `VelloPath`s.
- `crates/animatix/src/timeline/scene_eval.rs::render_actor_node()` detects `track.procedural_plot`, clones the current frame environment, and calls `sample_procedural_plot()` instead of using static `track.vector_paths`.
- `always` blocks execute before node rendering. Runtime assignments write into both `overrides` and the current frame environment via `frame_env::apply_override_incremental()`.
- `Expr` lookup already resolves identifiers and dotted paths from `Environment`, including flat dotted keys like `curve.freq`.

The missing piece is not the sampler. It is environment shape and policy:

1. `build_frame_env_internal()` injects `t`, scene dimensions, and keyframe-scoped variable tracks unconditionally.
2. It injects actor property lookup keys (`curve.opacity`, `curve.size.x`, etc.) only when modifiers exist.
3. A procedural plot with no `always` block can therefore get a frame env that lacks actor property keys, even though `needs_frame_env()` is true because `has_procedural_plots()` is true.
4. Unknown custom actor properties like `freq` are not currently stored as normal `AnimationTrack` fields, so `curve.freq = ...` cannot naturally keyframe unless the property is either registered, captured as plot params, or stored in a generic per-actor custom property map.
5. A closure reference to bare `freq` only works if `freq` is in the frame environment as a top-level variable. A same-actor property currently appears as `curve.freq`, not as bare `freq`.

That means these two syntaxes have different implementation requirements:

```animatix
#0s
let freq = 2
curve: PlotCurve, kind: "cartesian", func: (x) => sin(freq * x)

#3s
let freq = 5
```

This should already be close, because keyframe-scoped variables are injected into the frame environment.

```animatix
#0s
curve: PlotCurve, kind: "cartesian", func: (x) => sin(freq * x), freq: 2

#0s
curve.freq = 5 [3s, ease: ease-in-out]
```

This needs actor-local parameter storage and a scoped injection step before sampling.

## Option A: Closure Captures Env Vars

Option A keeps `func` as the only plot function mechanism and makes frame-time sampling inject same-actor parameter values into the closure environment.

### Syntax

Top-level variable capture:

```animatix
#0s
let freq = 2
curve: PlotCurve,
  kind: "cartesian",
  func: (x) => sin(freq * x),
  stroke: accent.primary

#3s
let freq = 5
```

Reactive capture:

```animatix
#0s
let freq = 2
curve: PlotCurve,
  kind: "cartesian",
  func: (x) => sin(freq * x),
  stroke: accent.primary

always {
  freq = 2 + 3 * sin(t * 0.5)
}
```

Same-actor property capture:

```animatix
#0s
curve: PlotCurve,
  kind: "cartesian",
  func: (x) => sin(freq * x),
  freq: 2,
  stroke: accent.primary

#0s
curve.freq = 5 [3s, ease: ease-in-out]
```

Explicit dotted property capture remains valid and avoids local-name ambiguity:

```animatix
#0s
curve: PlotCurve,
  kind: "cartesian",
  func: (x) => sin(curve.freq * x),
  freq: 2,
  stroke: accent.primary
```

### Semantics

For a `PlotCurve` actor named `curve`, frame-time sampling should evaluate the closure in this lookup order:

1. Closure sample bindings (`x`, `theta`, `u`, or `x`/`y`) shadow everything during a sample.
2. Actor-local plot params are available as bare names (`freq`) for this plot only.
3. Frame environment values remain available (`t`, keyframe `let` variables, `scene_width`, `accent.primary`, etc.).
4. Actor property dotted keys remain available (`curve.freq`, `curve.opacity`, `graph.size.x`).

If a plot param name conflicts with the closure sample arg, the sample arg wins and a build diagnostic should warn that the param is shadowed.

### Concrete Changes

`crates/animatix/src/timeline/plot.rs`:

```rust
pub struct ProceduralPlot {
    pub kind: PlotCurveKind,
    pub func_args: Vec<String>,
    pub func_body: Expr,
    pub actor_label: String,
    pub param_names: Vec<String>,
    // existing domains/style fields...
}
```

Keep `sample_procedural_plot(plot, env)` as the sampler entry point. Before sampling begins, inject actor-local params into `env`:

```rust
for name in &plot.param_names {
    if let Some(value) = env.get(&format!("{}.{}", plot.actor_label, name)) {
        env.set(name, value);
    }
}
```

This preserves the existing recursive sampler and only changes the environment it sees.

`crates/animatix/src/timeline/scene_eval.rs`:

```rust
if let Some(procedural_plot) = track.procedural_plot.as_ref() {
    let mut local_env = if let Some(env) = frame_env {
        env.clone()
    } else {
        self.build_frame_env_internal(time_ms, scene_dimensions, overrides)
    };
    self.inject_plot_actor_params(node_label, track, time_ms, node_overrides, &mut local_env);
    vector_paths = sample_procedural_plot(procedural_plot, &mut local_env);
}
```

The helper can live in `frame_env.rs` or `plot.rs`; it should use the same value/sub-key rules as `apply_override_incremental()`.

`crates/animatix/src/timeline/frame_env.rs`:

Split the current fast path so procedural plots can request actor property injection even without modifiers:

```rust
let has_modifiers = !self.modifier_programs.is_empty() || !self.modifiers.is_empty();
let has_procedural_plots = self.has_procedural_plots();

if !has_modifiers && !has_procedural_plots {
    return env;
}

self.inject_runtime_lookup_values(&mut env, time_ms, Some(scene_dimensions), Some(overrides));
```

If full actor injection is too expensive for plots, add a narrower path:

```rust
if has_procedural_plots && !has_modifiers {
    self.inject_procedural_plot_lookup_values(&mut env, time_ms, overrides);
    return env;
}
```

`crates/animatix/src/timeline/build/plot.rs` and `build/property.rs`:

Collect plot param names from declaration properties that are not built-in plot/render/layout properties. Initially this should be conservative: only numeric properties on `PlotCurve` with names not in `PROPERTY_REGISTRY` and not in `func_args`.

```rust
curve: PlotCurve,
  kind: "cartesian",
  func: (x) => sin(freq * x),
  freq: 2
```

During build, this should either:

- store `freq` in a new generic `custom_properties` map on `AnimationTrack`, or
- create a typed `plot_params` map on `ProceduralPlot` plus matching keyframe storage.

The second option is smaller for G6; the first is more reusable.

`crates/animatix/src/timeline/assignments.rs`:

When an assignment targets an unknown property on a `PlotCurve`, check whether it is a declared plot param. If yes, keyframe it as a numeric custom/plot param instead of emitting `UnsupportedAssignmentProperty`.

```animatix
#0s
curve.freq = 5 [3s, ease: ease-in-out]
```

This should produce an interpolated value in the frame env as both `curve.freq` and, during sampling only, `freq`.

### Complexity

Low to medium.

The minimal version can support top-level `let freq` and `always { freq = ... }` by fixing frame-env injection and documenting the supported pattern. Supporting same-actor `freq:` declarations and `curve.freq = ...` needs one small generic/custom property track mechanism.

### Edge Cases

- Bare param collision with closure sample arg: `(freq) => sin(freq * x)` should not allow actor param `freq` to override the sample arg.
- Multiple curves using `freq`: a bare `freq` top-level variable intentionally affects all curves; actor-local `freq` should affect only the owning curve.
- Dotted names cannot be closure parameters, so `func: (x) => sin(curve.freq * x)` remains explicit and unambiguous.
- `always { curve.freq = ... }` must update the frame env before plot sampling; the current modifier execution order already supports this.
- Keyframed custom params should interpolate only numeric values at first. Non-numeric params should be rejected or treated as discrete future work.
- Static subtree/frame cache must treat procedural plots as dynamic; `has_procedural_plots()` already makes `is_static_subtree()` conservative.
- Plot path cache should not be reused across frames for procedural plots unless the cache key includes all param values.

## Option B: Property-Driven `params:` Mechanism

Option B adds explicit plot parameter syntax so the function signature declares every runtime parameter.

### Syntax

```animatix
#0s
curve: PlotCurve,
  kind: "cartesian",
  func: (x, freq) => sin(freq * x),
  params: (freq: 2),
  stroke: accent.primary

#0s
curve.params.freq = 5 [3s, ease: ease-in-out]
```

A shorter assignment form could be allowed as sugar:

```animatix
#0s
curve.freq = 5 [3s, ease: ease-in-out]
```

Multi-param example:

```animatix
#0s
signal: PlotCurve,
  kind: "cartesian",
  func: (x, amp, freq, phase) => amp * sin(freq * x + phase),
  params: (amp: 1, freq: 2, phase: 0)

#2s
signal.params.amp = 0.5 [1s]
signal.params.phase = PI [1s]
```

### Semantics

`func` has one domain/sample argument followed by named runtime params.

For `cartesian` and `polar`, the first function arg is the domain variable. For `parametric`, the first arg is the curve parameter. For `implicit`, the first two args are domain variables. Any remaining args must match `params` keys by name and order.

At sample time, the sampler binds both the domain args and evaluated param values:

```rust
env.set_binding(domain_arg, Value::Num(x));
for (name, value) in evaluated_params {
    env.set(name, value);
}
evaluate_expr(&plot.func_body, env)
```

Because `Environment::bindings` currently has only two slots, param binding should not use `set_binding()`. Use `env.set()` on a per-plot cloned environment before entering the sampling loop, and reserve bindings for rapidly changing sample variables.

### Concrete Changes

`crates/animatix/src/ast.rs` and parser modules:

No new expression type is required if `params: (freq: 2)` is already representable as a tuple/object-like value. If named tuple syntax is not currently supported, either use an object construct:

```animatix
params: PlotParams { freq: 2, amp: 1 }
```

or add parser support for named tuple fields.

`crates/animatix/src/timeline/plot.rs`:

```rust
pub struct ProceduralPlot {
    pub kind: PlotCurveKind,
    pub domain_args: Vec<String>,
    pub param_args: Vec<String>,
    pub func_body: Expr,
    pub params: BTreeMap<String, PropertyTrack<Value>>,
    // existing fields...
}
```

A better typed first implementation is numeric-only:

```rust
pub params: BTreeMap<String, PropertyTrack<f64>>,
```

`sample_procedural_plot()` receives `time_ms` or pre-evaluated params:

```rust
pub fn sample_procedural_plot(
    plot: &ProceduralPlot,
    env: &mut Environment,
    time_ms: u64,
) -> Vec<VelloPath>
```

`crates/animatix/src/timeline/build/plot.rs`:

Parse `params`, validate that every non-domain function arg appears exactly once in `params`, and seed param tracks.

`crates/animatix/src/timeline/assignments.rs`:

Route `curve.params.freq = ...` and optionally `curve.freq = ...` to the param track.

`crates/animatix-analyzer` and `tree-sitter-animatix`:

Update completion/highlighting if new named tuple/object syntax is introduced. If existing object syntax is reused, only semantic completion needs updates.

### Complexity

Medium to high.

This is cleaner and more explicit than Option A, but it touches parser/analyzer/syntax if `params:` requires new syntax. It also changes plot function arity validation, assignment routing, and frame-time sampling signatures.

### Edge Cases

- Function arity validation must distinguish domain args from param args for all plot kinds.
- Parameter order must be deterministic. Prefer names over positional lookup, with declaration order retained only for diagnostics/source round-tripping.
- `params` values should be numeric-only first. Colors, vectors, and strings need interpolation/discrete semantics.
- `params` should not conflict with existing actor properties like `stroke_width` or `opacity` unless explicitly namespaced under `params`.
- `params` should work for implicit plots where the first two args are `x` and `y`.
- Keyframing `curve.params.freq` requires dotted assignment targets beyond the current actor/property pair model, or a special-case path parser.

## Option C: Keyframe Curve Data

Option C treats the function body itself as keyframeable curve data.

### Syntax

```animatix
#0s
curve: PlotCurve,
  kind: "cartesian",
  func: (x) => sin(2 * x),
  stroke: accent.primary

#3s
curve: PlotCurve,
  kind: "cartesian",
  func: (x) => sin(5 * x) [1s, strategy: morph]
```

Or assignment syntax:

```animatix
#3s
curve.func = (x) => sin(5 * x) [1s]
```

### Semantics

There are two plausible interpretations:

1. Discrete function switch: at the keyframe, replace the closure body and re-sample. This is simple but does not animate the curve shape.
2. Morph sampled paths: sample old and new functions into paths, then use existing path morphing to interpolate the resulting geometry over the assignment duration.

The second interpretation is what users expect from `[1s]`, but it is not really animating the function; it is animating between two sampled geometries.

### Concrete Changes

`crates/animatix/src/timeline/property_registry.rs`:

Make `func` assignable for `PlotCurve`, but probably not generally animated:

```rust
schema!("func", ValueType::BuildTimeOnly, F::ASSIGNABLE, ...)
```

`crates/animatix/src/timeline/assignments.rs`:

Special-case `curve.func = closure` because generic `PropertyTrack<T>` cannot interpolate `Expr` closures.

`crates/animatix/src/timeline/plot.rs`:

Represent function keyframes:

```rust
pub struct ProceduralPlotKeyframe {
    pub time_ms: u64,
    pub func_args: Vec<String>,
    pub func_body: Expr,
}

pub struct ProceduralPlot {
    pub func_keyframes: BTreeMap<u64, ProceduralPlotKeyframe>,
    // existing domains/style fields...
}
```

`crates/animatix/src/timeline/scene_eval.rs`:

At time `t`, choose the active function for discrete mode. For morph mode, sample both bracketing functions and pass their paths through `timeline/morph.rs`.

### Complexity

High.

This option touches function storage, assignment validation, morphing, diagnostics, and potentially caching. It also creates subtle semantics around discontinuities and function topology changes.

### Edge Cases

- Different function bodies can produce different path topology, numbers of subpaths, or discontinuities.
- Adaptive sampling may produce different segment counts frame-to-frame, causing unstable morphs unless resampling is normalized.
- Morphing implicit plots is expensive because each function sample is grid-based.
- Re-declaring `func` while also animating domains, size, or params requires a clear precedence model.
- `func` closures cannot interpolate directly; any smooth transition is geometry morphing, not semantic function interpolation.

## Implementation Analysis

### Option A Files

- `crates/animatix/src/timeline/frame_env.rs` — ensure procedural plots get actor lookup values when needed; optionally add a narrow plot-param injection path.
- `crates/animatix/src/timeline/plot.rs` — add `actor_label` and `param_names` to `ProceduralPlot`; inject actor-local params before sampling.
- `crates/animatix/src/timeline/build/plot.rs` — populate `ProceduralPlot` metadata and collect declared param names.
- `crates/animatix/src/timeline/track.rs` — optionally add `plot_params` or `custom_properties` if same-actor `freq:` declarations are supported.
- `crates/animatix/src/timeline/assignments.rs` — route `curve.freq = ...` to plot/custom param tracks.
- `crates/animatix/src/timeline/tests.rs` — add tests for top-level variable capture, same-actor param capture, and `always` override capture.
- `docs/spec.md` — document supported `PlotCurve` runtime parameter style.

Complexity: low for top-level/env capture; medium for actor-local keyframed params.

### Option B Files

- `crates/animatix/src/parser` / AST modules — only if named `params` syntax is new.
- `crates/animatix/src/timeline/build/plot.rs` — parse and validate `params` against closure args.
- `crates/animatix/src/timeline/plot.rs` — store and sample param tracks.
- `crates/animatix/src/timeline/assignments.rs` — support `curve.params.freq` assignment.
- `crates/animatix-analyzer` — completion and diagnostics for `params` keys.
- `tree-sitter-animatix` — highlighting if syntax changes.
- `docs/spec.md` and examples — document the new mechanism.

Complexity: medium/high.

### Option C Files

- `crates/animatix/src/timeline/property_registry.rs` — make `func` assignable or add special assignment metadata.
- `crates/animatix/src/timeline/assignments.rs` — parse closure assignments and timed behavior.
- `crates/animatix/src/timeline/plot.rs` — support function keyframes and sampling selected/bracketing functions.
- `crates/animatix/src/timeline/morph.rs` — reuse/extend path morphing for sampled function outputs.
- `crates/animatix/src/timeline/scene_eval.rs` — select discrete or morphing render behavior at frame time.
- `docs/spec.md` and examples — explain that timed function changes are geometry morphs.

Complexity: high.

## Recommendation

Implement Option A first, in two increments.

### A1: Environment Capture

Support and document top-level/keyframe/`always` variable capture:

```animatix
#0s
let freq = 2
curve: PlotCurve, kind: "cartesian", func: (x) => sin(freq * x)

always {
  freq = 2 + 3 * sin(t * 0.5)
}
```

This uses existing `variable_tracks`, modifier execution, and `Expr` lookup. The key fix is ensuring procedural plot sampling receives the fully useful frame env. Verification:

```bash
cargo test -p animatix runtime_plot_params
cargo test -p animatix test_keyframe_scoped_variables_injected_into_frame_env
```

### A2: Actor-Local Plot Params

Add numeric plot params as same-actor custom properties:

```animatix
#0s
curve: PlotCurve,
  kind: "cartesian",
  func: (x) => sin(freq * x),
  freq: 2

#0s
curve.freq = 5 [3s, ease: ease-in-out]
```

Store only declared numeric params on `PlotCurve` at first. Inject them as both `curve.freq` and bare `freq` during sampling for that actor. This keeps the authoring surface elegant and avoids a new syntax form.

### Future Work

Add Option B only if plots need many structured parameters, analyzer-visible param metadata, or non-numeric params. Keep Option C as a separate morphing feature; it solves a different problem and should not block runtime parameter animation.
