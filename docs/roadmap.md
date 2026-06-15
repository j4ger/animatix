# Animatix Roadmap

Keep track of what is yet to be done here, when a segment is fully done, remove the items from here.

---

## Planned

### Architecture & Maintainability

#### Assessment Summary

| Observation | Priority | Fix scope |
|---|---|---|
| AST change propagation | Done | Shared walk layer + full migration of all 29 identified walker functions across 8 files |
| Parser monolith | Done | Split into 5 submodules (`common`, `expr`, `inline`, `stmt`, `top_level`) |
| `process_plot_actor` 13-tuple | Done | Replaced with `ProcessedPlotActor` named struct |
| Formatter boundary | Done | Module docs in `format_core.rs`/`to_source.rs` + architecture.md note |
| For-loop duplication | Done | Centralized via `process_for_loop_stmts` / `process_for_loop_inline_items` |
| GUI AST match duplication | Done | All 7 GUI source_edit walk functions migrated to shared layer |
| Variant coverage guardrails | Done | 4 guardrail tests + explanatory comments at incompatible sites |
| Pre-existing friction | Done | Verified: FFT example in CI, ffmpeg gated, no changes needed |
| Property registry sorting | Icebox | — |


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
