# Animatix Roadmap

> What's left to build. Completed work should be removed, not marked done. For the language spec, see [`spec.md`](spec.md); for architecture, see [`architecture.md`](architecture.md).

---

## Source Editing

| Task | Effort | Notes |
|------|--------|-------|
| **Lossless whitespace/trivia preservation** | 6–8 weeks | Current `source_edit` + formatter preserve comments and produce stable source, but still normalize formatting. Defer until users report formatting loss that the current serializer cannot handle. |

---

## Export & Media

| Task | Effort | Notes |
|------|--------|-------|
| **Web canvas / WASM export** | 1–2 months | Requires a WASM-friendly renderer/export path and dependency audit; not just a CLI encoder change. |
| **Audio playback in GUI preview** | 1 week | Export/muxing exists; preview still needs an audio output backend such as `rodio` or `cpal` and timeline sync. |
| **APNG export** | 3 days | Image/GIF/video export paths exist; add APNG encoding and CLI surface. |

---

## Renderer & Import Gaps

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
