# Animatix Roadmap

Keep track of what is yet to be done here, when a segment is fully done, remove the items from here.

---

## Planned

Items are grouped into implementation batches based on dependencies and shared systems.

### New features
New primitive types for educational content.

| # | Task | Notes |
|---|------|-------|
| 9 | **Callout/annotation primitive** | ✅ Complete. Coordinate-based and targeted modes both fully implemented. Targeted mode uses typed `CalloutPlace` enum, world-space bounds resolution for nested transforms, build-time `CalloutTargetNotFound` diagnostic, shared geometry helper, narrow `TargetResolver` API, and GUI affordances (place edge handles, standoff drag, Shift-detach). See `examples/callout_example.amx`. |

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
