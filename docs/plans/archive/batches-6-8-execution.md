# Execution Plan: Roadmap Batches 6–8 (#10–#20)

Scope: plan execution order, commit grouping, parallelization, risk, and prerequisites for
Animatix roadmap batches 6 (Environment Model Unification), 7 (Code Quality), and 8 (Cleanup).

All findings below are grounded in the current source. File:line references are from inspection.

---

## 0. Grounded findings (from source inspection)

### Closure / environment model (#10, #11)
- `Value::Closure(Vec<String>, Box<Expr>)` — **no captured env** (`crates/animatix/src/timeline/env.rs:78`).
- Tree-walker **creates** closures without capture: `Expr::Closure(args, body) => Ok(Value::Closure(args.clone(), body.clone()))` (`timeline/utils.rs:488`).
- Tree-walker **calls** closures by cloning the **call-time** env, not creation-time (`timeline/utils.rs:647`). This is the core #11 bug.
- **IR/VM path does not support closures at all**: `compile_expr` (`modifier_runtime/ir/lower.rs:135+`) has no `Expr::Closure` arm → returns `None` → `ModifierExpr::Unsupported`. `ir/eval.rs` only references `Value::Closure` in a debug formatter (`:292`). ⇒ #11 is **tree-walker-only**; the IR path needs no closure changes.
- Plot/property build already special-case capture into a 3-tuple:
  - `build/property.rs:107` and `build/plot.rs:590`: `if let Value::Closure(args, body) = v { let captures = initial_eval_env.overrides.clone(); func = Some((args, body, captures)); }`
  - Stored as `FuncSource::Raw(Vec<String>, Expr, HashMap<String, Value>)` (`timeline/plot.rs:112`).
  - These captures clone **`overrides` only** (not the stdlib `base`).
- `Environment` = `overrides` + shared `base` (Arc, ~90 stdlib entries) + 2-slot `bindings` (`env.rs:204+`). Render-time frame env is built by `Timeline::build_frame_env` / `build_frame_env_internal` (`frame_env.rs:65,118`) and **re-provides the stdlib base**. ⇒ At render time the stdlib is available; `CapturedEnv` only needs build-time `overrides` (loop vars, `let`s).
- Render-time merge happens in `sample_procedural_plot_at` (`timeline/plot.rs:967`).

### For-loop bugs (#17) — all three confirmed
1. **Variable leak**: `process_for_loop_stmts` / `process_for_loop_inline_items` (`build/process.rs:213,233`) `bind_loop_var` + `env.set(iv, …)` but **never clear** after the loop. VM `BeginFor`/`CheckFor` (`modifier_runtime/vm.rs:493,503`) set `__for_iter_*`/loop vars and **never clear** them either.
2. **`for_iter_values` List bug**: `property_lookup.rs:152` — catch-all `_ => match evaluate_expr(iterable, env) { … Ok(value) => vec![value], … }` wraps a `Value::List` held in a **variable** as a single element instead of spreading. (Literal `Expr::List` is handled separately above it.)
3. **IR drops `index_var`**: `modifier_runtime/ir/lower.rs:86` — `Stmt::ForLoop { var, iterable, body, .. }` uses `..`, discarding `index_var`. `ModifierIrStmt::For` has no index slot.

### Graph params (#13)
- `graph_math_to_screen` (`build/utils.rs:108`) and `graph_screen_to_math` (`build/utils.rs:65`): signatures `(mx, my, x_domain, y_domain, size, at, padding, [relative], x_scale, y_scale)`.
- Call sites: `build/utils.rs` tests (`:161,163,186,188,…`), `build/plot.rs:2117` (NativeFn `make_graph_map_inverse_fn`), `build/plot.rs:2172`, `build/property.rs:304`.
- **Complication**: `size`/`at`/`padding` are read from the **runtime env** at render time (for animation) inside the NativeFn (`build/plot.rs:2103+`), while `x_domain`/`y_domain`/`x_scale`/`y_scale` are static captures. A `GraphContext` is therefore **partly static, partly dynamic** — the plan must account for this (see #13).

### AnimationTrack parent (#14)
- Struct: `dispatch.rs:49`. Has `children: Vec<String>` (`:62`), **no `parent`**.
- `children` populated at build time in `build/node.rs:16` (`parent_track.children.push(label.clone())`). This is the mirror site for setting `parent`.
- Also touched in `scene_eval.rs:1350`, `actions/mod.rs` (several), `persistence.rs` tests. Serialization lives in `persistence.rs`.

### Nested blend (#16)
- `FuncSource::Blend { from, to, frozen_progress }` (`plot.rs:114`). Evaluated in `sample_procedural_plot_at` (`plot.rs:967`); each level doubles per-sample cost.

### FFmpeg / GUI (#20)
- GUI `Cargo.toml:12` hardcodes `animatix = { path = "../animatix", features = ["video"] }`.
- Video code: `app/stores/export_store.rs:14,46`; `app/shell/export_dialog.rs:966,987,1012,1036,1064,1085` (`render_video_*_with_progress`, `VideoCodec`).

### Dual eval paths (#12)
- Tree-walker: `evaluate_expr` / `evaluate_call` (`utils.rs`).
- IR/VM: `compile_expr` (`ir/lower.rs`), `evaluate_compiled_expr` (`ir/eval.rs`), VM (`vm.rs`).
- Duplicated logic: binary ops, builtin fns (tree-walker `evaluate_call` vs IR `BuiltinFn` enum + `eval_sin`/`eval_cos`/… in `ir/eval.rs:239+`).

---

## 1. Execution order

### Critical-path track (sequential — Batch 6 core, then dependents)

| Step | Task | Why this position |
|------|------|-------------------|
| 1 | **#11** Closure capture mechanism | Foundational. Gives `Value::Closure` capture-at-creation. Unblocks #10 (simplification), #17-bug1 (safe to clear loop vars), #12 (settles `evaluate_call`). Tree-walker only. |
| 2 | **#10** CapturedEnv unification | Builds on #11. Introduce `CapturedEnv`, decide capture = build-time `overrides` (base re-provided at render), thread through `ProceduralPlot`/`FuncSource`, **remove the now-redundant** `initial_eval_env.overrides.clone()` special-case in `build/property.rs:107` + `build/plot.rs:590`. |
| 3 | **#17** For-loop bugs | bug1 (clear vars) is only safe after #11 (closures already captured their env). bugs 2 & 3 are independent but group naturally here. |
| 4 | **#16** Nested blend opt | Touches `plot.rs` (`FuncSource`, `sample_procedural_plot_at`); do after #10 settles capture representation in `plot.rs`. |
| 5 | **#19** Deprecate legacy shims | Touches `plot.rs` (`sample_procedural_plot` → `sample_procedural_plot_at`); do after #16 so the plot.rs churn is done. |
| 6 | **#15** "Never silently drop" convention | Broad audit of `.unwrap_or(Value::Num(0.0))` sites in `build/property.rs` / `build/plot.rs`; do after #10/#11 so it audits the post-refactor patterns, not the soon-to-be-removed special-case. |
| 7 | **#12** Consolidate dual eval paths | Largest/riskiest. Extract shared binary-op + builtin helpers used by both `utils.rs` and `ir/eval.rs`. Must follow #11 (which rewrites closure handling in `evaluate_call`) and #10. |

### Parallel tracks (independent — run alongside the critical path)

| Track | Task | Conflict notes |
|-------|------|----------------|
| B | **#14** `parent` field on `AnimationTrack` | Files: `dispatch.rs`, `build/node.rs`, `persistence.rs`. **No overlap** with critical-path files (`env.rs`, `utils.rs`, `build/property.rs`, `build/plot.rs`, `plot.rs`). Safe to parallelize. |
| C | **#18** Align PEG / tree-sitter | Files: `crates/animatix` parser + `tree-sitter-animatix` grammar. No overlap. Safe to parallelize. |
| D | **#20** FFmpeg optional for GUI | Files: `animatix-gui/Cargo.toml`, `export_store.rs`, `export_dialog.rs`. GUI crate only. **Fully independent.** Safe to parallelize. |

### #13 (GraphContext) — placement choice
- #13 shares `build/plot.rs` + `build/property.rs` with #10/#11 (different regions: graph-coord calls vs closure-capture blocks). Parallel work risks merge conflicts.
- **Recommended**: do #13 **either first (before step 1) as a low-risk mechanical warmup, OR after step 2**. Doing it first clears the graph-param debt before the env refactor and avoids conflicts entirely (sequential commits). If parallelized, scope strictly to `build/utils.rs` (the 2 fn defs + `GraphContext` struct) and defer call-site migration — but that leaves an inconsistent tree, so sequential is preferred.

---

## 2. Commit grouping (fixer-sized, ~1–3 files, one concern)

> Conventional-commit scopes from `cog.toml`: `animatix`, `parser`, `renderer`, `timeline`, `gui`, …
> Run before each commit: `cargo check --workspace && cargo test -p animatix-syntax && cargo test -p animatix --lib` (and `--no-fail-fast` at phase ends).

### Commit 1 — `refactor "bundle graph params into GraphContext" timeline`  (#13)
- `crates/animatix/src/timeline/build/utils.rs`: define `pub(super) struct GraphContext { x_domain, y_domain, size, at, padding, x_scale, y_scale }`; change `graph_math_to_screen`/`graph_screen_to_math` to take `&GraphContext` (+ keep `relative: bool` as a param or field).
- `crates/animatix/src/timeline/build/plot.rs`, `build/property.rs`: update call sites (incl. `make_graph_map_inverse_fn` NativeFn) to build a `GraphContext` and pass `&ctx`.
- **Caveat to resolve in-task**: `size`/`at`/`padding` are dynamic (read from runtime env in the NativeFn). Decide: `GraphContext` holds the static parts (domains/scales) and the dynamic parts are filled at call time, OR build the full ctx at each call. Pick the option that keeps animation support working. Add/keep tests in `build/utils.rs`.
- Verify: `cargo test -p animatix --lib` (graph math tests).

### Commit 2 — `feat "capture closure environment at creation" timeline`  (#11)
- `crates/animatix/src/timeline/env.rs`: `Value::Closure(Vec<String>, Box<Expr>, CapturedEnv)` — initially `CapturedEnv` can be a thin newtype/alias over the captured snapshot (or reuse `Environment`/`HashMap<String,Value>`; #10 will formalize). Update `Debug`, `PartialEq` (closures already non-eq), `Hash` (`utils.rs:83`).
- `crates/animatix/src/timeline/utils.rs`:
  - Creation (`:488`): snapshot `env` at creation → store in the closure.
  - Call (`:647`): build the child env **from the captured env** (not the call-time env), then bind params. This is the behavior fix.
- `crates/animatix/src/timeline/build/property.rs:107`, `build/plot.rs:590`: still match the new 3-tuple; keep the explicit `captures` for now (removed in Commit 3). Update the `Value::Closure(_, _)` matches at `build/plot.rs:844`, `assignments.rs:309`, `utils.rs:875`.
- Tests: add `for freq in [1,2,3] { let f = (x) => sin(x * freq) }` closure-capture test (general case, not just plot funcs). Update existing closure tests in `utils.rs:945,1172`.
- Verify: `cargo test -p animatix --lib`; specifically `tests/plot_transitions.rs::for_loop_closure_captures_loop_variable` still passes.

### Commit 3 — `refactor "unify captured env into CapturedEnv type" timeline`  (#10)
- `crates/animatix/src/timeline/env.rs`: define `CapturedEnv` (snapshot of build-time `overrides`; document that stdlib `base` is re-provided at render time via `build_frame_env`). Provide `CapturedEnv::snapshot(&Environment)` capturing `overrides` (decide whether `bindings` matter — likely no for closures).
- `crates/animatix/src/timeline/plot.rs`: `FuncSource::Raw(Vec<String>, Expr, CapturedEnv)` (was `HashMap<String,Value>`). `ProceduralPlot` (`:917`) uses `CapturedEnv`. `sample_procedural_plot_at` (`:967`) merges `CapturedEnv` into the frame env.
- `crates/animatix/src/timeline/build/property.rs:107`, `build/plot.rs:590`: **remove the redundant** `initial_eval_env.overrides.clone()` special-case — the closure (from Commit 2) already carries its captures; store the `Value::Closure` (or extract its `CapturedEnv`) directly into `FuncSource::Raw`.
- Verify: `cargo test -p animatix --lib` + `cargo test --no-fail-fast` (render-time plot sampling path).

### Commit 4 — `fix "clear for-loop vars and spread list iterables" timeline`  (#17)
- `crates/animatix/src/timeline/property_lookup.rs:152`: `for_iter_values` — in the catch-all, spread `Ok(Value::List(items)) => items` instead of `vec![value]`.
- `crates/animatix/src/timeline/build/process.rs:213,233`: after the loop, remove the loop var + index var from `self.env` (save names, clear after). Mirror in `process_for_loop_inline_items`.
- `crates/animatix/src/timeline/modifier_runtime/vm.rs`: `BeginFor`/`CheckFor` — clear `__for_iter_*` / `__for_idx_*` / bound loop var when the loop ends (the `CheckFor` `else` branch jumps to `*end`; clear there).
- `crates/animatix/src/timeline/modifier_runtime/ir/lower.rs:86`: thread `index_var` through `ModifierIrStmt::For` (add field) and bind it in the VM. (Coordinate with `ModifierIrStmt::For` definition.)
- Tests: (a) loop var undefined after loop, (b) `for x in some_list_var` spreads, (c) `for (a,b) in ... index i` works in IR path. `timeline/tests/property_lookup.rs`, `tests/build.rs`, `tests/modifiers.rs`.
- Verify: `cargo test --no-fail-fast`.

### Commit 5 — `perf "flatten nested blend evaluation" timeline`  (#16)
- `crates/animatix/src/timeline/plot.rs`: in `sample_procedural_plot_at` (`:967`), either (a) cache intermediate blend results per sample pass, or (b) **preferred** flatten `blend(A, blend(B, C, p), q)` → weighted sum `q*A + (1-q)*p*B + (1-q)*(1-p)*C` into a linear list of `(weight, FuncSource)` evaluated once each (O(N) vs O(2^N)). Add a `flatten_blend(&FuncSource) -> Vec<(f64, &FuncSource)>` helper.
- Tests: nested-blend parity (output equals naive recursive eval within epsilon); depth-3+ blend.
- Verify: `cargo test -p animatix --lib` (plot transition tests).

### Commit 6 — `chore "deprecate legacy compatibility shims" timeline`  (#19)
- Audit: `sample_procedural_plot` → `sample_procedural_plot_at` (plot.rs). `grep` for other delegators.
- Add `#[deprecated(note = "use sample_procedural_plot_at")]`, migrate call sites, or keep shim with attribute + migration note.
- Verify: `cargo check --workspace` (deprecation warnings appear only at shim, not call sites).

### Commit 7 — `chore "enforce never-silently-drop convention" timeline`  (#15)
- Audit direct `Expr` matches and `.unwrap_or(Value::Num(0.0))` silent drops in `build/property.rs`, `build/plot.rs` (post-#10/#11 state). Add a `docs/code-review.md` (or AGENTS.md) checklist item. Where a property receives an unrecognized value, emit a `Diagnostic::warning` instead of dropping.
- Add a note/grep-based check. Optionally a `warn!` for unmapped property arms.
- Verify: `cargo test -p animatix --lib` (no behavior regressions; new diagnostics tested).

### Commit 8 — `refactor "share binary-op and builtin eval helpers" timeline`  (#12)
- Extract shared helpers (e.g. `eval_binary_op(op, l, r)`, a single `BuiltinFn` enum + dispatch used by **both** `utils.rs::evaluate_call` and `ir/eval.rs::evaluate_compiled_expr`). Move builtins to a new `timeline/modifier_runtime/builtins.rs` (or `timeline/eval_shared.rs`).
- Keep the two execution paths (tree-walker + IR/VM) but eliminate duplicated op/builtin logic. Do **not** attempt a full merge in this pass (roadmap says "at minimum, shared helpers").
- Verify: `cargo test --no-fail-fast` (both eval paths exercised).

### Parallel commit (Track B) — `feat "add parent field to AnimationTrack" timeline`  (#14)
- `crates/animatix/src/timeline/dispatch.rs:49`: add `pub parent: Option<String>`; init `None` in `AnimationTrack::new`.
- `crates/animatix/src/timeline/build/node.rs:16`: when pushing `label` to `parent_track.children`, also set `child_track.parent = Some(parent_label.clone())`.
- `crates/animatix/src/timeline/dispatch.rs`: add `parent(&self) -> Option<&str>` and `children(&self) -> &[String]` helpers (children already a field; add parent helper + maybe a `child_map` for O(1) children lookup if needed — but roadmap only asks for `parent()`/`children()`).
- `crates/animatix/src/timeline/persistence.rs`: serialize/deserialize `parent` (add to the persisted form; default `None` for old files).
- Tests: build a nested actor, assert `child.parent() == Some("parent")`; exclusive-highlight-group path no longer O(n)-scans.
- Verify: `cargo test -p animatix --lib`.

### Parallel commit (Track C) — `chore "align PEG parser and tree-sitter grammar" syntax`  (#18)
- `tree-sitter-animatix` grammar + `crates/animatix` PEG parser: reconcile any drifted tokens. Add a CI script that runs both parsers over `examples/*.amx` and compares structure (best-effort). Document manual-sync rule in `AGENTS.md` "Common Pitfalls".
- Verify: `cargo test -p animatix-syntax`; tree-sitter generate + tests.

### Parallel commit (Track D) — `build "make FFmpeg optional for the GUI" gui`  (#20)
- `crates/animatix-gui/Cargo.toml`: add a `video` feature (default on); `animatix = { path = "../animatix", features = ["video"] }` only when `gui/video` enabled → use `dep:`/`?` syntax: `animatix = { path = "../animatix" }` + `video = ["animatix/video"]`.
- `app/stores/export_store.rs`, `app/shell/export_dialog.rs`: gate `render_video_*` / `VideoCodec` / video `ExportFormat` arms behind `#[cfg(feature = "video")]`; provide stubs (disabled menu items / "FFmpeg not enabled" message) when off.
- Document in `AGENTS.md` "Optional Features" how to build the GUI without FFmpeg.
- Verify: `cargo check -p animatix-gui --no-default-features` and `cargo check -p animatix-gui`.

---

## 3. Parallelization safety summary

| Task | Safe to parallel with critical path? | Reason |
|------|--------------------------------------|--------|
| #11 | — (is the critical path) | — |
| #10 | No | Builds on #11; same files. |
| #12 | No | Touches `utils.rs`/`ir/eval.rs` after #11/#10. |
| #13 | Conditional | Shares `build/plot.rs`+`build/property.rs` regions with #10/#11. Do first or after step 2; avoid concurrent edits. |
| #14 | **Yes** | `dispatch.rs`+`build/node.rs`+`persistence.rs`; no overlap. |
| #15 | No | Audits `build/property.rs`/`build/plot.rs` post-#10/#11. |
| #16 | No | `plot.rs` after #10. |
| #17 | No | bug1 depends on #11; shares `build/process.rs`/`vm.rs`. |
| #18 | **Yes** | Parser + tree-sitter only. |
| #19 | No | `plot.rs` after #16. |
| #20 | **Yes** | GUI crate only. |

Net: **#14, #18, #20 can run fully in parallel** with the Batch 6 critical path. **#13** can parallelize only if strictly scoped to `build/utils.rs` (recommend sequential instead).

---

## 4. Highest-risk tasks (need careful handling)

1. **#11 + #10 (tied)** — Highest risk. Changes the core `Value` enum and closure call semantics across the tree-walker, all match sites, and the render-time plot sampling path. Risk: regressions in plot transitions / closure capture; subtle behavior change (creation-time vs call-time env) could break existing `.amx` that relied on call-time leakage. **Mitigation**: comprehensive closure-capture tests (general case + plot case); keep the explicit-capture special-case working until Commit 3 removes it; run `tests/plot_transitions.rs` after every sub-step.
2. **#12** — Large surface; refactoring shared eval logic across two paths risks subtle divergence. **Mitigation**: shared-helpers only (no full merge); exhaustive builtin/op tests on both paths.
3. **#17 bug3 (IR index_var)** — Touches IR instruction shape (`ModifierIrStmt::For`) + VM; risk of bytecode/VM mismatch. **Mitigation**: add IR-level for-loop-with-index tests.
4. **#16** — Blend math correctness (weighted-sum flatten must match recursive lerp exactly). **Mitigation**: parity test vs naive recursive eval.
5. **#14 persistence** — Adding a serialized field risks breaking saved-file compat. **Mitigation**: default `None` on load; round-trip test.

---

## 5. Prerequisites / exploration before starting

- **#10 (design)**: Read `frame_env.rs` (`build_frame_env_internal`) to confirm exactly what the render-time env provides (stdlib `base`?) and confirm `CapturedEnv` only needs build-time `overrides`. Read `sample_procedural_plot_at` (`plot.rs:967`) to see how captures merge with the frame env today. *(Partially confirmed: base is the stdlib, re-provided at render time.)*
- **#11 (scope)**: Enumerate every `Value::Closure(` site. *(Done: construction at `utils.rs:488`; matches at `utils.rs:647,875`, `build/property.rs:107`, `build/plot.rs:590,844`, `assignments.rs:309`, `ir/eval.rs:292`; tests at `utils.rs:945,1172`. IR path creates none.)*
- **#13**: Decide `GraphContext` static-vs-dynamic split for `size`/`at`/`padding` (read `make_graph_map_inverse_fn` at `build/plot.rs:2103+`). *(Confirmed they're runtime-env-sourced.)*
- **#14**: Confirm `build/node.rs:16` is the sole build-time parent-establishment site (also check `scene_eval.rs`, `actions/mod.rs` for runtime child mutations that should mirror `parent`). *(Primary site confirmed; runtime sites to audit.)*
- **#16**: Read `sample_procedural_plot_at` fully to choose cache-vs-flatten and confirm weight math.
- **#20**: Confirm `animatix` crate's `video` feature already gates `renderer::video` (so GUI just needs to not require it). *(Confirmed GUI hardcodes the feature in `Cargo.toml:12`.)*
- **#18**: Diff current PEG grammar vs tree-sitter grammar for drifted tokens before writing the sync check.

---

## 6. Recommended start order (concrete)

1. Kick off **#14, #18, #20** in parallel (independent tracks).
2. Critical path: **#13** (warmup, low-risk) → **#11** → **#10** → **#17** → **#16** → **#19** → **#15** → **#12**.
3. Merge parallel tracks at any point (they don't touch critical-path files).
4. After each commit: `cargo check --workspace`; after each phase: `cargo test --no-fail-fast`.
