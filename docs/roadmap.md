# Animatix Roadmap

> What's left to build. Completed work should be removed, not marked done. For the language spec, see [`spec.md`](spec.md); for architecture, see [`architecture.md`](architecture.md).

---

## P0 — GUI Correctness

*All P0 tasks complete.*

---

## P2 — Export & Media

| Task | Effort | Notes |
|------|--------|-------|
| **Audio playback in GUI preview** | 1 week | Export/muxing exists; preview still needs an audio output backend such as `rodio` or `cpal` and timeline sync. |
| **WebM export** | 1 week | More useful than APNG for browser embedding and sharing; add VP9/AV1-capable encoding path and CLI surface alongside existing GIF/video export. |
| **Web canvas / WASM export** | 1–2 months | Requires a WASM-friendly renderer/export path and dependency audit; not just a CLI encoder change. |

---

## P3 — Renderer & Import Gaps

| Task | Effort | Notes |
|------|--------|-------|
| **Zero-readback filter compositing** | Unknown | GPU filter compute exists, but scene evaluation/export still composites filtered results through CPU image readback. Wire filtered texture compositing through the renderer when the render path can consume it directly. |
| **SVG `<mask>` import conversion** | Unknown | Runtime masking exists, but the SVG importer skips `<mask>`. Add importer support that maps SVG masks to Animatix runtime masks. |
| **Scene primitive / picture-in-picture** | Needs design | Transition blending is shipped; keep only if there is a concrete use case for embedding one scene timeline inside another. Existing components and `Stack` cover many reuse cases. |

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
