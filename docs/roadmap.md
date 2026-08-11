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

Structural risks identified during the 2026-08-11 cleanup. These are design
questions, not scheduled implementation tasks; settle the target architecture
before starting.

| Item | Status / Notes |
|------|----------------|
| Semantic AST single source | Open. Chumsky is the runtime/module semantic source, but `animatix-analyzer::Workspace` still builds symbols from the tree-sitter converter AST. One semantic AST should feed runtime, module loading, type checking, and analyzer; tree-sitter should remain a CST/position/incremental frontend only. |
| Module/Workspace resolver unification | Open. `ModuleGraph` and analyzer `Workspace` are two import/namespace resolvers with different component, action, scene, and alias semantics. Add behavior-equivalence tests before reusing one from the other. |
| Semantic diagnostics single emitter | Open. Label/action/property/type checks are duplicated between `animatix-syntax` typecheck/diagnostics and `animatix-analyzer::diagnostics`. One canonical diagnostic type/code/emitter with one LSP DTO conversion point is the target. |
| Path/source-map model | Open. `ModuleGraph` mixes raw in-memory paths and canonicalized disk paths as keys. Extract a normalized `SourceMap`/path resolver so import lookup, cache keys, and source overrides share one identity model. |
| Source override lifecycle | Open. `load_program_with_source` manually inserts, loads, then restores/removes source. A scoped API such as `with_source(path, source, \|graph\| ...)` would make cleanup failure-proof. |
| GUI mutation/cache/snapshot convergence | Open. `commit_source` and `replace_text` now both invalidate GUI caches, but the invariant is still manual. Snapshot finalize/abort is also per-handler; centralize mutations and use a mutation guard. |
| Rebuild worker lifecycle | Open. The worker thread is detached and has no automatic restart after failure; long-lived GUI sessions can silently lose background rebuilds. Decide between supervised restart, fallback synchronous rebuild, or explicit unsupported state. |
| Type model vs annotation grammar | Open. Internal `Type::Vec3/Tuple/Function` currently degrade to `Any` annotations because the language grammar has no corresponding forms. Expand only when parser, tree-sitter, typechecker, and analyzer can be updated together. |
| Parser-sync AST equivalence | Open. `check-parser-sync.sh` verifies tree-sitter parses examples, not that its converted AST matches the semantic parser. Add corpus-level AST equivalence tests for actions, keyframes, scenes, modifiers, and cross-file modules. |

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
