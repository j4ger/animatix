# Animatix Roadmap

> What's left to build. Completed work should be removed, not marked done. For the language spec, see [`spec.md`](spec.md); for architecture, see [`architecture.md`](architecture.md).

---

## P0 — GUI Correctness

*All P0 tasks complete.*

---

## P2 — Export & Media

*All P2 tasks complete.*

---

## P3 — Renderer & Import Gaps

| Task | Effort | Notes |
|------|--------|-------|
| **Zero-readback filter compositing** | Implemented | GPU filter compute (WGSL blur + color matrix via GpuFilterBackend) now supports zero-readback compositing via `PendingComposite` + fullscreen blit. Safe path activates when the filter is last-in-render-order; otherwise falls back to readback. Wired into OffscreenRenderer (export) and PreviewSurface (GUI preview). |
| **SVG `<mask>` import conversion** | Implemented | SVG importer now collects `<mask>` definitions via `collect_masks()`, removes `"mask"` from the skip list, and wraps masked elements in `Mask` actors with an invisible mask shape as the first child. Threaded through `convert_group` and `convert_use`. |
| **Scene primitive / picture-in-picture** | Deferred | Existing components and `Stack` cover most reuse cases. Would need a `SceneRef` actor kind + offscreen rendering via FilterBackend infrastructure. Revisit if a concrete use case emerges. |

---

## Icebox

| Task | Reason |
|------|--------|
| **Export performance: pre-compiled plot closures** | Only matters for many plot actors or heavy sampled fields. |
| **Asset usage tracking** | Show which actors reference an asset; no strong user story yet. |
| **Variable track UI** | GUI for `let` variable tracks; `always` blocks cover most interactive cases. |
| **Module dependency graph** | Visual graph of `.amx` imports; internal tooling value only so far. |
| **Lossless whitespace/trivia preservation** | The current write-back pipeline is correct for all normal use cases — comments roundtrip, formatting is idempotent. Formatter normalizes formatting deterministically per spec Appendix A; no user reports of formatting loss. If ever needed, effort: 2wk for value-level surgical editing (~90% coverage) up to 6wk for full patch-based approach. Deferred until users report concrete loss the serializer cannot handle. |
| **APNG export** | Request-driven only; GIF covers lightweight previews, video/WebM covers higher-quality sharing, and APNG files can be large. |
