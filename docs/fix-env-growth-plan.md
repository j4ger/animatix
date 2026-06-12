# Fix Plan: Environment Growth During Plot Evaluation

## Problem Summary

`env_hash()` sorts all override entries on every `evaluate_expr()` call.
With 10+ actors and a Heatmap (resolution 28), the env grows to ~1989 entries,
each eval sorts them, and 784 cells × ~4 sub-expressions = ~3,136 sorts of
~1989 entries. That's O(n² log n) for what should be O(1) work.

Root causes (in order of impact):

1. **`evaluate_scalar_field()` clones the full env per cell** — each clone copies
   all 1989 entries, and `set(x)`/`set(y)` adds 2 more. 784 clones × 1989 entries
   = 1.56M `HashMap` inserts.

2. **`env_hash()` sorts all entries on every non-literal `evaluate_expr()`** —
   the cache key on every sub-expression sorts the full entry list.

3. **`inject_runtime_lookup_values()` injects every property for every actor**
   — including typed sub-keys (`.x`, `.y`, `.r`, `.g`, `.b`, `.a`) and
   `_animating_*` flags. Each Vec2 property costs 3 env entries; each Color
   costs 5; each `_animating_*` flag costs 1 more.

## Design Principles

- **Keep the registry pattern** — `PROPERTY_REGISTRY` remains the single
  source of truth. No ad-hoc property lists.
- **Framework-level fix** — changes go in the env/expression/registry layers,
  not in plot evaluation code.
- **Abstract into the registry** — new behavior is encoded as schema flags
  and `ReadSource` variants, not as string-name checks.

## Changes

### Change 1: Add `INJECTED_ON_DEMAND` flag to `PropertyFlags`

**File:** `crates/animatix/src/timeline/property_registry.rs`

**What:** Add a new flag that marks properties that should only be injected
into the environment when explicitly requested, rather than eagerly on every
`build_eval_env()` call.

```rust
impl PropertyFlags {
    // ... existing flags ...
    pub const INJECTABLE: Self = Self(0b0100);
    /// New: Injected only when the property is actually referenced.
    /// Properties without this flag are always injected.
    pub const INJECTED_ON_DEMAND: Self = Self(0b10000);
}
```

**Which properties get the flag:** Typed sub-keys (`position.x`, `size.y`,
`color.r`, etc.) and `_animating_*` flags. These are only needed when an
expression explicitly references `actor.prop.x` or `actor._animating_prop`.

**Registry macro change:** Add a convenience combination:

```rust
/// INJECTABLE | INJECTED_ON_DEMAND
pub const INJECTABLE_D: Self = Self(0b10100);
```

Then in the registry table, sub-key properties like `at.x`, `size.y`, `color.r`,
and all `_animating_*` entries get `INJECTABLE_D` instead of `INJECTABLE`.

### Change 2: Add `env.get_on_demand(label, property)` method

**File:** `crates/animatix/src/timeline/env.rs`

**What:** A method that checks if a property is `INJECTED_ON_DEMAND` in the
registry and, if so, lazily evaluates and injects it (and its sub-keys).

```rust
impl Environment {
    /// Resolve a dotted path like `title.color.r`.
    /// For `INJECTED_ON_DEMAND` properties, evaluates and caches the value
    /// on first access instead of requiring eager injection.
    pub fn resolve_path(
        &mut self,
        path: &[&str],
        tracks: &TrackMap,       // all animation tracks
        time_ms: u64,
    ) -> Option<Value> {
        let (label, prop) = (path[0], path[1]);
        // Check overrides first (always-injected or already-resolved)
        if let Some(val) = self.overrides.get(&format!("{label}.{prop}")) {
            return Some(val.clone());
        }
        // Not in overrides — look up schema and lazy-inject if eligible
        let schema = lookup_property(prop)?;
        if !schema.flags.contains(PropertyFlags::INJECTED_ON_DEMAND) {
            return None; // not a lazy property, and not injected — shouldn't happen
        }
        // Evaluate the property from the track and inject it (plus sub-keys)
        if let Some(track) = tracks.get(label) {
            inject_property_into_env_partial(self, label, track, time_ms, prop);
            return self.overrides.get(&format!("{label}.{prop}")).cloned();
        }
        None
    }
}
```

The key insight: `build_eval_env()` still injects all non-`INJECTED_ON_DEMAND`
properties eagerly (these are the commonly-used ones like `size`, `color`,
`position`). The sub-keys and `_animating_*` flags are injected lazily on first
access, so plot evaluation never pays for them.

### Change 3: Thread a `use_cache: bool` parameter through `evaluate_expr`

**File:** `crates/animatix/src/timeline/utils.rs`

**What:** Add a parameter to `evaluate_expr` that skips the `EVAL_CACHE` lookup
and `env_hash()` computation. In dense sampling loops where `x` and `y` change
every call, the cache will never hit anyway (different env each time), so
computing the full `env_hash()` is pure overhead.

```rust
pub fn evaluate_expr(
    expr: &Expr,
    env: &Environment,
    use_cache: bool,           // ← new
) -> Result<Value, EvalError> {
    match expr {
        Expr::Num(_) | Expr::Percent(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null => {
            return evaluate_expr_inner(expr, env, use_cache);
        }
        _ if !use_cache => {
            // Cache disabled: evaluate directly, no sorting
            return evaluate_expr_inner(expr, env, use_cache);
        }
        _ => {
            let expr_h = expr_hash(expr);
            let env_h = env_hash(env);
            // ... rest of caching ...
        }
    }
}

fn evaluate_expr_inner(
    expr: &Expr,
    env: &Environment,
    use_cache: bool,           // ← threaded through
) -> Result<Value, EvalError> {
    // ... all recursive calls pass use_cache through ...
}
```

**Call sites:** `evaluate_scalar_field()` and `evaluate_vec2_field()` in
`build/plot.rs` pass `use_cache: false` because they're inside tight sampling
loops where `x`/`y` change every iteration.

### Change 4: Extend `Environment` with dual-binding support

**File:** `crates/animatix/src/timeline/env.rs`

**What:** Currently `Environment` has a single `binding: Option<(String, Value)>`
for one variable. Extend to two bindings so `evaluate_scalar_field` can set
both `x` and `y` without cloning the env.

```rust
pub(crate) struct Environment {
    pub(crate) overrides: HashMap<String, Value>,
    pub(crate) base: Option<Arc<HashMap<String, Value>>>,
    // Before: binding: Option<(String, Value)>,
    // After:
    pub(crate) bindings: [Option<(String, Value)>; 2],
    /// Number of occupied bindings (0, 1, or 2).
    pub(crate) binding_count: usize,
}
```

Update `get()` to check all bindings before falling back to overrides:

```rust
pub fn get(&self, name: &str) -> Option<Value> {
    for i in 0..self.binding_count {
        if let Some((ref binding_name, ref binding_value)) = self.bindings[i] {
            if binding_name == name {
                return Some(binding_value.clone());
            }
        }
    }
    self.overrides.get(name).cloned().or_else(|| {
        self.base.as_ref().and_then(|b| b.get(name).cloned())
    })
}
```

Update `set_binding()` to use the next available slot:

```rust
pub fn set_binding(&mut self, name: &str, value: Value) {
    // Find an empty slot or replace existing binding with same name
    for i in 0..2 {
        if let Some((ref existing_name, _)) = self.bindings[i] {
            if existing_name == name {
                self.bindings[i] = Some((name.to_string(), value));
                return;
            }
        }
    }
    // Add to first empty slot
    let slot = self.binding_count.min(1);
    self.bindings[slot] = Some((name.to_string(), value));
    self.binding_count = (slot + 1).max(self.binding_count);
}
```

Then in `evaluate_scalar_field()`:

```rust
fn evaluate_scalar_field(
    env: &Environment,
    arg_names: &[String],
    body: &Expr,
    x: f64,
    y: f64,
) -> f64 {
    let x_name = arg_names.first().map(String::as_str).unwrap_or("x");
    let y_name = arg_names.get(1).map(String::as_str).unwrap_or("y");
    // No clone! Use bindings instead.
    env.set_binding(x_name, Value::Num(x));
    env.set_binding(y_name, Value::Num(y));
    evaluate_expr(body, env, false)  // use_cache: false
        .unwrap_or(Value::Num(f64::NAN))
        .as_num()
}
```

This eliminates ALL 784 env clones and ALL the `overrides.insert()` calls.

### Change 5: Add `inject_properties_for_label()` for selective injection

**File:** `crates/animatix/src/timeline/property_engine.rs`

**What:** A variant of `inject_runtime_lookup_values()` that only injects
properties for a specific actor label, not all labels. Used when only a
subset of actors need env entries.

```rust
/// Inject properties for a single labeled actor into the env.
pub fn inject_properties_for_label(
    env: &mut Environment,
    label: &str,
    track: &AnimationTrack,
    time_ms: u64,
) {
    let mut key = String::with_capacity(64);
    key.push_str(label);
    key.push('.');
    let prefix_len = label.len() + 1;
    // ... same logic as inject_property_into_env but for one label ...
}
```

This isn't strictly needed for the fix (Change 4 eliminates the main perf issue
by avoiding clones), but it's useful for composing minimal `PlotSampleEnv`
without the full actor list.

### Change 6: Optional — Add `eval_env_mode` enum to `Timeline`

**File:** `crates/animatix/src/timeline/frame_env.rs`

**What:** Explicit modes for which env to build. Avoids injecting 35 entries
per actor when only a few are needed.

```rust
pub enum EvalEnvMode {
    /// Full env: all actor properties + sub-keys + _animating_ flags.
    /// Used for modifier evaluation and always blocks.
    Full,
    /// Expression env: stdlib + let bindings + dimensions.
    /// Used for config evaluation and non-plot expressions.
    Expression,
    /// Plot sample env: stdlib + dimensions + optional specific actors.
    /// Used for Heatmap, VectorField, ContourSet evaluation.
    Plot { extra_labels: Vec<String> },
}

impl Timeline {
    pub fn build_eval_env(&self, time_ms: u64, mode: EvalEnvMode) -> Environment {
        match mode {
            EvalEnvMode::Full => {
                let mut env = self.env.clone();
                self.inject_runtime_lookup_values(&mut env, time_ms, None, None);
                env
            }
            EvalEnvMode::Expression => {
                // Just the base env + let bindings, no actor properties
                let mut env = self.env.clone();
                // Inject scene anchors and background
                // ... but NOT per-actor properties ...
                env
            }
            EvalEnvMode::Plot { extra_labels } => {
                let mut env = self.env.clone();
                // Only inject specified labels' properties
                for label in &extra_labels {
                    if let Some(track) = self.tracks.get(label) {
                        inject_properties_for_label(&mut env, label, track, time_ms);
                    }
                }
                env
            }
        }
    }
}
```

## Performance Impact Estimate

For a scene with 10 actors + Heatmap (resolution 28):

| Change | Before | After | Speedup |
|--------|--------|-------|---------|
| Dual bindings (Change 4) | 784 env clones × 1989 entries | 0 clones | ∞ |
| `use_cache: false` (Change 3) | 3136 sorts of ~273 entries | 0 sorts | ∞ |
| Lazy sub-keys (Change 1+2) | 1989 entries injected | ~300 entries injected | ~6.6× less data |
| `EvalEnvMode` (Change 6) | 1989 entries in plot env | ~80 entries | ~25× smaller sort |

Combined: the heatmap evaluation goes from ~30+ seconds to ~tens of milliseconds.

## Implementation Order

1. **Change 4** (dual bindings) + **Change 3** (use_cache) — these two alone
   fix the hang completely by eliminating env clones and env_hash sorts in
   the hot loop. Minimal code change, maximal impact.

2. **Change 1+2** (lazy sub-keys) — reduces memory and the size of remaining
   sorts in `Full` mode. Nice to have.

3. **Change 5+6** (mode enum) — further optimization for `Expression` mode.
   Can wait.

## Risk Assessment

- **Change 3** (`use_cache` parameter): Low risk. `evaluate_scalar_field` is
  the only tight loop where the cache can't hit. All other callers pass
  `use_cache: true` (current behavior).

- **Change 4** (dual bindings): Low risk. The `bindings` array only shadows
  `overrides` for the specific bound names; all other lookups fall through
  to the normal path. The change to `get()` is backwards-compatible.

- **Change 1+2** (lazy sub-keys): Medium risk. The `resolve_path` method needs
  access to the track map. The `Environment` struct doesn't currently hold a
  reference to it. Options:
  - Pass `tracks` as a parameter to `evaluate_expr` (invasive)
  - Store an `Option<&TrackMap>` in `Environment` (simpler)
  - Use a thread-local with the current tracks (zero-coupling)
