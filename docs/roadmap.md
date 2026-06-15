# Animatix Roadmap

Keep track of what is yet to be done here, when a segment is fully done, remove the items from here.

---

## Planned

### Architecture & Maintainability

#### Assessment Summary

| Observation | Valid? | Severity | Priority | Fix scope | Risk |
|---|---|---|---|---|---|
| Two formatters, unclear relationship | Partly | Low | Later | 2-3 syntax/doc files, 0.5-1 day | Low: mostly naming/docs unless merging APIs |
| GUI crate duplicates AST matches | Partly | Medium | Soon | 4-8 GUI/syntax files; `apply.rs`, `ast_utils.rs`, `scene_edits.rs`, `actor_edits.rs` migrated | Remaining: format_core, inline_actions, rewrite (deep), symbol_table |
| Variant coverage guardrails | Done | — | — | 4 guardrail tests in `format_core.rs` + `apply.rs`; runtime tests alert on new variants | — |

#### P1 Soon

- **Migrate walkers to shared traversal** (partial — `discovery.rs`, `expand.rs`, `diagnostics.rs`, `rewrite.rs` expr, GUI `apply.rs`, `ast_utils.rs`, `scene_edits.rs`, `actor_edits.rs` migrated) — Remaining: `format_core.rs`, `inline_actions.rs`, `rewrite.rs` (stmt/inline helpers), `symbol_table.rs`


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
