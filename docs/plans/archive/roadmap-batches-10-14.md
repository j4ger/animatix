# Roadmap Batches 10–14 Implementation Plan

## Goal
Complete roadmap batches 10–14 with low-risk commit groups, keeping Batch 10 as documentation cleanup and sequencing serialization after modifier IR/VM closure support.

## Assumptions / Blockers
- Batch 10 tasks 21–22 are already complete; only `docs/roadmap.md` should be updated to remove Batch 10.
- The `serde` batch is larger than the other refactors because `animatix-syntax` AST types and several runtime-only values currently lack serialization support.
- Do not combine Batch 13 with graph or env refactors; it changes public data model shape and needs isolated review.
- `cargo check --workspace` may require FFmpeg system libraries if GUI default `video` feature is enabled; if blocked, run `cargo check --workspace --exclude animatix-gui` plus `cargo check -p animatix-gui --no-default-features` and document the blocker.

## Commit Groups
1. `docs(roadmap): remove completed batch 10`
2. `feat(timeline): compile modifier closures and constructs`
3. `refactor(timeline): store VM loop state off-env`
4. `refactor(timeline): type graph scales and split graph context`
5. `feat(timeline): add serializable track snapshots`
6. `refactor(timeline): harden captured env invariant`
7. `docs(roadmap): remove completed batches 11-14`

## Plan

1. Remove completed Batch 10 from roadmap
   - Files: `docs/roadmap.md`.
   - Change: delete the `### Batch 10: Correctness Triage & Hygiene` section because tasks 21–22 are complete.
   - Expected outcome: roadmap only lists remaining work, per AGENTS.md.
   - Verify: `grep -n "Batch 10\|Task | 21\|Task | 22\|allow(dead_code)" docs/roadmap.md` returns no Batch 10 task entries.
   - Commit: `cog commit docs "remove completed roadmap batch 10" docs`.

2. Add compiled modifier closure and construct IR forms
   - Files: `crates/animatix/src/timeline/modifier_runtime/ir/types.rs`, `crates/animatix/src/timeline/modifier_runtime/ir/lower.rs`, `crates/animatix/src/timeline/modifier_runtime/ir/eval.rs`.
   - Change: add `CompiledExpr::Closure(Vec<String>, Box<Expr>)` and `CompiledExpr::Construct(String, Vec<(String, CompiledExpr)>)` (or `Vec<Property>` only if keeping tree-walk fallback for property values is intentional); lower `Expr::Closure` with captured body and `Expr::Construct` by compiling each property value.
   - Change: in `evaluate_compiled_expr`, evaluate `Closure` to `Value::Closure(params, body, CapturedEnv::snapshot(env))` and `Construct` to `Value::Object(type_name, evaluated_fields)`.
   - Expected outcome: IR no longer marks closures/constructs as `Unsupported`, so bytecode compilation can proceed for modifiers containing these expressions.
   - Verify: add/adjust tests under the existing modifier runtime test area or `crates/animatix/src/timeline/tests/modifiers.rs`; run `cargo test -p animatix --lib modifier` and `cargo test -p animatix --lib timeline::utils::tests::test_evaluate_construct_creates_object`.
   - Commit with task 3 after bytecode support lands, not alone, to avoid IR/VM mismatch.

3. Add bytecode instructions for closure and construct expressions
   - Files: `crates/animatix/src/timeline/modifier_runtime/vm.rs`, same IR files as task 2 if needed.
   - Change: add instructions such as `MakeClosure(Vec<String>, Box<Expr>)` and `MakeObject(String, Vec<String>)`; compile closure by emitting `MakeClosure`, compile construct by emitting each field value followed by `MakeObject` with field names.
   - Change: execute `MakeClosure` with `CapturedEnv::snapshot(frame_env)` and execute `MakeObject` by popping field values in reverse order into a `HashMap<String, Value>`.
   - Expected outcome: `compile_modifier_bytecode` succeeds for always blocks containing closure and construct expressions, without `Bytecode compilation failed` diagnostics.
   - Verify: build a regression source with `always { let f = (x) => x + t; let p = Point { x: f(1), y: 2 }; actor.opacity = p.x }`; assert `timeline.modifier_bytecode_programs.len() == 1` and no `DiagnosticCode::ModifierCompilationError`. Run `cargo test -p animatix --lib modifier_runtime` or the targeted new test module.
   - Commit: `cog commit feat "compile modifier closures and constructs" timeline`.

4. Replace VM loop magic env keys with explicit loop stack
   - Files: `crates/animatix/src/timeline/modifier_runtime/vm.rs`.
   - Change: remove `loop_pat_key` and all `__for_iter_*` / `__for_idx_*` env writes; add `LoopState { items: Vec<Value>, index: usize, var: LoopPattern, index_var: Option<String> }` and `loop_stack: Vec<LoopState>` to `ModifierVm`.
   - Change: `BeginFor` pops iterable, converts to `Vec<Value>`, pushes a `LoopState`; `CheckFor` reads `last_mut()`, binds user-facing loop/index vars, increments index, and pops/cleans vars when exhausted.
   - Change: preserve the 100,000 iteration guard but make it per loop state if feasible; otherwise keep the existing VM-level guard and reset it on `BeginFor`.
   - Expected outcome: user variables named like `__for_iter_item` no longer collide with VM internals; nested loops no longer share env-backed state accidentally.
   - Verify: add tests for collision (`let __for_iter_item = 42` survives a `for item in ...`) and nested loops. Run `cargo test -p animatix --lib modifier`.
   - Commit: `cog commit refactor "store VM loop state off env" timeline`.

5. Introduce `ScaleType` and parsing boundary helpers
   - Files: `crates/animatix/src/timeline/build/utils.rs`, `crates/animatix/src/timeline/build/mod.rs`, `crates/animatix/src/timeline/build/plot.rs`, `crates/animatix/src/timeline/build/property.rs`, `crates/animatix/src/timeline/assignments.rs`.
   - Change: define `pub(super) enum ScaleType { Linear, Log }` in `build/utils.rs` with `from_value(Value)`, `from_str(&str)`, `as_str()` if env persistence stays string-based, and `is_log()`.
   - Change: convert user-facing `x_scale` / `y_scale` strings at build boundaries; warn or emit `InvalidPropertyValue` for unknown values and default to `Linear` rather than silently treating typos as linear.
   - Change: update `normalize`, `denormalize`, `normalize_axis`, `generate_axis_ticks`, `build_graph_axis_paths`, `make_graph_map_fn`, `make_graph_map_inverse_fn`, and assignment rebuild logic to accept `ScaleType` instead of `&str` / `String` internally.
   - Expected outcome: internal graph scale checks use `matches!(scale, ScaleType::Log)`; user syntax and env keys remain compatible unless the team chooses to store `Value::Object`/enum later.
   - Verify: `cargo test -p animatix --lib timeline::build::utils` and existing graph tests; add one invalid-scale test if diagnostics are straightforward.
   - Commit with task 6 after `GraphContext` split lands.

6. Split graph static scale config from dynamic geometry
   - Files: `crates/animatix/src/timeline/build/utils.rs`, `crates/animatix/src/timeline/build/property.rs`, `crates/animatix/src/timeline/build/plot.rs`.
   - Change: replace `GraphContext` with `GraphScaleConfig { x_domain, y_domain, x_scale, y_scale }` and `GraphGeometry { size, at, padding }`.
   - Change: update `graph_math_to_screen(mx, my, &scale_config, &geometry, relative)` and `graph_screen_to_math(sx, sy, &scale_config, &geometry)`; update tests to construct the two structs explicitly.
   - Change: in map closures, capture `GraphScaleConfig` plus static padding where appropriate, then reconstruct only `GraphGeometry` from runtime `size`, `at`, and padding values.
   - Expected outcome: static domain/scale data is clearly separated from animated geometry, with no behavior changes.
   - Verify: `cargo test -p animatix --lib timeline::build::utils`; `cargo test -p animatix --lib graph` if a filter exists; otherwise `cargo test -p animatix --lib`.
   - Commit: `cog commit refactor "type graph scales and split context" timeline`.

7. Add serde feature to syntax AST and runtime value model
   - Files: `crates/animatix-syntax/Cargo.toml`, `crates/animatix-syntax/src/ast.rs`, `crates/animatix/Cargo.toml`, `crates/animatix/src/timeline/env.rs`.
   - Change: add optional `serde = { version = "1", features = ["derive"], optional = true }` to `animatix-syntax`; add feature `serde = ["dep:serde"]` and derive `Serialize, Deserialize` on AST structs/enums behind `cfg_attr(feature = "serde", ...)`.
   - Change: add `serde` to `animatix` dependencies and enable `animatix-syntax/serde`; derive or custom-serialize `CapturedEnv`.
   - Change: implement custom serde for `Value` because `NativeFn` cannot serialize; support `Num`, `Str`, `Bool`, vectors, `Color`, `List`, `Object`, and `Closure`; serialize `NativeFn` as an error for disk persistence, not as a silent skip.
   - Expected outcome: AST-backed closures and construct objects can be encoded without pulling runtime function pointers into persisted data.
   - Verify: `cargo check -p animatix-syntax --features serde`; add `serde_json` roundtrip tests for `Expr::Closure`, `Expr::Construct`, `CapturedEnv`, and serializable `Value`; run `cargo test -p animatix --lib value_serde` or targeted module tests.
   - Commit with task 8 only if `AnimationTrack` derives also compile; otherwise split at syntax/runtime boundary.

8. Serialize `AnimationTrack` and replace clone-based persistence where intended
   - Files: `crates/animatix/src/timeline/property_track.rs`, `crates/animatix/src/timeline/animation_track.rs`, `crates/animatix/src/timeline/dispatch.rs`, `crates/animatix/src/timeline/plot.rs`, `crates/animatix/src/timeline/persistence.rs`, possibly actor kind/shape/layout type files.
   - Change: derive or hand-implement serde for `PropertyTrack<T>` with `last_evaluated` skipped/defaulted; derive serde for track tier structs, `AnimationTrack`, `FuncSource`, `FuncTransition`, and dependent enums (`PlacementMode`, `PositionBinding`, `ActionEvent`, etc.).
   - Change: for non-serializable render-only payloads (`SceneImage`, `TextPath`, `VelloPath`, GPU/render caches), decide per field: skip with documented rebuild semantics, serialize a portable representation, or gate serde by feature. Do not silently drop values.
   - Change: keep `snapshot_track_at`’s in-memory clone/collapse unless the team specifically wants a serde roundtrip for carrying; use serde for disk save/load paths and add helpers like `serialize_carry_bag` / `deserialize_carry_bag` only if a disk persistence API already exists.
   - Expected outcome: persistence can roundtrip actor tracks through serde without relying on wholesale clone for serialized snapshots; frame snapshots still use direct clone unless replacing that is proven necessary.
   - Verify: add roundtrip tests for a simple Rect track, Graph/PlotCurve track with `FuncSource::Raw`, and `NativeFn` error behavior. Run `cargo test -p animatix --lib persistence` and `cargo test -p animatix --lib`.
   - Commit: `cog commit feat "add serializable track snapshots" timeline`.

9. Harden `CapturedEnv` base-env invariant
   - Files: `crates/animatix/src/timeline/env.rs`, `crates/animatix/src/timeline/frame_env.rs`, `crates/animatix/src/timeline/plot.rs`, `crates/animatix/src/timeline/utils.rs`.
   - Change: add `Environment::has_base()` or `Environment::base_len()` for invariant checks without exposing the `base` map publicly.
   - Change: add `debug_assert!(env.has_base(), "CapturedEnv merge requires a render/build env with stdlib base")` at render-time call sites that evaluate stored closures, especially `FuncSource::Raw` paths in `plot.rs`; do not assert in pure unit-test helpers that intentionally use `Environment::new()` unless they merge captured stdlib separately.
   - Change: document in `CapturedEnv::snapshot` and `merge_into` that captures store only overrides and must be merged into an environment whose base layer is supplied by `Timeline::build_frame_env_internal` / `build_eval_env`.
   - Expected outcome: the existing efficient capture semantics stay intact, and invariant violations fail loudly in debug builds.
   - Verify: `cargo test -p animatix --lib captured_env` if adding focused tests; otherwise `cargo test -p animatix --lib timeline::utils::tests::test_evaluate_closure_captures_variable_at_creation_time` and `cargo test -p animatix --lib plot_transitions`.
   - Commit: `cog commit refactor "harden captured env invariant" timeline`.

10. Remove completed roadmap batches 11–14
   - Files: `docs/roadmap.md`.
   - Change: after code is merged and verified, delete Batch 11, Batch 12, Batch 13, and Batch 14 sections from `docs/roadmap.md`.
   - Expected outcome: roadmap lists only genuinely remaining items.
   - Verify: `grep -n "Batch 11\|Batch 12\|Batch 13\|Batch 14\|Task | 23\|Task | 28" docs/roadmap.md` returns no active planned entries.
   - Commit: `cog commit docs "remove completed roadmap batches 11 through 14" docs`.

## Full Verification Matrix
- After Batch 11 commits: `cargo test -p animatix --lib modifier` and `cargo test -p animatix --lib`.
- After Batch 12 commit: `cargo test -p animatix --lib timeline::build::utils` and `cargo test -p animatix --lib`.
- After Batch 13 commit: `cargo test -p animatix-syntax --features serde`, `cargo test -p animatix --lib persistence`, and `cargo test -p animatix --lib`.
- After Batch 14 commit: `cargo test -p animatix --lib`.
- Before final commit handoff: `cargo check --workspace`, `cargo test -p animatix-syntax`, `cargo test -p animatix --lib`, and `cargo test --no-fail-fast`.

## Risks
- `Value::NativeFn` is intentionally non-serializable; persistence must surface an error or use an explicit skip policy with tests.
- `VelloPath`, `TextPath`, and `SceneImage` may not derive serde; serializing `AnimationTrack` may require a portable snapshot DTO rather than deriving directly on every runtime field.
- Graph scale strings are stored as env values today; switching env storage to an enum would require changing `Value`, so the safer refactor is enum internally plus string at env/user boundaries.
- `MakeClosure` in bytecode must capture the environment at instruction execution time, not compile time, or loop/local captures will be wrong.
- VM loop cleanup currently removes user-facing loop variables; preserve that behavior for both normal and exhausted empty loops, and test nested loops.
- `debug_assert!` on `CapturedEnv` must not break tree-walker unit tests that use `Environment::new()` without stdlib base.
