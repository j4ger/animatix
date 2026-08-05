# Roadmap #5: Graph coordinate mapping — Feasibility & Implementation Plan

## Goal

Add a `graph.map(math_x, math_y) → screen_coords` capability usable inside `always` blocks, eliminating manual magic-number conversion (`400 + mx * 70`) as seen in `examples/gradient_descent.amx`.

---

## Verdict

**YES — practical. Complexity: Low–Medium (≈1–2 days).**

The mapping math already exists in two places (`build/property.rs:240-255` for actor props, `build/plot.rs:1266` for plot curves). The AST, tree-sitter grammar, IR lowerer, and all three eval engines already support `Expr::Method(receiver, name, args)` with arguments. The domains/size are *already* readable at runtime (they survive into `env_base`). The only real work is: (1) one PEG parser gap, (2) inject a graph `map` value into the frame env, (3) teach the IR `eval_method` to honor a captured-closure receiver.

Two findings refine (and partially correct) the explorer report:
- **Corrected:** Domains are NOT registered `INJECTABLE` properties, but they ARE frozen into `env_base` at build end (`build/entry.rs:285` freezes `env.overrides`, where `build/plot.rs:780` wrote `{label}_x_domain` etc.). So the runtime frame env *can already read* `descent_graph_x_domain`, `_y_domain`, `_size`, and `.at`. They are static build-time snapshots — see Risks for the animated-`size` edge case.
- **Confirmed blocker:** `always` blocks run via the **IR/VM path** (`scene_eval.rs:943` → `apply_modifier_ir_program` → `execute_modifier_ir` → `evaluate_compiled_expr` → `eval_method`), whose `eval_method` (`ir/eval.rs:421`) takes `(receiver, name, args)` **with no `env`**. The tree-walker `evaluate_method` (`utils.rs:667`) takes `env`. A captured `NativeFn` does not need runtime env, so this is surmountable — but both `eval_method` impls must gain a receiver-dispatch arm.

---

## Recommended approach: **Option A — inject `graph.map` as a `NativeFn` (closure)**

Capture the graph's domain/size/at at build time into an `Arc` closure, expose it in the frame env under key `graph.map` (a `Value::NativeFn`), and resolve `graph.map(mx, my)` as a method call where the receiver path `graph.map` looks up the `NativeFn` and invokes it.

### Why A over the alternatives

| | Pros | Cons |
|---|---|---|
| **A. NativeFn injection** ✅ | No new `Value` variant; reuses `NativeFn` + existing `Expr::Method`/`Expr::Path`; closure captures exact transform (incl. `at` offset); future padding/log-scale = edit one closure body; per-graph isolation is automatic (one key per graph). | Needs `Expr::Path(["graph","map"])` to resolve to the `NativeFn` (it does, via env lookup) AND method-call args to apply — requires the PEG parser fix + a dispatch arm in both `eval_method`s. `NativeFn` signature wants `&Environment`; IR path lacks it (pass a throwaway or thread env). |
| B. Injectable domains + Object wrapper | "Cleaner" data model. | Touches `property_registry` flags, adds a `Graph` `Value` variant or `Object` convention, larger blast radius. Medium-high effort, low marginal benefit today. |
| C. Compile-time expansion | Zero runtime cost. | Requires AST rewriting during build, breaks if `map` is called inside a closure/conditional evaluated lazily, hard to localize errors. Fragile. |
| D. Built-in `map(graph, mx, my)` | Avoids parser changes (uses `Expr::Call`). | `graph` still must resolve to a value (same injection problem); loses method-call ergonomics; `map` is a poor global name (collides risk); harder to extend to per-graph settings. Rejected on UX grounds. |

**A wins** on minimal surface area, consistency with the existing `Value::NativeFn` pattern (already used for stdlib in `builtins.rs`), and future extensibility (padding/log scale live inside the one closure).

### Syntax decision: `graph.map(mx, my)` (method form)

The roadmap specifies method syntax and users expect it. The PEG parser gap is the only thing standing in the way — and it's a localized, well-understood fix (see Task 1). `map(graph, mx, my)` (Option D's syntax) is rejected as the primary path but noted as a fallback if the parser fix proves risky.

### Return type

`map` returns `Value::Vec2([screen_x, screen_y])` — absolute scene coords (offset + graph `at`), matching `build/property.rs:252-253`. This is what `ball.at = graph.map(mx, my)` needs. A companion `graph.map_rel(mx, my)` returning centered offsets (range `[-half,+half]`) is a cheap future addition but **out of scope** for #5.

---

## Implementation Plan

Tasks are fixer-sized and ordered. Each names file/function, exact change, outcome, and check.

### Task 1 — PEG parser: postfix method calls with args
**File:** `crates/animatix-syntax/src/parser/expr.rs` (the `access` combinator, lines ~133-163)

**Problem:** `access` folds `.segment` chains but only ever emits `Expr::Method(base, seg, Vec::new())` (zero args) for non-ident bases, and `Expr::Path(parts)` for ident bases. `graph.map(x, y)` parses as `Path(["graph","map"])` then leaves `(x, y)` unconsumed → parse error.

**Change:** After the `.segment` chain, allow an optional `(args)` call suffix on the *final* segment. Concretely: extend the `access` parser so that a segment immediately followed by `(...)` becomes `Expr::Method(Box::new(base_so_far), segment, args)` instead of being folded into a `Path`. Two implementation shapes:
- (Preferred) Make the per-segment fold produce, for each segment, either a path-extend (no parens) or a `Method` node (with parens); the first `Method` node stops further path-folding and chains subsequent `.seg` as nested method receivers.
- Keep `Expr::Path` for bare `a.b.c` with no parens (preserves existing behavior/tests).

**Outcome:** `graph.map(x, y)` parses to `Expr::Method(Expr::Ident("graph"), "map", [x, y])`. `descent_graph.at` still parses to `Path(["descent_graph","at"])`. Existing method-without-args cases (`(a.b).c`) unchanged.

**Verify:** `cargo test -p animatix-syntax` (parser tests); add a parse test asserting `graph.map(1, 2)` yields `Expr::Method(.., "map", 2 args)`. Also `cargo test -p animatix` (AST consumers). Note: tree-sitter path (`ts_convert.rs:720 convert_method_call`) already handles args, so only the chumsky PEG needs this.

### Task 2 — Shared mapping helper
**File:** `crates/animatix/src/timeline/build/property.rs` (near existing transform, lines 240-255)

**Change:** Extract the math→screen transform into a `pub(crate) fn graph_math_to_screen(mx: f64, my: f64, x_domain: [f64;2], y_domain: [f64;2], size: [f64;2], at: [f64;2]) -> [f64;2]` returning absolute screen coords. Reimplement the inline block at lines 240-255 to call it. This is the single source of truth that `map` will reuse (DRY; guarantees identical results to actor prop mapping).

**Outcome:** One function owns the transform formula. No behavior change.

**Verify:** `cargo test -p animatix` (existing prop-mapping tests must still pass).

### Task 3 — Build a per-graph `map` NativeFn and inject into env_base
**File:** `crates/animatix/src/timeline/build/plot.rs` (right after lines 780-786 where `{label}_x_domain/_y_domain/_size` are set)

**Change:** After storing domain/size, also store a `Value::NativeFn` under key `{label}.map`. The closure captures (via `Arc` or move): `x_domain`, `y_domain`, `size`, and the graph's `at` (read from `self.tracks.get(label).geometry.position.last([0,0])`, same source as `build/property.rs:204`). Closure body: validate 2 args → `as_num` → call `graph_math_to_screen` → `Ok(Value::Vec2([sx, sy]))`; else `Err(EvalError::TypeMismatch("graph.map expects (x, y)"))`.

Because this is written via `self.env.set(...)`, it lands in `env.overrides` and is frozen into `env_base` at `build/entry.rs:285` — automatically available in every frame env. (No `inject_runtime_lookup_values` change needed.)

**Outcome:** `env.get("descent_graph.map")` returns the `NativeFn` at runtime.

**Verify:** New unit test: build a timeline with a `Graph`, assert `timeline.env_base.get("g.map")` is `Some(Value::NativeFn(..))`. Also assert calling it with `(3.0, 2.4)` for domain `(-4,4)` size `(560,560)` at `(400,380)` yields `(610, 212)` — the exact magic numbers from the example, proving equivalence.

### Task 4 — Dispatch method calls on `NativeFn` receivers (IR path)
**File:** `crates/animatix/src/timeline/modifier_runtime/ir/eval.rs` (`eval_method`, line 421)

**Problem:** `eval_method(receiver, name, args)` has no `NativeFn` arm — a `NativeFn` receiver falls through to `UnsupportedMethod`. This is the path `always` blocks use.

**Change:** Add an arm **before** the final fallback:
```rust
(Value::NativeFn(f), _) => f(args, &Environment::new()),
```
The closure captured its data at build time, so it ignores the passed env. (If a future `map` variant needs runtime values like animated `size`, thread `env` through `eval_method` instead — see Risks. For #5, the captured snapshot is correct because domains are non-animatable.)

**Outcome:** `graph.map(mx, my)` executes in `always` blocks.

**Verify:** IR eval unit test: env with `"g.map" -> NativeFn(..)`, evaluate `Expr::Method(Ident("g"), "map", [Num(3.0), Num(2.4)])` via `evaluate_compiled_expr` → `Vec2([610, 212])`.

### Task 5 — Dispatch method calls on `NativeFn` receivers (tree-walker path)
**File:** `crates/animatix/src/timeline/utils.rs` (`evaluate_method`, line 667)

**Change:** Add the same `NativeFn` arm:
```rust
(Value::NativeFn(f), name) if name == "map" || /* general: */ true => {
    let mut arg_values = Vec::with_capacity(args.len());
    for a in args { arg_values.push(evaluate_expr(a, env)?); }
    f(&arg_values, env)
}
```
Place before the `Object` field-access arm (line ~792) so it takes precedence for `NativeFn` receivers. (The tree-walker is the AST-fallback path used when IR lowering fails — `scene_eval.rs:953` — and is used by `build_eval_env`/plot sampling.)

**Outcome:** Consistent behavior across all eval engines.

**Verify:** `cargo test -p animatix` (utils.rs already has method-call tests at lines 932-1040; add one for `NativeFn` receiver).

### Task 6 — VM bytecode path (verify, likely no change)
**File:** `crates/animatix/src/timeline/modifier_runtime/vm.rs` (`CallMethod`, line 408/520)

**Check:** The VM delegates to `super::ir::eval_method` (line 411). Since Task 4 adds the `NativeFn` arm to that shared function, **no VM change is needed**. Confirm by running a bytecode-execution test.

**Verify:** `cargo test -p animatix` modifier VM tests.

### Task 7 — End-to-end example conversion
**File:** `examples/gradient_descent.amx` (the `Descent` scene `always` block, ~line 230)

**Change:** Replace
```
ball.at = (400 + mx * 70, 380 - my * 70)
```
with
```
ball.at = descent_graph.map(mx, my)
```
Optionally also convert the hardcoded starting `at: (610, 212)` (a comment already derives it from math `(3, 2.4)`).

**Outcome:** Demonstrates the feature; proves the magic numbers vanish.

**Verify:** Render the example (GUI / headless render test) and diff a frame against the pre-change render — pixels must be identical (the transform is mathematically identical). Add a render-golden test if the infra exists.

### Task 8 — Docs
**Files:** `docs/spec.md` (or wherever `always`/Graph is documented), `docs/roadmap.md` (remove item #3/#5 once done — per AGENTS.md, roadmap holds only remaining work).

**Change:** Document `graph.map(x, y)` semantics (absolute screen coords, linear domains, build-time snapshot), and note `map_rel`/padding/log-scale as future. Remove the completed roadmap row.

**Verify:** Doc build / review.

---

## Files to touch

- `crates/animatix-syntax/src/parser/expr.rs` — PEG method-call-with-args (Task 1)
- `crates/animatix/src/timeline/build/property.rs` — extract `graph_math_to_screen` (Task 2)
- `crates/animatix/src/timeline/build/plot.rs` — inject `{label}.map` NativeFn (Task 3)
- `crates/animatix/src/timeline/modifier_runtime/ir/eval.rs` — `NativeFn` arm in `eval_method` (Task 4)
- `crates/animatix/src/timeline/utils.rs` — `NativeFn` arm in `evaluate_method` (Task 5)
- `crates/animatix/src/timeline/modifier_runtime/vm.rs` — verify only (Task 6)
- `examples/gradient_descent.amx` — convert magic numbers (Task 7)
- `docs/spec.md`, `docs/roadmap.md` — document & close (Task 8)

## Dependency order

1 → 2 → 3 → (4,5 parallel) → 6 → 7 → 8. Task 1 (parser) and Tasks 4/5 (dispatch) are independent and could be developed in parallel, but the example (7) needs all of 1-5.

---

## Risks & edge cases

### R1 — Animated `size` / `at` desync (medium)
The `map` closure captures `size` and `at` as **build-time snapshots** (domains are non-animatable, but `size` and `at` ARE `ANIMATED|ASSIGNABLE|INJECTABLE`). If a user animates `descent_graph.size`, `map` will use the stale snapshot while the rendered graph uses the live size → ball drifts off the curve.

**Mitigations (pick one):**
- (A, recommended for #5) Document that `map` reflects the graph's static layout; animate the *ball*, not the graph. Add a build-time diagnostic warning if a graph with a `.map` consumer has keyframed `size`/`at`.
- (B) Make the closure read live values each call by capturing the `Timeline`/track handle — but `NativeFn` is `Send+Sync` and takes only `(&[Value], &Environment)`. The runtime env already has live `descent_graph.size` and `descent_graph.at` (injected each frame). So prefer: capture only domains (static), and read `size`/`at` from the passed `env` inside the closure. This requires the IR `eval_method` to pass `env` (Task 4 becomes: thread `env` into `eval_method`). **This is the more correct design** — see "Refinement" below.

**Refinement to Task 3/4 (recommended):** Capture only `x_domain`/`y_domain` (truly static). Read `size` from `env.get("{label}.size")` and `at` from `env.get("{label}.at")` inside the closure body. Then the closure is always correct, and the only cost is threading `env` through IR's `eval_method` (change its signature to `eval_method(receiver, name, args, env)` and update the two callers: `ir/eval.rs:229` and `vm.rs:411`). This is a small, mechanical change and removes R1 entirely. **Adopt this refinement.**

### R2 — Multiple graphs in one scene (low)
Each graph gets its own `{label}.map` key, so `descent_graph.map(...)` and `lr_graph.map(...)` coexist cleanly. No shared global state. The `LearningRate` scene in the example already has two graphs — good test case.

### R3 — PEG parser regression (medium)
The `access` combinator is subtle (it handles `a.b.c`, `(expr).field`, method chains). Task 1 must not break `node.at.x`, `scene.center`, `text.primary`, or the existing zero-arg `Method` emission. The tree-sitter grammar is unaffected (already supports args). Run the full parser test suite; the chumsky parser may have fewer tests than tree-sitter, so **add explicit tests** for: `a.b()`, `a.b(c)`, `a.b.c`, `a.b(c).d`, `(f()).g(h)`.

### R4 — `NativeFn` cannot be hashed (low, already handled)
`timeline/utils.rs:45-82` already skips `NativeFn` in hashing/equality. Injecting `map` into `env_base` won't break the eval cache or env hashing. No action — just don't add `map` to any hash-dependent structure.

### R5 — `map` name collision (low)
If a user names an actor `map` or a property `map`, the key `{label}.map` could collide. Actor labels are user-chosen; `.map` is not a registered property. Risk is minimal but real. Mitigation: the `NativeFn` is only injected for `is_graph_host()` actors (Task 3 is inside that `if`), so only `Graph`/graph-host labels get `.map`. A non-graph actor named `map` is unaffected.

### R6 — Domain of `(0,0)` or equal min/max (low)
`graph_math_to_screen` already guards `x_range != 0.0` / `y_range != 0.0` (build/property.rs:247,250). Reuse that guard. Returns `0.0` offset for degenerate axis. No NaN.

### R7 — `always` writes to `graph.at` then reads via `map` (low)
If an `always` block both overrides `graph.at` and calls `graph.map`, order matters. With R1's refinement (read live `at` from env), `apply_override_incremental` (`frame_env.rs:18`) updates `env` immediately on assignment, so a `let ... = graph.map(...)` *after* `graph.at = ...` sees the new `at`. A `map` *before* the override sees the old. This matches normal env semantics — document ordering sensitivity.

### R8 — Coord system mismatch: offset vs absolute (medium, design)
`build/plot.rs:1266 math_to_screen` returns **centered** offsets `[-half,+half]` (used for plot-curve path geometry, which is later translated by the actor transform). `build/property.rs:240-255` returns **absolute** coords (offset + `at`). `ball.at = graph.map(...)` needs **absolute**. Task 2's helper must return absolute (offset + `at`). Do NOT reuse `plot.rs:1266` directly without adding `at`. This is the single most important correctness detail — get it wrong and the ball lands at the graph's *local origin*, not its screen position.

---

## Alternative: skip method syntax entirely

### `map(graph_label, mx, my)` as a built-in call (Option D)
- **UX:** inferior. Reads "map of what?" and `map` is a generic name begging to collide. Rejected.
- **Cost saving:** avoids Task 1 (PEG parser). But still needs Tasks 2-5 (resolution + dispatch), so saves ~15% effort at a real UX cost. Not worth it.

### `graph.map` as a property returning a closure
- Bind `graph.map` to a `Value::Closure` whose body is `Expr` that does the arithmetic. But the closure body can't reference captured `x_domain` unless they're env vars — and they are (`descent_graph_x_domain`). So: `Closure(["x","y"], <expr referencing descent_graph_x_domain, .size, .at>)`.
- **Pros:** no `NativeFn`-receiver dispatch needed — `graph.map(mx,my)` becomes `Call` on the looked-up closure, which `evaluate_call` (`utils.rs:655`) already handles for `Closure`. **Skips Tasks 4 & 5.**
- **Cons:** the closure body is an `Expr` that must be hand-assembled at build time (a nested `Binary`/`Tuple` AST) — verbose and fragile vs. a Rust closure. Also the IR `evaluate_compiled_expr` path for `Call` (`ir/eval.rs:140`) handles `NativeFn`/builtins but check it handles `Closure` lookup from env — it appears to only handle known builtins, **not env-looked-up closures** (unlike the tree-walker's `evaluate_call`). So this may *not* work in the IR path without more changes. **The NativeFn approach (A) is more robust.**

### Verdict on alternatives
Stick with **Option A (NativeFn), method syntax, with the R1 refinement** (capture domains statically, read size/at from env). It is the smallest correct surface and extends cleanly to padding/log-scale later.

---

## Open questions for the maintainer

1. **Return type scope:** Ship absolute `map` only for #5, defer `map_rel` (centered offsets) and inverse `screen_to_math`? (Recommendation: yes, defer.)
2. **Animated graph layout:** Accept the "don't animate graph size/at if you use map" constraint (with a diagnostic), or invest in the env-threading refinement up front? (Recommendation: do the refinement — it's cheap and correct.)
3. **Padding/insets:** Confirmed absent today. Confirm we defer all padding logic to a future roadmap item and document `map` as "linear, no padding" for now.
