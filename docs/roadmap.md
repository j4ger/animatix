# Animatix Roadmap

> What's left to build. For the language spec, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

---

## Performance

Build and runtime optimizations.

| # | Task | Impact | Effort | Description |
|---|------|--------|--------|-------------|
| 1 | **Incremental parsing (tree-sitter)** | High | 1–2 weeks | Use existing tree-sitter grammar for incremental parsing. Feed AST diffs to build pipeline. |
| 2 | **Expression evaluation memoization** | Medium | 1 day | Cache `(expr_ptr, env_hash) → Value` during build. Same expression in same environment produces same result. |
| 3 | **Incremental component expansion** | Medium | 2–3 days | Only re-expand changed component instances. Requires AST diffing or dirty-flag per component. |
| 4 | **Skip stmts_to_source for surgical edits** | Low | 1 day | For keyframe moves/resizes, use SourceIndex byte-range replacement instead of full AST re-serialization. |

---

## Component & Module System

| # | Task | Effort | Description |
|---|------|--------|-------------|
| 1 | **Warn on extra instance properties** | 1 day | `component_bindings` silently adds unmatched properties. Emit a diagnostic to catch typos. |
| 2 | **Consolidate `Import` types** | 0.5 days | `Stmt::Import` and standalone `Import` struct serve the same purpose. Eliminate the standalone struct. |

---

## Multi-Scene Composition

| # | Task | Effort | Description |
|---|------|--------|-------------|
| 1 | **Tree-sitter grammar for multi-scene** | 1 week | Update grammar for `# SceneName` and `play` syntax. Syntax highlighting is broken for multi-scene files. |
| 2 | **Cross-file scenes** | 1–2 weeks | Allow scenes to be defined in imported files and composed across modules. Needs design for how `play` edges reference imported scenes. |
| 3 | **Live preview transition blending** | 3–5 days | `TransitionCompositor` (WGSL shader) is implemented and used during export, but the `animatix render` live preview shows hard cuts only. Wire preview window to use `render_transition`. |

---

## Source Editor — Lossless AST

> 6–8 weeks. Defer until users report formatting loss. Current `source_edit` + formatter handles 90% of cases.

| # | Task | Effort | Blocker |
|---|------|--------|---------|
| 1 | **Trivia data structure** | 1 week | — |
| 2 | **Parser trivia capture** | 2–3 weeks | #1 |
| 3 | **to_source trivia emission** | 1 week | #2 |
| 4 | **Source edit trivia preservation** | 1–2 weeks | #3 |
| 5 | **Round-trip tests** | 1 week | #4 |

---

## Web Export & Media

> Blocked on renderer abstraction (big refactor). No quick wins.

| # | Task | Effort | Blocker |
|---|------|--------|---------|
| 1 | **Web canvas / WASM export** | 1–2 months | Renderer abstraction |
| 2 | **Audio playback in preview** | 1 week | Audio backend (rodio/cpal) |
| 3 | **APNG export** | 3 days | APNG encoder |

---

## Blocked on Upstream

No committed timeline. Waiting on external features or dependencies.

| # | Task | Blocked on | Notes |
|---|------|------------|-------|
| 1 | **Zero-readback GPU filters** | Vello GPU filter support ([#1296](https://github.com/linebender/vello/issues/1296)) | Phase 8.6a (GPU compute + readback) ships the perf win. Wait for Vello to eliminate the readback. |
| 2 | **SVG `<mask>` support** | Vello Scene API mask support | `vello_cpu` has it; `vello` proper does not. |
| 3 | **Scene primitive (PiP)** | Composition-level design | Existing components + Stack cover reuse. Revisit after transition blending. |

---

## Icebox

> Low demand or niche. Kept for reference.

| Task | Reason |
|------|--------|
| **Export performance: pre-compiled plot closures** | Only matters for dozens of plot actors. `always` block easing covers simpler cases. |
| **Asset usage tracking** | Show which actors reference an asset. No user stories. |
| **Variable track UI** | GUI for `let` variable tracks. `always` blocks cover most cases. |
| **Module dependency graph** | Visual graph of `.amx` imports. Internal tooling, no user stories. |
