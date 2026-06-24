# Animatix Roadmap

Keep track of what is yet to be done here, when a segment is fully done, remove the items from here.

---

## Planned

Items are grouped into implementation batches based on dependencies and shared systems.

### New features
New primitive types for educational content.

| # | Task | Notes |
|---|------|-------|
| 9 | **Callout/annotation primitive** | `Callout { target: actor, text: "...", arrow: true }` for educational diagrams — labeled arrow pointing at a specific actor or plot element. Not yet designed or implemented. |

---

## Architectural Debt

Accumulated technical debt from organic growth. Grouped by priority and dependencies.

### Batch 6: Environment Model Unification (Foundation) ✅
**Impact:** Critical | **Effort:** High | **Dependencies:** None (but blocks other fixes)

These issues stem from the fundamental disconnect between build-time and render-time environments.

| # | Task | Status |
|---|------|--------|
| 10 | **Unified build/render environment model** | ✅ Complete - `CapturedEnv` type snapshots build-time overrides, threaded through `ProceduralPlot` and `FuncSource`. Unified capture semantics eliminate ad-hoc plumbing. |
| 11 | **Global closure environment capture** | ✅ Complete - `Value::Closure` now captures environment at creation time (not call time). Closures behave like closures in other languages. |
| 12 | **Consolidate dual evaluation paths** | ✅ Complete - Extracted shared `eval_binary_op`, `eval_builtin_fn`, and type conversion helpers into `timeline/eval_shared.rs`. Both tree-walker and IR/VM use shared logic. |

### Batch 7: Code Quality & Maintainability ✅
**Impact:** Moderate | **Effort:** Medium | **Dependencies:** None (can parallel with Batch 6)

These issues affect code maintainability and developer experience.

| # | Task | Status |
|---|------|--------|
| 13 | **Bundle graph parameters into context objects** | ✅ Complete - `GraphContext` struct bundles `x_domain`, `y_domain`, `size`, `at`, `padding`, `relative`. All graph functions updated. |
| 14 | **Add parent field to AnimationTrack** | ✅ Complete - `parent: Option<String>` field added to `AnimationTrack`. O(1) parent lookup via `track.parent()`. Populated during build. |
| 15 | **Establish "never silently drop" convention** | ✅ Complete - Established convention: always log or comment silent drops. Added `tracing::warn!` for unrecognized property values. Documented in AGENTS.md. |
| 16 | **Optimize nested blend evaluation** | ✅ Complete - Flattened nested blends into weighted sum: `blend(A, blend(B, C, p), q)` → `q*A + (1-q)*p*B + (1-q)*(1-p)*C`. O(N) instead of O(2^N). |

### Batch 8: Cleanup & Polish ✅
**Impact:** Low | **Effort:** Low-Medium | **Dependencies:** None

Minor fixes and technical debt cleanup.

| # | Task | Status |
|---|------|--------|
| 17 | **Fix remaining for loop bugs** | ✅ Complete - Loop variables cleared after exit. `for_iter_values` spreads `Value::List` correctly. IR lowerer threads `index_var` through. |
| 18 | **Align PEG parser and tree-sitter grammar** | ✅ Complete - Added CI sync check (`scripts/check-parser-sync.sh`). Documented sync strategy in AGENTS.md. Both parsers handle same feature set. |
| 19 | **Deprecate legacy compatibility shims** | ✅ Complete - `sample_procedural_plot` and `build_implicit_plot_path` marked `#[deprecated]`. Call sites updated to use new functions. |
| 20 | **Make FFmpeg truly optional for GUI** | ✅ Complete - GUI `Cargo.toml` now has optional `video` feature. FFmpeg-dependent code gated behind feature flag. Documented in AGENTS.md. |

### Batch 10: Correctness Triage & Hygiene
**Impact:** Potentially Critical (if #21 is a real bug) / Low otherwise | **Effort:** Low | **Dependencies:** None

Triage the only potentially-real correctness bug before bigger refactors, and clear policy violations.

| # | Task | Analysis | Fix Path |
|---|------|----------|----------|
| 21 | **Triage & fix `test_hierarchical_assignment_target`** | Test at `timeline/tests/build.rs:108` asserts `circ.opacity=0.0 at t=0` (pre-keyframe "hidden" default) and `0.5 at t=1s` for hierarchical target `g.circ.opacity = 0.5` at `#+1s`. Currently failing, marked "pre-existing". | Run the test, read the actual failure. If real bug: fix hierarchical target resolution in `assignments.rs` / `build/process.rs`. If stale test: update assertion to match current semantics. If real bug, promote to hotfix ahead of Batch 11. |
| 22 | **Justify or remove unjustified `#[allow(dead_code)]`** | AGENTS.md requires an inline justification comment on every `#[allow(dead_code)]`. Known sites: `renderer/text.rs:1201,1423`, `timeline/build/mod.rs:39`, `timeline/svg_import.rs:65,76,84,95`, `timeline/plot.rs:187,215`. | For each: keep + add justification comment if forward-looking; remove the item if truly dead. Verify: `grep -rn "allow(dead_code)" crates/` shows every hit has a trailing `//` justification. |

### Batch 11: Modifier VM — Closure Support & Loop-State Cleanup
**Impact:** Moderate | **Effort:** Medium-High | **Dependencies:** None (schedule after Batch 10 so CI is green)

Closures in modifiers work via fallback (bytecode→IR→tree-walker) but emit misleading warnings and disable bytecode for the entire modifier program. Loop state uses fragile magic strings.

| # | Task | Analysis | Fix Path |
|---|------|----------|----------|
| 23 | **First-class closure and `Construct` support in IR/VM** | Closures work via graceful fallback chain (lower→Unsupported→bytecode error→IR→tree-walker per-expression), but emit "Bytecode compilation failed" warning and disable bytecode for the whole modifier program. `Expr::Construct` shares the same gap. | Add `CompiledExpr::Closure` and `Construct` arms to `ir/lower.rs`. Add VM instructions to push/invoke closures. Handle new arms in `ir/eval.rs`. Goal: bytecode compiles cleanly, no warning, no perf cliff. |
| 24 | **Remove magic loop-variable strings from VM** | `vm.rs:505–555` tracks loop state via `frame_env.set("__for_iter_{pat_key}")` / `"__for_idx_{pat_key}"`. Fragile; collides if a user names a variable `__for_iter_*`. | Add `loop_stack: Vec<LoopState>` to VM struct. BeginFor pushes, CheckFor reads/pops from stack. Keep user-facing loop-var binding (legitimate env entries). |

### Batch 12: Graph Subsystem Type Safety
**Impact:** Moderate | **Effort:** Medium | **Dependencies:** None

Pure refactors for type safety and clarity in the graph coordinate system.

| # | Task | Analysis | Fix Path |
|---|------|----------|----------|
| 25 | **Replace string scale types with `ScaleType` enum** | `"linear"`/`"log"` strings compared across `build/utils.rs`, `build/plot.rs`, `build/property.rs`, `assignments.rs`. Allows typos, no compile-time safety. | Define `pub enum ScaleType { Linear, Log }`. Change `GraphContext.x_scale/y_scale: String` → `ScaleType`. Replace `== "log"` with `matches!(scale, ScaleType::Log)`. User-facing parse input stays a string, converted at build boundary. |
| 26 | **Split `GraphContext` static vs dynamic fields** | `GraphContext` mixes static config (`x_domain, y_domain, x_scale, y_scale`) with per-frame geometry (`size, at, padding, relative`). | Split into `GraphScaleConfig` (immutable per actor) and `GraphGeometry` (per-frame). `graph_math_to_screen` takes `(&GraphScaleConfig, &GraphGeometry)`. Pure refactor, no behavior change. |

### Batch 13: Persistence & Serialization
**Impact:** Moderate | **Effort:** Medium-High | **Dependencies:** **Batch 11** (Value/CapturedEnv/closure shape must be stable)

Replace wholesale `clone()` persistence with proper serde. Cross-crate AST serde decision is the long pole.

| # | Task | Analysis | Fix Path |
|---|------|----------|----------|
| 27 | **Add serde to `AnimationTrack`/`Value`; replace `clone()` persistence** | `AnimationTrack` has no serde derives; `persistence.rs:305` does `track.clone()` for frame snapshots. `ast::Expr` has no serde. `Value` carries `NativeFn` (Rust fn pointer) which is not serializable. | (a) Add serde to `animatix-syntax` AST types (cross-crate, gate behind `serde` feature). (b) Custom serde for `Value`: serialize `FuncSource::Raw`; error/skip on `NativeFn`. (c) Derive serde on `AnimationTrack`. (d) Keep `clone()` for frame snapshots, use serde for disk save/load. |

### Batch 14: Env Capture Invariant
**Impact:** Low-Moderate | **Effort:** Low-Medium | **Dependencies:** None if "harden invariant" chosen

`CapturedEnv` only captures overrides, relying on the implicit invariant that stdlib `base` is re-provided at render time.

| # | Task | Analysis | Fix Path |
|---|------|----------|----------|
| 28 | **CapturedEnv capture semantics** | `CapturedEnv::snapshot` clones only `env.overrides`; relies on stdlib `base` being re-provided at render time via `build_frame_env`. Works today but the invariant is implicit. | **Recommended (B):** Harden the invariant — add `debug_assert!` at render time that `base` is present, document the guarantee. **Alternative (A):** Capture full env (correct but clones stdlib into every closure; perf cost). Adopt (A) only if a concrete render-time-only variable breaks the invariant. |

---

## Icebox

Not strictly needed, ones that require more design, or simply weird thoughts that came to mind. Should be ignored when planning for implementation, in most cases.

| Task | Reason |
|------|--------|
| **Scene primitive / picture-in-picture** | Transition blending shipped; existing components and `Stack` cover most reuse cases. |
| **Export performance: pre-compiled plot closures** | Only matters for many plot actors or heavy sampled fields. |
| **Asset usage tracking** | Show which actors reference an asset; no strong user story yet. |
| **Variable track UI** | GUI for `let` variable tracks; `always` blocks cover most interactive cases. |
| **Module dependency graph** | Visual graph of `.amx` imports; internal tooling value only so far. |
| **Lossless whitespace/trivia preservation** | Current write-back pipeline correct for all normal use cases; comments roundtrip, formatting idempotent. |
| **APNG export** | Request-driven only; GIF covers lightweight previews, video/WebM covers higher-quality sharing. |
| **Source-diff preview sidecar** | Show the `.amx` diff when dragging actors or editing properties in the inspector. |
| **Animation heatmap view** | Heatmap of animated property density across time, actors, categories. Useful for large generated `.amx` files. |
| **Auto-sorted property registry** | Keep manually sorted with `registry_is_sorted` guard; proc-macro adds more maintenance surface than it removes. |
| **Interactive step control (presentational mode)** | Manim-style `wait()` / `next_slide()`. Architecturally incompatible with Animatix's declarative deterministic playback model. GUI scrubbing covers most use cases. |
| **Auto-arrow layout** | Arrows that auto-connect actor positions. Niche use case; workaround via manual `Arrow` with hardcoded coords. |
| **Per-actor exit before scene transition** | Animate individual actors out before `play SceneName [fade, ...]`. Workaround: `fade-out` actions timed at scene end. Transition blending is already uniform. |
| **Merge tree-walker and IR/VM into single execution engine** | Long-term, high-risk unification. Batch 6 (#12) extracted shared helpers so duplication is bounded. The dual-path-with-fallback is currently a *safety feature* (it makes closures non-critical). Needs a design spike before scheduling. |
| **Full `typst_shorthand` (`$$…$$`) parser sync** | Known Batch-8 leftover. Requires tree-sitter external scanner (C) changes, not just grammar edits. Highlighting-only impact today (PEG parser handles `$$…$$` correctly). Pull into a batch only after a scanner spike. |
