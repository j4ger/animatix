# Animatix Roadmap

> What's left to build. For the language spec, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

---

## Component & Module System

No remaining tasks.

---

## Multi-Scene Composition

No remaining tasks.

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
