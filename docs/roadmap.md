# Animatix Roadmap

Keep track of what is yet to be done here, when a segment is fully done, remove the items from here.

---

## P0 — Critical Bugs (UI Audit) *(complete)*

## P1 — High Impact *(complete)*

## P2 — Medium

| # | Task | File(s) | Details |
|---|------|---------|---------|
| 11 | **Segmented controls float on scroll** | `inspector/mod.rs:567-590, 717-740` | Positioned at `ui.clip_rect().min.y`; glues to viewport top when scrolling. Anchor to card header. |
| 12 | **Bulk delete keyframes misses fields** | `timeline_panel.rs:1259-1276` | Delete macro covers only 11 fields; keyframes on `text_content`, `font_size`, `shape_type`, `points`, filters are left behind. |
| 13 | **Create new scene swallows error** | `app/mod.rs:744` | `std::fs::write(...).ok()` silently fails; proceeds with nonexistent file. Report error to user. |
| 14 | **Settings not persisted across restarts** | `settings.rs`, `persistence.rs:88-94` | Nudge steps, snap FPS, debounce, grid size are lost on restart. Only tree/window geometry survives. |
| 15 | **Multi-select card shows no editable controls** | `inspector/mod.rs:536-549` | Claims "edits apply to all selected actors" but renders no editable fields. Render controls or update copy. |
| 16 | **Local palette constants violate design tokens** | `app/mod.rs:585`, `graph_editor.rs:95,100`, `insertion_palette.rs:122`, `timeline_panel.rs:1131,1352` | `design_tokens.rs` forbids local palette constants. Replace with token lookups. |
| 17 | **Error detection by substring matching** | `app/mod.rs:593` | `contains("error")` fragile. Carry severity alongside status string. |
| 18 | **HiDPI hit-target double-scaling** | `drag_handler.rs:26,119` | Multiplying hit radii by `pixels_per_point` doubles on HiDPI since egui pointer is already in logical points. |

## P3 — Polish & UX Papercuts

| # | Task | File(s) | Details |
|---|------|---------|---------|
| 19 | **Command palette lacks keyboard navigation** | `command_palette.rs:80-105` | No arrow-key nav or Enter-to-execute (unlike insertion palette). Inconsistent UX. |
| 20 | **"Find Next" always jumps to first occurrence** | `find_replace.rs:131-143` | No cursor-relative search, no match highlight. Rename or implement iteration. |
| 21 | **Ruler tick step from total duration, not visible range** | `timeline_panel.rs:667` | Zoomed-in views of long timelines get extremely sparse ticks. |
| 22 | **New actor position hardcoded** | `spreadsheet.rs:107` | `[400.0, 300.0]` instead of scene center (like `sidebar.rs:617-620`). |
| 23 | **Unused design tokens** | `design_tokens.rs:196-197` | `WELCOME_ICON_SIZE`, `WELCOME_BTN_WIDTH` unused; welcome screen hardcodes sizes. Remove or use tokens. |
| 24 | **Empty `secondary_clicked()` block** | `timeline_panel.rs:1226-1233` | Dead code with only comments; real menu is `context_menu` below. |
| 25 | **Identical ternary branches** | `timeline_panel.rs:958` | `if prop_expanded { 16.0 } else { 16.0 }` — both branches identical. |
| 26 | **Misleading keyboard shortcut tooltip** | `toolbar.rs:307-321` | Advertises "⌘K / ?" but neither is registered; palette opens via Ctrl+Shift+P. |

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
