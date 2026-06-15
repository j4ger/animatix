# Animatix Roadmap

Keep track of what is yet to be done here, when a segment is fully done, remove the items from here.

---

## Planned

### Architecture & Maintainability

#### Assessment Summary

| Observation | Valid? | Severity | Priority | Fix scope | Risk |
|---|---|---|---|---|---|
| AST change propagation is painful | Yes | High | Now | 6-10 files, 1-2 days for traversal layer; broader migration incremental | Medium: traversal mistakes can silently skip nested nodes |
| No shared AST traversal layer | Yes | High | Now | 3-6 files for shared walkers plus first call-site migrations, 1-2 days | Medium: edit/find behavior can regress if traversal order changes |
| Parser is a monolith | Yes | Medium | Soon | 4-6 parser files, 2-4 days | Medium-high: grammar precedence and diagnostics are easy to perturb |
| Two formatters, unclear relationship | Partly | Low | Later | 2-3 syntax/doc files, 0.5-1 day | Low: mostly naming/docs unless merging APIs |
| `process_plot_actor` returns a 13-tuple | Yes | Medium | Now | 2 files, 0.5 day | Low: mechanical refactor with compiler coverage |
| Property registry manual sorting | No | Low | Icebox | 1-3 files, 0.5-1 day if ever automated | Medium: proc-macro/build-script complexity exceeds current benefit |
| Duplicated for-loop iteration logic | Yes | Low | Soon | 2-3 files, 0.5 day | Low: small helper around existing behavior |
| GUI crate duplicates AST matches | Yes | Medium | Soon | 4-8 GUI/syntax files, 1-2 days after shared walkers exist | Medium: source edit and scene operations depend on exact traversal coverage |
| No compile-time test for exhaustive variant coverage | Partly | Low | Later | 1-2 files, 0.5 day | Low-medium: tests can become brittle without reducing runtime risk |
| Pre-existing friction points | Yes | Medium | Soon | 2-5 CI/dependency/example files, 1 day | Low-medium: CI/dependency feature changes can affect developer setup |

#### P0 Now

- **Shared AST traversal primitives** — Add reusable `walk_stmt`, `walk_stmts`, `walk_inline_item`, `walk_expr`, and mutable variants in `crates/animatix-syntax/src/ast.rs` or a new `crates/animatix-syntax/src/walk.rs`; first migrate `crates/animatix-syntax/src/module.rs`, `crates/animatix-syntax/src/source_index.rs`, and `crates/animatix-gui/src/source_edit/apply.rs` so future AST fields stop requiring broad manual edits.
- **Named plot actor build output** — Replace `Timeline::process_plot_actor()`'s `Option<(...)>` return in `crates/animatix/src/timeline/build/plot.rs` with a named struct such as `ProcessedPlotActor`, and update `crates/animatix/src/timeline/build/actor.rs`; expected outcome is safer BarChart/plot evolution without tuple-position bugs.

#### P1 Soon

- **Migrate duplicated AST walkers** — Move duplicated tree-walking code in `crates/animatix-syntax/src/format_core.rs`, `crates/animatix-syntax/src/module/discovery.rs`, `crates/animatix-syntax/src/module/expand.rs`, `crates/animatix-syntax/src/module/rewrite.rs`, `crates/animatix-syntax/src/module/inline_actions.rs`, `crates/animatix-analyzer/src/symbol_table.rs`, `crates/animatix-analyzer/src/diagnostics.rs`, and `crates/animatix-gui/src/source_edit/*.rs` onto the shared traversal layer; expected outcome is fewer out-of-sync AST variant matches.
- **Parser module split** — Extract expression, statement, inline item, and top-level grammar helpers out of `crates/animatix-syntax/src/parser/mod.rs` into the existing `parser/expr.rs`, `parser/stmt.rs`, `parser/inline.rs`, and `parser/top_level.rs`; expected outcome is smaller review surfaces for syntax changes while preserving parser tests.
- **For-loop lowering helper** — Centralize the `for_iter_values` → bind item/index → recurse sequence currently duplicated in `crates/animatix/src/timeline/build/process.rs` and `crates/animatix/src/timeline/build/container.rs`; expected outcome is one semantic path for programmatic actor generation.
- **Iteration friction cleanup** — Add explicit CI/example validation for `examples/fft_explain.amx`, keep video/FFmpeg support feature-gated around `rsmpeg` in `crates/animatix/Cargo.toml` and `.github/workflows/ci.yml`, and prefer `..Default::default()` constructors in touched AST/test code where appropriate; expected outcome is less time lost to environment and constructor churn.

#### P2 Later

- **Formatter boundary documentation** — Document that `crates/animatix-syntax/src/format_core.rs` owns raw formatting and `crates/animatix-syntax/src/to_source.rs` is the canonical 2-space serialization API, or rename APIs if the split remains confusing; expected outcome is clear ownership without unnecessary merging.
- **Variant coverage guardrails** — Add lightweight tests or macros near `crates/animatix-syntax/src/ast.rs`, `crates/animatix-syntax/src/format_core.rs`, and `crates/animatix-gui/src/source_edit/apply.rs` to force review of key exhaustive match sites when `Stmt`, `InlineItem`, or `Expr` variants change; expected outcome is earlier failures for missing formatting/edit traversal coverage.

#### Not Prioritized

- **Auto-sorted property registry** — Keep `crates/animatix/src/timeline/property_registry.rs` as a manually sorted static slice for now because `registry_is_sorted` already catches mistakes and a proc-macro/build-script would add more maintenance surface than it removes.

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
