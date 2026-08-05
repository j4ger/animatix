# Roadmap Batch Plan: Issues Identified During Batches 6–8

Status: **PLAN (not yet merged into `docs/roadmap.md`)**. Review the corrected
severity assessment for issue #1 (closures) before scheduling — the original
"silent failure" premise is inaccurate.

Task numbering continues the existing roadmap (Batch 6–8 used #10–#20; the
callout feature is #9). New debt tasks start at #21. New batches start at 10
(Batch 9 = callout feature, already planned).

---

## ⚠️ Premise correction: Issue #1 (closures in IR/VM) is NOT a silent failure

The issue states closures "silently fail in the IR path." Inspection shows a
**graceful fallback chain** already exists:

1. `modifier_runtime/ir/lower.rs:199` — `Expr::Closure(_, _) | Expr::Construct(_, _) => None`,
   wrapped by `compile_modifier_expr` (`lower.rs:126`) as `ModifierExpr::Unsupported(expr)`.
   IR **lowering succeeds** (no error, no AST fallback triggered).
2. `modifier_runtime/vm.rs:236` — `ModifierExpr::Unsupported(_) => Err(VmCompileError::UnsupportedExpr)`.
   Bytecode compilation **errors**.
3. `build/entry.rs:269–290` — on bytecode compile error, emits a
   `ModifierCompilationError` **warning** diagnostic and does **not** store the
   bytecode program (IR program is still stored).
4. `scene_eval.rs:985` — frame-time dispatch: `if !bytecode.is_empty() { bytecode } else if !ir.is_empty() { ir } else { tree-walker }`.
   Bytecode empty → **IR path runs**.
5. `modifier_runtime/ir/eval.rs:14` — `ModifierExpr::Unsupported(expr) => crate::timeline::evaluate_expr(expr, env)`.
   The IR executor **falls back to the tree-walker per-expression** for closures.

**Net behavior:** closures in `always`/`drive` blocks **work correctly** via
bytecode→IR→tree-walker degradation.

**Real costs (why it's still worth fixing):**
- **Misleading build warning:** every `.amx` with a closure (or `Construct`)
  in a modifier emits "Bytecode compilation failed: UnsupportedExpr. Using IR
  fallback." to the user (GUI/LSP diagnostics). Confusing noise.
- **Performance cliff:** a single closure anywhere in the modifier program
  disables bytecode optimization for the **entire** timeline's modifiers (whole
  program falls back to IR, which itself falls back to tree-walker per-closure).
- `Expr::Construct` (object construction) shares the exact same gap.

**Reclassified severity:** Moderate (perf + noise), **not** Critical correctness.
This reshuffles priority — see ordering rationale.

---

## Batch 10: Correctness Triage & Hygiene
**Impact:** Potentially Critical (if #3 is a real bug) / Low otherwise | **Effort:** Low | **Dependencies:** None

Do first: `test_hierarchical_assignment_target` is the only item that *might* be
a real correctness bug, and it's currently masked as "pre-existing". Triage
before bigger refactors so CI trust is restored and a latent hierarchical-
assignment bug can't hide behind later churn.

| # | Issue | Analysis & Fix Path |
|---|-------|---------------------|
| 21 | **Triage & fix `test_hierarchical_assignment_target`** (issue #3) | Test at `timeline/tests/build.rs:108` asserts `circ.opacity=0.0 at t=0` (pre-keyframe "hidden" default) and `0.5 at t=1s` for hierarchical target `g.circ.opacity = 0.5` at `#+1s`. **Step 1:** run `cargo test -p animatix test_hierarchical_assignment_target -- --nocapture` and read the actual failure. **Step 2a (real bug):** if hierarchical target `g.circ` isn't resolving to the `circ` track, fix resolution in `timeline/assignments.rs` / `timeline/build/process.rs` (hierarchical target-key derivation). **Step 2b (stale test):** if the pre-keyframe default semantics changed (e.g. now defaults to 1.0 / inherits), update the assertion + comment to match intended semantics. Verify: `cargo test -p animatix test_hierarchical_assignment_target` passes; no new diagnostics on `examples/*.amx`. |
| 22 | **Justify or remove unjustified `#[allow(dead_code)]`** (issue #6) | AGENTS.md requires an inline justification comment on every `#[allow(dead_code)]`. Audit sites found: `renderer/text.rs:1201,1423`, `timeline/build/mod.rs:39`, `timeline/svg_import.rs:65,76,84,95`, `timeline/plot.rs:187,215`. For each: keep + add justification comment if forward-looking (e.g. "Reserved for future clip-path rendering"); remove the item if truly dead. Verify: `cargo check --workspace` clean of new dead_code warnings; `grep -rn "allow(dead_code)" crates/` shows every hit has a trailing `//` justification. |

**Note:** If #21 reveals a real hierarchical-assignment bug, **promote it to a
hotfix ahead of Batch 11** (do not bundle behind the VM work).

---

## Batch 11: Modifier VM — Closure Support & Loop-State Cleanup
**Impact:** Moderate | **Effort:** Medium-High | **Dependencies:** None (schedule after Batch 10 so CI is green)

Both tasks touch `modifier_runtime/` (lowerer + VM) and share review context.
#23 makes closures first-class (eliminates the false warning + perf cliff from
the corrected #1 analysis); #24 removes the magic-string loop state. Order #23
before #24 within the batch.

| # | Issue | Analysis & Fix Path |
|---|-------|---------------------|
| 23 | **First-class closure (and `Construct`) support in IR/VM** (issue #1) | Currently works via fallback (see premise correction). Goal: bytecode compiles, no warning, no perf cliff. **Fix path:** (a) `modifier_runtime/ir/types.rs` — add `CompiledExpr::Closure(Vec<String>, Box<Expr>, CapturedEnv)` and a `CompiledExpr::CallClosure` / invoke variant (reuse existing `Value::Closure(Vec<String>, Box<Expr>, CapturedEnv)` from `env.rs:104` — no new Value variant needed). (b) `ir/lower.rs::compile_expr` (~line 199) — replace `Expr::Closure(_, _) \| Expr::Construct(_, _) => None` with real arms: `Closure` → `CompiledExpr::Closure` capturing `CapturedEnv::snapshot` at lower time; `Construct` → emit field-eval + `MakeObject`. (c) `vm.rs` — add instructions to push a closure value and to invoke one (bind params into a child frame / inline body, run, push result); update `compile_expr` + the execution loop (~lines 480–560). (d) `ir/eval.rs` — handle the new `CompiledExpr` arms so the IR path also evaluates them natively (not via Unsupported fallback). Verify: add a test `always { let f = x => x * 2; drive g.val = f(3) }` asserting correct value **and** that no `ModifierCompilationError` diagnostic is emitted; `cargo test -p animatix`. |
| 24 | **Remove magic loop-variable strings from VM** (issue #2) | `vm.rs:505–555` tracks loop state via `frame_env.set("__for_iter_{pat_key}")` / `"__for_idx_{pat_key}"` (BeginFor/CheckFor). Fragile; collides if a user names a variable `__for_iter_*`. **Fix path:** add `loop_stack: Vec<LoopState>` (`LoopState { items: Vec<Value>, idx: usize }`) to the Vm struct; BeginFor pushes `(items, 0)`; CheckFor reads/pops from the stack instead of `frame_env.get`. Keep **user-facing** loop-var binding (`LoopPattern` → `frame_env.set(name, …)`) and the user-facing index var — those are legitimate env entries, not magic. Remove the `loop_pat_key` helper and the `__for_*` cleanup removes. Verify: existing for-loop tests pass (`cargo test -p animatix` for-loop suite); add a test where a user variable literally named `__for_iter_x` coexists with a loop and survives correctly. |

---

## Batch 12: Graph Subsystem Type Safety
**Impact:** Moderate | **Effort:** Medium | **Dependencies:** None

#25 (enum) is a prerequisite input to #26 (struct split): the split places
`ScaleType` into the static-config half. Same subsystem (`timeline/build/`),
same files — one batch.

| # | Issue | Analysis & Fix Path |
|---|-------|---------------------|
| 25 | **Replace string scale types with `ScaleType` enum** (issue #4) | `"linear"`/`"log"` strings compared across `build/utils.rs:11,27`, `build/plot.rs:258,272,344,354,496,497`, `build/property.rs:207,216,272,277`, `assignments.rs:951,955`, with defaults in `property_registry.rs:700,703` and field docs in `build/mod.rs:57,59`. **Fix path:** define `pub enum ScaleType { Linear, Log }` (with `impl From<&str>` for parse-time conversion and `#[derive(Clone, Copy, PartialEq, Eq, Debug)]`); change `GraphContext.x_scale/y_scale: String` → `ScaleType`; replace every `== "log"` with `matches!(scale, ScaleType::Log)`; update the property-registry default factory to `ScaleType::Linear`. User-facing parse input stays a string, converted at the build boundary. Verify: `cargo test -p animatix` (graph/scale tests); `grep -rn '"log"\|"linear"' crates/animatix/src/timeline/build` shows only the parse-time `From<&str>`. |
| 26 | **Split `GraphContext` static vs dynamic fields** (issue #9) | `build/utils.rs:42 GraphContext` mixes static config (`x_domain, y_domain, x_scale, y_scale`) with per-frame geometry (`size, at, padding, relative`). **Fix path:** split into `GraphScaleConfig { x_domain, y_domain, x_scale: ScaleType, y_scale: ScaleType }` (immutable per actor) and `GraphGeometry { size, at, padding, relative }` (per-frame); `graph_math_to_screen` takes `(&GraphScaleConfig, &GraphGeometry)`. Update call sites: `build/plot.rs:2122,2185`, `build/property.rs:301`, and test constructions `build/utils.rs:247,274,301,322`. Verify: `cargo test -p animatix`; no behavior change (pure refactor). |

---

## Batch 13: Persistence & Serialization
**Impact:** Moderate | **Effort:** Medium-High | **Dependencies:** **Batch 11** (Value/CapturedEnv/closure shape must be stable before serializing; #23 may add/adjust closure representation)

| # | Issue | Analysis & Fix Path |
|---|-------|---------------------|
| 27 | **Add serde to `AnimationTrack`/`Value`; replace wholesale `clone()` persistence** (issue #5) | `animation_track.rs` (structs at lines 22–316) has no serde derives; `persistence.rs:305 snapshot_track_at` does `track.clone()` for frame snapshots and there's no disk serialization. **Blocker to verify first:** `ast::Expr` (`animatix-syntax/src/ast.rs:105`) has only `#[derive(Clone, Debug, PartialEq)]` — **no serde**. `Value` (`env.rs:91`) carries `NativeFn` (Rust fn pointer) and `Closure` (live) which are **not** serializable. **Fix path:** (a) add `serde::{Serialize, Deserialize}` to `animatix-syntax` AST types (`Expr`, `Stmt`, etc.) — cross-crate change; gate behind a `serde` feature in `animatix-syntax/Cargo.toml`. (b) Custom serde for `Value`: serialize `FuncSource::Raw(args, Expr, CapturedEnv)` fine (Expr + HashMap<String, Value>); error/skip on `NativeFn` and live `Closure` (document: only static plot sources are disk-serializable, not runtime closure values). (c) Derive serde on `AnimationTrack` + sub-structs. (d) `persistence.rs`: keep targeted `clone()` for **frame snapshots** (cheap, in-memory), use serde for **disk save/load**. Verify: round-trip test (build → serialize → deserialize → re-evaluate a frame, assert equal); `cargo test -p animatix --features animatix-syntax/serde` (or whatever feature gate). |

**Risk:** the cross-crate AST serde change is the long pole; if `animatix-syntax`
must stay serde-free (e.g. `no_std`-ish constraints), fall back to a custom
binary serialization for the AST subset Animatix persists. Decide in step (a).

---

## Batch 14: Env Capture Invariant
**Impact:** Low-Moderate | **Effort:** Low-Medium | **Dependencies:** None if "harden invariant" chosen; soft dep on Batch 13 if "capture full env" chosen

| # | Issue | Analysis & Fix Path |
|---|-------|---------------------|
| 28 | **CapturedEnv capture semantics** (issue #10) | `env.rs:47–55 CapturedEnv::snapshot` clones only `env.overrides`; relies on the implicit invariant that stdlib `base` is re-provided at render time via `Timeline::build_frame_env`. **Design decision (do first):** (A) *Capture full env* — correct but clones stdlib `base` into every closure (perf cost, especially after Batch 13 serde). (B) *Harden the invariant* — keep overrides-only capture, add `debug_assert!` at render time that `base` is present, and document the guarantee on `CapturedEnv`/`build_frame_env`. **Recommend (B)** as the default (cheap, preserves the deliberate optimization); only adopt (A) if a concrete render-time-only variable is introduced that breaks the invariant. **Fix path (B):** strengthen the doc comment on `CapturedEnv` (`env.rs:46`), add a `debug_assert!(env.base_is_present())`-style check in `build_frame_env`, add a test that constructs a closure, mutates the base, and asserts the closure still resolves base symbols at render. Verify: `cargo test -p animatix`. |

---

## Icebox additions

| Task | Reason |
|------|--------|
| **Merge tree-walker and IR/VM into a single execution engine** (issue #8) | Long-term, high-risk unification. Batch 6 (#12) already extracted shared helpers (`eval_shared.rs`), so duplication is bounded. Full merge needs a design spike (which path wins? what happens to the tree-walker fallback guarantee?). Schedule only after a spike proves the migration path; until then the dual-path-with-fallback is workable and the fallback is a correctness safety net (see #1 correction). |
| **Full `typst_shorthand` (`$$…$$`) parser sync** (issue #7) | Known Batch-8 leftover. `grammar.js:171` uses `token(/[^$]*/)` for content — crude (no escaping, no nesting). Proper sync requires **tree-sitter external scanner (C)** changes (`src/scanner.c`), not just grammar edits — the AGENTS.md sync script can't cover it. Needs scanner redesign (how to tokenize `$$`-delimited content robustly). Pull into a batch only after a scanner spike; highlighting-only impact today (PEG parser handles `$$…$$` correctly). |

---

## Rationale for ordering

1. **Batch 10 first** — the only item that *might* be a real correctness bug
   (#3 hierarchical assignment) is currently hidden as "pre-existing". Triage
   immediately so a latent bug can't camouflage behind later refactor churn, and
   so CI is trustworthy for the bigger batches. Dead-code hygiene rides along
   (same low-effort profile). #22 is policy compliance (AGENTS.md rule) — cheap
   to clear now.

2. **Batch 11 next** — the corrected #1 analysis shows closures *work* but emit a
   misleading warning and disable bytecode for the whole modifier program.
   Medium severity, no correctness fire, but it's the highest-value cleanup after
   triage and it stabilizes the `Value`/`CapturedEnv`/closure representation that
   Batch 13 depends on. Loop-state cleanup (#24) shares the same VM files, so it
   rides along to avoid touching `modifier_runtime/` twice.

3. **Batch 12** — independent graph-subsystem type-safety work (#4+#9). Pure
   refactor, no behavior change, no cross-batch deps. Schedulable any time after
   10/11; placed here to keep the critical path moving while Batch 13's design
   (AST serde) is settled.

4. **Batch 13 after 11** — hard dependency: serializing `Value`/`CapturedEnv`
   requires the closure representation from #23 to be final. The cross-crate
   `animatix-syntax` AST serde decision is the long pole and should be spiked
   before committing to a disk format.

5. **Batch 14 last** — lowest urgency (#10 works today via the documented
   invariant). Recommended fix (B: harden-invariant) is cheap and independent,
   so it can actually be pulled forward into Batch 10 if fewer batches are
   desired; kept separate here to preserve its architectural framing and the
   (A)-vs-(B) design decision.

**Parallelism:** Batches 10, 11, 12, 14 are mutually independent (different
subsystems: tests/hygiene, modifier_runtime, graph, env). Only Batch 13 has a
hard predecessor (11). A multi-contributor plan can run 10/11/12/14 in parallel
once Batch 10's #21 triage confirms no cross-cutting hierarchical-assignment bug.

**What is NOT scheduled (icebox):** #8 (merge eval paths) and #7 (typst scanner
sync) both need design spikes before they can be sized into a batch. #8 in
particular should not be started lightly — the dual-path-with-fallback is
currently a *safety feature* (it's exactly what makes #1 non-critical).
