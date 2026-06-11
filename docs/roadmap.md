# Animatix Roadmap

> What's left to build. For the language spec, see [`spec.md`](spec.md); for architecture, see [`architecture.md`](architecture.md); for GUI architecture, see [`contributing.md` §GUI Data Flow](contributing.md#gui-data-flow).

---

## P0 — GUI Correctness *(complete)*

## P1 — GUI Architecture Integration *(complete)*

## P2 — Animation Workflow *(complete)*

## P3 — Polish & Performance *(complete)*

## P4 — UI Audit & Hardening

| Priority | Task | Effort | Notes |
|----------|------|--------|-------|
| **P0** | **Fix dead feedback channel**: render `preview.status` in a visible status bar, or convert all 135 write sites to toasts | 1d | Every error/confirmation routed through `.status` is silently overwritten each frame by `live_preview_status()`. |
| **P0** | **Guard `T`-key time lens behind `egui_wants_keyboard_input()`** | 0.5d | Fires during text input in cell editor, rename fields, explorer filter. All other global shortcuts have this guard. |
| **P0** | **Fix spreadsheet "Add" actor type**: use `default_actor_type()` + `unique_label()` instead of `"rect"` + `"actor_N"` | 0.5d | `"rect"` (lowercase) doesn't match primitive registry "Rect"; hardcoded label may collide. |
| **P1** | **Deduplicate zoom/pan state**: remove from `UiStore::snapshot()`; read from `PreviewStore` directly | 1d | Undo snapshots record zoom=1.0/pan=0.0 because `UiStore` copies are never written after init. |
| **P1** | **Repopulate insertion palette on open**: don't gate on `items.is_empty()` | 1d | Components/actions added after first open are invisible; deleted ones remain insertable. |
| **P1** | **Gate timeline zoom behind Ctrl/Cmd+wheel** | 0.5d | Plain wheel zooms AND scrolls the `ScrollArea` simultaneously — timeline with many tracks is nearly unscrollable. |
| **P1** | **Fix bulk keyframe delete**: iterate property tracks directly instead of `collect_actor_keyframes()` which dedups by time | 1d | Two properties keyframed at the same time: only one property's keyframe is deleted. |
| **P1** | **Include `track_idx` in action drag matching** | 0.5d | `is_action_drag` matches only `start_time_ms` — dragging one block highlights same-time blocks on all tracks. |
| **P1** | **Fix layer tree drop-to-root 100px halo**: show drop indicator and shrink expand region | 1d | Dropping near (but not on) the tree silently reparents. No feedback shown. |
| **P1** | **Multi-selection inspector: show all selected actors' common properties** instead of iterating `HashSet` | 1-2d | Currently shows a non-deterministic single actor's properties; edits target that arbitrary actor only. |

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
