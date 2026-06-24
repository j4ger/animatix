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

### Batch 6: Environment Model Unification (Foundation)
**Impact:** Critical | **Effort:** High | **Dependencies:** None (but blocks other fixes)

These issues stem from the fundamental disconnect between build-time and render-time environments.

| # | Task | Analysis | Fix Path |
|---|------|----------|----------|
| 10 | **Unified build/render environment model** | Most serious architectural flaw. `Timeline.env` (build-time) and frame env (render-time) are separate systems, causing closure capture bugs and for loop variable leaks. Every feature crossing this boundary needs custom plumbing. | Design unified environment with explicit capture semantics. Create `CapturedEnv` type that snapshots build-time values needed at render time. Thread captured values through `ProceduralPlot` and similar structures. |
| 11 | **Global closure environment capture** | `Value::Closure` only captures environment for plot functions (Batch 3 patch). General case broken: `for freq in [1,2,3] { let f = (x) => sin(x * freq) }` doesn't capture `freq` unless used as plot func. Closures don't behave like closures in other languages. | Change `Value::Closure` from `(params, body)` to `(params, body, captured_env)`. Capture environment at closure creation time globally, not just for plots. Update all closure creation sites. |
| 12 | **Consolidate dual evaluation paths** | Tree-walker (`utils.rs`) and IR/VM (`ir/eval.rs`) remain separate despite `eval_method` consolidation. Every new feature requires parallel updates to both paths. High maintenance burden, easy to introduce inconsistencies. | Extract more shared evaluation logic into common functions. Consider merging into single evaluation engine long-term. At minimum, create shared helpers for common patterns (binary ops, function calls, etc.). |

### Batch 7: Code Quality & Maintainability
**Impact:** Moderate | **Effort:** Medium | **Dependencies:** None (can parallel with Batch 6)

These issues affect code maintainability and developer experience.

| # | Task | Analysis | Fix Path |
|---|------|----------|----------|
| 13 | **Bundle graph parameters into context objects** | Adding padding (Batch 2) required updating 10+ function signatures: `graph_math_to_screen(mx, my, x_domain, y_domain, size, at, padding, relative)`. Error-prone and hard to maintain. | Create `GraphContext { x_domain, y_domain, size, at, padding, relative }` struct. Update all graph-related functions to accept `&GraphContext` instead of individual parameters. Reduces signature complexity and makes adding new parameters easier. |
| 14 | **Add parent field to AnimationTrack** | Finding parent requires O(n) scan of all tracks. Caused performance issues in exclusive highlight groups (Batch 1) and makes parent-child queries awkward. | Add `parent: Option<String>` field to `AnimationTrack`. Populate during build when processing nested actors. Add helper methods `track.parent()` and `track.children()` for O(1) lookup. |
| 15 | **Establish "never silently drop" convention** | Multiple properties silently dropped unrecognized values (tech debt #16). Systemic pattern from direct `Expr` matching instead of using evaluation helpers. Led to subtle bugs where invalid values were ignored. | Establish codebase convention: "Always evaluate, never silently drop." Add code review checklist item. Consider adding lint or runtime warning when properties receive unrecognized values. Audit remaining direct `Expr` matches. |
| 16 | **Optimize nested blend evaluation** | `FuncSource::Blend` creates tree where each level doubles evaluation cost. N-deep cascading transitions = 2^N evaluations per sample point. Adaptive quality (Batch 4) is mitigation, not fix. | Cache intermediate blend results at each level. Alternative: flatten nested blends into weighted sum of base functions: `blend(A, blend(B, C, 0.5), 0.5)` → `0.5*A + 0.25*B + 0.25*C`. Reduces to O(N) instead of O(2^N). |

### Batch 8: Cleanup & Polish
**Impact:** Low | **Effort:** Low-Medium | **Dependencies:** None

Minor fixes and technical debt cleanup.

| # | Task | Analysis | Fix Path |
|---|------|----------|----------|
| 17 | **Fix remaining for loop bugs** | Partially fixed in Batch 3, but issues remain: (1) Variable leaks - last value persists after loop exits, (2) `for_iter_values` bug - `Value::List` variables not spread properly, (3) IR lowerer drops `index_var` silently. | (1) Clear loop variables from env after loop exits. (2) Fix `for_iter_values` to spread `Value::List` when iterable is a variable. (3) Pass `index_var` through IR lowering. Add tests for all three cases. |
| 18 | **Align PEG parser and tree-sitter grammar** | Two parsers for same language can drift apart. Updated both when adding features (Batch 3), but no automated check ensures they stay in sync. Maintenance burden. | Keep manually in sync for now. Consider generating tree-sitter grammar from PEG parser definitions (if feasible). Add CI check that runs both parsers on test suite and compares AST structure. |
| 19 | **Deprecate legacy compatibility shims** | `sample_procedural_plot` delegates to `sample_procedural_plot_at`. Other legacy functions may exist. Technical debt from incremental refactoring. | Audit codebase for legacy shims. Add `#[deprecated]` attributes with migration guidance. Update all call sites to use new functions. Remove shims after one release cycle. |
| 20 | **Make FFmpeg truly optional for GUI** | GUI crate can't build without FFmpeg system libraries (tech debt #23 partially addressed). Barrier to entry for new contributors who just want to work on non-video features. | Move FFmpeg-dependent code behind feature flag. Provide stub implementations when feature disabled. Update `Cargo.toml` to make `video` feature truly optional. Document how to build without FFmpeg. |

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
