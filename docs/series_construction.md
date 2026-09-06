# Series Construction (LG-1) — Design Note

Status: approved 2026-09-06. Tracks roadmap LG-1; evidence in
`dogfood/projects/taylor-sin/notes.md`.

## Motivation

STEM explainers need series. The taylor-sin dogfood had to hand-expand
S₇(x) into 7 gated terms with the degree knob inlined 6× into the plot
closure. This note specifies the smallest language addition that removes
that workaround class:

1. `factorial(n: Num) -> Num`
2. `sum(items: List<Num>) -> Num`
3. `sum_range(f: Closure, lo: Num, hi: Num) -> Num` — the folder
4. Block-bodied closures: `(x) => { let a = … let b = … tail-expr }`

Non-goals: `return`/`for` statements inside closures (pure-fn bodies
already cover statement-heavy computation at build time), recursion
guards (pre-existing gap, unchanged), lazy sequences.

## Mechanism selection (why two registration routes)

The runtime has two builtin routes, and the folder must use the second:

- **Fast path** (`eval_shared::eval_builtin_fn`, no `env` parameter, called
  from both the tree-walker and the IR's `CallBuiltin` arm): for
  environment-independent functions. `factorial` and `sum` land here, with
  the IR triple (`BuiltinFn` variant, `builtin_for_name` arm, enum→name arm
  in ir/eval.rs — the last has no wildcard, so the compiler enforces it).
- **Env route** (stdlib `NativeFn` in `builtins.rs::load_standard_library`,
  dispatched via `utils::evaluate_call_value` on both paths): for functions
  that need the caller's `Environment`. A folder must invoke a closure,
  which requires the caller's base layer (`Environment::with_base`) to keep
  NativeFns reachable inside the body — `eval_builtin_fn` has no env, so
  `sum_range` registers as a NativeFn, exactly like `rand`/`lerp_vec2`.

## `factorial` / `sum` semantics

- `factorial(n)`: `n` must be an integer ≥ 0 (`n.fract() == 0.0 &&
  n >= 0.0`), otherwise `EvalError::TypeMismatch`. Computed by f64
  iteration; up to `170` before infinity (values beyond error out with a
  range message).
- `sum(items)`: every element must be `Value::Num` (strict — unlike
  `list_swap`'s `as_num` coercion, an error message beats a silently wrong
  total); empty list → `0`. Non-list first arg → `TypeMismatch`.

## `sum_range` semantics

`sum_range(f, lo, hi)` = `Σ_{k=lo}^{hi} f(k)`:

- `f` must be a single-parameter `Value::Closure`; `lo`/`hi` must be
  integers; `lo > hi` → `0` (empty range). Iteration count is bounded
  (`hi - lo + 1 ≤ 100_000`, mirroring the IR for-loop guard).
- The child env is built **once** (caller base + `captures.merge_into`),
  then each iteration only rebinds the parameter — not a fresh
  `Environment::with_base` per call (a plot at 128 samples × n terms makes
  this the hot path).
- Errors from `f` propagate; no silent NaN.

## Block-bodied closures — `Expr::LetChain`

Grammar: after `=>`, if the next token is `{` **and the token after it is
`let`**, parse a block body; otherwise `{` keeps its list-literal meaning
(`(x) => {1, 2}` continues to return a list — zero compatibility break).

AST: `Expr::LetChain { bindings: Vec<(String, Expr)>, tail: Box<Expr> }`.
Bindings evaluate in order with lexical shadowing (`let a = 1; let a = a + 1`
is legal); the tail expression is the value. This is deliberately narrower
than the pure-fn statement set: no `return`, no statement-level
`for`/`match` (`if` remains available as an expression).

Evaluation (tree-walker and IR share the protocol):
1. Record every inserted key, insert bindings into the *current* env.
2. Evaluate the tail.
3. Restore: remove exactly the inserted keys, re-mark mutated.

This mirrors the plot sampler's existing hygiene protocol
(`plot.rs` `eval_source_scalar`'s merge-then-remove) because plot closures
share one env across sample points — a leaked `let` would corrupt
subsequent samples and other actors.

Capture semantics: `Expr::Closure` bodies are compiled at value creation
(`utils.rs:292`), and `CapturedEnv` snapshots the override layer at that
moment. A `LetChain` never becomes a capture (it evaluates immediately),
so capture machinery is untouched.

IR: `CompiledExpr::LetChain { bindings: Vec<(String, CompiledExpr)>, tail:
Box<CompiledExpr>) }` with the same eval protocol; serde derives carry it
through the plugin ABI like every other `CompiledExpr` node.

## Env duality at build vs frame time

- Build time (pre-freeze): stdlib lives in `env.overrides`, so a closure
  value created at declaration time captures it. Calls resolve through the
  capture.
- Frame time (post-freeze): `timeline.env_base` is frozen into `base`;
  closure captures exclude base (by design), and invocation rebuilds the
  child env from the caller's base (`utils.rs:490-500`). This is why the
  folder needs the caller env — and why plot closures calling pure user
  fns already work end to end (verified by `plot_closure_calls_pure_fn`,
  Stage 0): `CallEnv` resolves the fn name through the frame env's frozen
  base into `Value::UserFn`.

## Testing strategy

- Unit: `eval_shared` (factorial/sum incl. error arms), utils tree-walker,
  IR parity (`tests/ir_tests.rs` asserts IR result == tree-walker result).
- Folder: build-time `let` precompute, frame-time `always`, plot-closure
  integration (dynamic gate fires because the body references `t`), error
  paths.
- LetChain: parser round-trip + formatter idempotence, shadowing order,
  IR parity, plot integration, analyzer/SymbolTable correctness, serde.
- End-to-end: taylor-sin Sweep scene rewritten onto `sum_range` +
  `factorial`, pixel-compared against the hand-expanded version at
  n = 1/5/13.
