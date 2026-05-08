# Animatix Editor — Cell-Based Design Plan

## Overview

Replace the single `TextEdit` with a Jupyter-style cell editor. Each keyframe becomes a self-contained editable cell with its own header, body, and actions. Non-keyframe content lives in code cells.

---

## Data Model

```rust
/// A single editable cell in the notebook editor.
pub enum Cell {
    /// Config, import, component definition blocks (collapsible preamble).
    Code { body: String, expanded: bool },

    /// A keyframe declaration with its timestamp, body, and optional leading comment.
    Keyframe {
        /// Raw timestamp text: "0s", "+1.5s", "500ms"
        timestamp: String,
        /// True if this was `#+...` (relative), false for absolute `#...`.
        is_relative: bool,
        /// The editable body content (actor declarations, assignments, actions).
        body: String,
        /// Leading comment lines attached to this keyframe (e.g. `// setup scene`).
        attached_comment: Option<String>,
    },
}
```

### Cell ↔ Source Translation

- **Source → Cells**: Scan line-by-line. When `#timestamp {` hit, emit `Keyframe` with everything inside braces as body. Comments immediately preceding a keyframe are attached to that keyframe cell (displayed as a collapsible note). Config/import/component blocks become `Code` preamble cells. Everything between keyframes becomes a `Code` cell (or is merged into the preceding keyframe if it's just trailing comments).
- **Cells → Source**: Concatenate all cells. `Keyframe` cells emit `#timestamp {\n{body}\n}\n`. `Code` cells emit `{body}\n`. Comments attached to keyframes are written before the `#timestamp` line.

---

## Rendering Layout

```
ScrollArea::vertical()
└── Vertical stack (top_down)
    ├── Cell 0: Code
    │   └── TextEdit::multiline(&mut body)
    ├── Divider (thin line, droppable zone for insert)
    ├── Cell 1: Keyframe
    │   ├── Header bar
    │   │   ├── [▶] play button → scrubs timeline to this time
    │   │   ├── ⏱ 0s          (small, amber text)
    │   │   └── [⋯] menu      (delete, duplicate, convert to relative/absolute)
    │   ├── Body
    │   │   └── TextEdit::multiline(&mut body)
    │   └── (tinted background, 2px amber left border)
    ├── Divider
    ├── Cell 2: Code
    │   └── TextEdit::multiline(&mut body)
    └── ...
```

### Header Styling

- Height: 24px
- Background: `Color32::from_rgb(28, 31, 38)` (slightly lighter than editor bg)
- Timestamp label: 11px monospace, `Color32::from_rgb(255, 196, 92)`
- Buttons: small (16×16), muted unless hovered

### Body Styling

- `TextEdit::multiline` with `code_editor()` styling
- Keyframe cells: full-cell background tint (alternating or single subtle shade)
- 2px amber left border on keyframe cells
- No wrap (`desired_width(f32::INFINITY)`)
- `frame(false)` — the cell container provides the frame

### Divider

- Thin horizontal line (1px, `Color32::from_rgb(40, 44, 52)`)
- Hover: line brightens, shows a faint "+" in center for quick insert

---

## Interaction Model

| Action | Behavior |
|---|---|
| Type in body | Normal `TextEdit` behavior. On change → rebuild source → debounced `DocumentSession::rebuild()` |
| Click timestamp | Timeline scrubs to this keyframe's time |
| ▶ in header | Same as click timestamp |
| Click divider "+" | Inserts new `Keyframe` cell with midpoint-relative timestamp |
| Menu → Delete | Removes cell, rebuilds source |
| Menu → Duplicate | Clones cell with same timestamp, appends after |
| Menu → Toggle absolute/relative | Rewrites `#0s` ↔ `#+0s` depending on context |
| Drag cell handle | **Disabled.** Cells are source-ordered only. |

---

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Shift+Enter` inside body | Insert new keyframe cell below |
| `Ctrl+Shift+K` | Convert current cell's timestamp type (abs ↔ rel) |
| `Ctrl+Shift+D` | Duplicate current cell |
| `Ctrl+Shift+Backspace` | Delete current cell (if empty or confirmed) |
| `Ctrl+Shift+↑/↓` | **Disabled.** No reordering. |

---

## Synchronization with DocumentSession

```
User types in Cell body
    ↓
Cells → source_text (concatenate)
    ↓
DocumentSession::set_source_text(source_text)
    ↓
pending_rebuild_at = now + 150ms debounce
    ↓
Rebuild → new keyframe_lines, timeline_index
    ↓
Cells may need re-split if structure changed (e.g. user typed `#new` in a body)
```

### Bidirectional sync edge case

If user types `#newtimestamp` inside a cell body, we need to **re-split cells** on next source sync. The cell editor detects this and re-parses source into a new `Vec<Cell>`.

---

## File Changes

### New files

| File | Purpose |
|---|---|
| `crates/animatix-gui/src/cell_editor/mod.rs` | Cell data model, split/merge, rendering |
| `crates/animatix-gui/src/cell_editor/cell.rs` | `Cell` enum and operations |
| `crates/animatix-gui/src/cell_editor/parser.rs` | Source text → `Vec<Cell>` |
| `crates/animatix-gui/src/cell_editor/render.rs` | Cell UI rendering (headers, bodies, dividers) |

### Modified files

| File | Changes |
|---|---|
| `crates/animatix-gui/src/editor.rs` | Replace `EditorBuffer` internals with cell editor; keep same public API for `app.rs` |
| `crates/animatix-gui/src/app/workspace.rs` | `editor_ui` becomes simpler — just calls `self.editor.show(ui)` |
| `crates/animatix-gui/src/app.rs` | Remove `set_keyframe_lines`, `set_keyframe_times_s` calls; editor handles internally |
| `crates/animatix-gui/src/highlighting.rs` | Simplify — no more `keyframe_lines`, `highlighted_line` deco; just pure syntax highlighting per cell body |

### Deleted code

- `keyframe_section_bands`, `keyframe_tag_range` in `highlighting.rs`
- Gutter/tick overlay code in `workspace.rs`
- `keyframe_times_s` field in `editor.rs`

---

## Decisions (User Confirmed)

| # | Question | Decision |
|---|---|---|
| Q1 | Drag reorder / timestamp rewrite | **No drag reorder.** Cells always stay in source order. Timestamps are not rewritten on reorder because reorder is disallowed. No negative relative timestamps. |
| Q2 | Non-keyframe cell visibility | **Collapsed by default, expandable.** Comments attach to the nearest preceding keyframe cell (not standalone). Config/import cells are collapsible preamble blocks. |
| Q3 | Insert default timestamp | **Midpoint.** When inserting between two keyframes at T₁ and T₂, default to `#+(T₂−T₁)/2`. If after last keyframe, `#+1s`. |
| Q4 | Brace style | **Standardize to braces.** All keyframes serialize as `#time {\n  body\n}`. Update grammar to require braces. Legacy brace-less source is accepted on parse but normalized on save. |
| Q5 | Undo granularity | **Document-wide.** One undo = revert entire source text snapshot. Keeps implementation simple; matches current behavior. |

---

## Risk Assessment

| Risk | Likelihood | Mitigation |
|---|---|---|
| Scroll jump when re-splitting cells | Medium | Only re-split on rebuild, not on every keystroke |
| Source ↔ Cell drift (user types `#` in body) | Low | Re-parse source after rebuild; warn if structure changes |
| Performance with many cells (>50 keyframes) | Low | Virtual scrolling if needed; unlikely in practice |
| Grammar breakage (braces now required) | Medium | Graceful parse of legacy brace-less; normalize on save |

---

## Implementation Order

1. **Cell model + parser** (source ↔ cells, no UI)
2. **Cell rendering** (headers + TextEdit bodies in vertical stack)
3. **Source sync** (cells → source → rebuild → re-split loop)
4. **Cell actions** (insert, delete, duplicate, play)
5. **Keyboard shortcuts**
6. **Remove old overlay code**
7. **Grammar update** (require braces for keyframes)

---

## Appendix: Why This Beats Overlay Approach

| Problem | Overlay Fix | Cell Fix |
|---|---|---|
| Background only covers text | Painter rectangles that must track scroll offset | `Frame::new().fill()` — native widget background |
| Timestamp smaller font | Custom `TextFormat` with smaller `font_id` in `LayoutJob` | Separate `ui.label()` with `.size(11.0)` |
| Tick marks out of sync | Complex scroll offset tracking | Tick marks become cell header widgets |
| Paragraph-level tinting | Painter rectangle per line, O(n) per frame | One `Frame` per cell |
| Left accent border | Painter overlay that clips | Cell `Frame::new().stroke()` |
| Keyframe actions (play, delete) | Right-click context menu | Native buttons in cell header |
| Insert new keyframe | Manual text insertion | "+" divider button |

*Verdict: Cells turn every hard problem into a straightforward widget composition.*
