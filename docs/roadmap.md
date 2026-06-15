# Animatix Roadmap

Keep track of what is yet to be done here, when a segment is fully done, remove the items from here.

---

## Planned

### Architecture & Maintainability

#### Assessment Summary

| Observation | Valid? | Severity | Priority | Fix scope | Risk |
|---|---|---|---|---|---|
| AST change propagation is painful | Resolved | — | — | Shared walk layer in `crates/animatix-syntax/src/walk.rs`; 3 call sites migrated | Residual risk in unmigrated walkers (P1) |
| No shared AST traversal layer | Resolved | — | — | `walk_stmts`, `walk_expr`, `walk_inline_items` + find helpers shipped in `walk.rs` | — |
| Parser is a monolith | Resolved | — | — | Split into 5 submodules (`common`, `expr`, `inline`, `stmt`, `top_level`) | — |
| Two formatters, unclear relationship | Partly | Low | Later | 2-3 syntax/doc files, 0.5-1 day | Low: mostly naming/docs unless merging APIs |
| `process_plot_actor` returns a 13-tuple | Resolved | — | — | Replaced with `ProcessedPlotActor` named struct | — |
| Property registry manual sorting | No | Low | Icebox | 1-3 files, 0.5-1 day if ever automated | Medium: proc-macro/build-script complexity exceeds current benefit |
| Duplicated for-loop iteration logic | Resolved | — | — | Centralized via `Timeline::process_for_loop_stmts` / `process_for_loop_inline_items` | — |
| GUI crate duplicates AST matches | Partly | Medium | Soon | 4-8 GUI/syntax files; `apply.rs`, `ast_utils.rs`, `scene_edits.rs`, `actor_edits.rs` migrated | Remaining: format_core, inline_actions, rewrite (deep), symbol_table |
| No compile-time test for exhaustive variant coverage | Partly | Low | Later | 1-2 files, 0.5 day | Low-medium: tests can become brittle without reducing runtime risk |
| Pre-existing friction points | Resolved | — | — | FFT example already validated in CI + integration tests; CI has ffmpeg; no changes needed | — |

#### P1 Soon

- **Migrate walkers to shared traversal** (partial — `discovery.rs`, `expand.rs`, `diagnostics.rs`, `rewrite.rs` expr, GUI `apply.rs`, `ast_utils.rs`, `scene_edits.rs`, `actor_edits.rs` migrated) — Remaining: `format_core.rs`, `inline_actions.rs`, `rewrite.rs` (stmt/inline helpers), `symbol_table.rs`

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
