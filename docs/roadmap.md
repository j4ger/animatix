# Animatix Roadmap

> What's left to build. For the language spec, see [`spec.md`](spec.md); for architecture, see [`architecture.md`](architecture.md); for GUI architecture, see [`contributing.md` §GUI Data Flow](contributing.md#gui-data-flow).

---

## P0 — GUI Correctness *(complete)*

## P1 — GUI Architecture Integration *(complete)*

## P2 — Animation Workflow *(complete)*

## P3 — Polish & Performance *(complete)*

## P4 — UI Audit & Hardening *(all complete)*

## Icebox

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
