# Animatix Roadmap

Keep track of what is yet to be done here, when a segment is fully done, remove the items from here.

---

## Planned

### BarChart Primitive — resolves FFT gap G1 ✅

**Status:** Implemented.

- `BarChartPrimitive` in `crates/animatix/src/primitives/bar_chart.rs`
- `ActorKindId::BarChart` registered in `track.rs`
- Properties: `data`, `bar_width`, `bar_colors`, `direction`, `max_value`, `show_axis`, `show_labels`
- `build_bar_chart_paths()` in `build/plot.rs` produces rectangle VelloPaths
- Standalone and `Graph`-child modes supported
- Label generation deferred (manual `Text` actors for now)
- FFT spectrum scene updated: `examples/fft_explain.amx`
- G1 in `docs/FFT_THOUGHT_EXPERIMENT.md` marked resolved

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
