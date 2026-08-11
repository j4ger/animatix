# Animatix Roadmap

Canonical source of truth for remaining work. When a segment is fully done,
remove the completed items from this file. Detailed planning documents and
historical archives live in [docs/plans/README.md](plans/README.md).

---

## Active Work

### eparts Framework Expansion (committed, unscheduled)

The `eparts` crate has an active framework-expansion track that is committed but
not yet scheduled. It includes JSON themes, hot-reload, table/chart/webview
surfaces, i18n, accessibility depth, a gallery app, CI parity, and related
framework widgets.

The full itemized list and sequencing guidance are in
[docs/plans/eparts-refinement-roadmap.md](plans/eparts-refinement-roadmap.md)
section `6.X`. First candidates when capacity opens are the gallery app, JSON
themes, CI platform parity, and `StyledExt` helpers.

### Architecture Consolidation

Structural risks identified during the 2026-08-11 cleanup. Most items are now
implemented; the remaining two are design questions that still need a settled
target before implementation.

| Item | Status / Notes |
|------|----------------|
| Semantic AST single source | Done. `parse_canonical` is the Chumsky semantic source; analyzer uses tree-sitter only as CST for positions/completions/incremental edits. |
| Module/Workspace resolver unification | Behavior aligned. `Workspace` and `ModuleGraph` share `SourceMap` identity/import resolution, canonical semantic AST, and a behavior-equivalence corpus covering pub let/type/component/scene/nested namespace exports. Full structural merge remains deferred: `Workspace` is a cloneable virtual-path symbol model used by LSP, while `ModuleGraph` owns disk/source-map loading and loaded programs; merging them requires designing one shared resolved-program model without coupling LSP to I/O. |
| Semantic diagnostics single emitter | Done. `animatix-syntax::semantic_diagnostics` is the canonical emitter; analyzer and LSP convert DTOs instead of re-implementing checks. |
| Path/source-map model | Done. `animatix-syntax::module::source_map` owns normalized path identity, import resolution, and in-memory source overrides. |
| Source override lifecycle | Done. `ModuleGraph::with_source` scopes temporary overrides and restores/removes them on both success and error. |
| GUI mutation/cache/snapshot convergence | Done for the core path. `commit_source`/`replace_text` invalidate caches, and `DocumentStore::with_mutation` scopes snapshot finalize/abort. Remaining handlers can migrate opportunistically. |
| Rebuild worker lifecycle | Done. `RebuildWorker::submit` restarts a dead worker thread. |
| Type model vs annotation grammar | Open. Internal `Type::Vec3/Tuple/Function` still degrade to `Any` annotations because the language grammar has no corresponding forms. Expand only when parser, tree-sitter, typechecker, and analyzer can be updated together. |
| Parser-sync AST equivalence | Partially done. Corpus-level equivalence tests now cover actions, keyframes, scenes, modifiers, shorthand, for loops, reactive bindings, sequence/stagger, component/action definitions, method/if expressions, and parameter defaults. Tree-sitter converter gaps found by this corpus have been fixed. Expand coverage as new syntax lands. Note: spec's `match` statement examples omit arm commas while the PEG parser requires them; decide and document the canonical form before adding `match` to the equivalence corpus. |

### GUI Follow-Ups

| Item | Status / Notes |
|------|----------------|
| Opportunistic eparts widget adoption | Partially complete; remaining call sites migrate when the surrounding GUI area is next edited. |

### Language and Runtime Gaps

| Item | Status / Notes |
|------|----------------|
| Precise shape/path/text bounds | Open; callout geometry and actor anchor points use world-space affine plus available local bounds. Exact text/path bounds remain deferred. |
| Text/Typst/Code frame-time content overrides | Open; timed assignments recompile glyph paths, but changing `text` directly inside `always` is not a supported path. |
| Data-dependent algorithm timelines | Open; no runtime mutable state or branching timeline, so algorithm animations must be hand-unrolled recordings. Confirmed by `dogfood/projects/sorting-visualizer`. |

---

## Audit History

The 2026-08-05 audit trail is archived at
[docs/plans/archive/roadmap-audit-2026-08-05.md](plans/archive/roadmap-audit-2026-08-05.md).
Future sessions should read `Active Work` above for current remaining items and
consult the archive only for prior findings and resolution context.

---

## Icebox

Not strictly needed, ones that require more design, or simply weird thoughts that
came to mind. Should be ignored when planning for implementation, in most cases.
Audit status is from 2026-08-05.

| Task | Reason / Audit Status |
|------|-----------------------|
| **Scene primitive / picture-in-picture** | Transition blending shipped; existing components and `Stack` cover most reuse cases. Unchanged. |
| **Asset usage tracking** | Show which actors reference an asset; no strong user story yet. Unchanged. |
| **Variable track UI** | GUI for `let` variable tracks; `always` blocks cover most interactive cases. Unchanged. |
| **Module dependency graph** | Visual graph of `.amx` imports; internal tooling value only so far. Unchanged. |
| **Lossless whitespace/trivia preservation** | Current write-back pipeline correct for all normal use cases; comments roundtrip, formatting idempotent. Unchanged. |
| **APNG export** | Request-driven only; GIF covers lightweight previews, video/WebM covers higher-quality sharing. Unchanged. |
| **Source-diff preview sidecar** | Show the `.amx` diff when dragging actors or editing properties in the inspector. Unchanged. |
| **Animation heatmap view** | Heatmap of animated property density across time, actors, categories. Useful for large generated `.amx` files. Unchanged. |
| **Auto-sorted property registry** | Keep manually sorted with `registry_is_sorted` guard; proc-macro adds more maintenance surface than it removes. Unchanged. |
| **Interactive step control (presentational mode)** | Manim-style `wait()` / `next_slide()`. Architecturally incompatible with Animatix's declarative deterministic playback model. GUI scrubbing covers most use cases. Unchanged. |
| **Auto-arrow routing / smart connector layout** | Actor anchor-point endpoint refs (`from: n0.right`, `to: n1.left`) cover manual auto-tracking. Remaining value is automatic edge routing/relayout, still niche. |
| **Per-actor exit before scene transition** | Animate individual actors out before `play SceneName [fade, ...]`. Workaround: `fade-out` actions timed at scene end. Transition blending is already uniform. Unchanged. |
