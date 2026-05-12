# GUI Actor Creation

## Overview

Users can now create actors directly from the GUI without switching to the code editor. This bridges the gap between the visual editing workflow (select, drag, keyframe) and the code-first creation model.

## Philosophy

- **GUI creates minimal, valid actors** — users refine them in the inspector or switch to code
- **Code remains canonical** — complex actors (plots, media, math) stay code-only
- **Narrow scope** — Phase 1 supports: Rect, Circle, Text, Row, Col

## Entry Points

### 1. Toolbar "+" Button
- Click `+` in the top toolbar → compact dropdown palette
- Primary path for power users

### 2. Empty-State CTAs
- Inspector "No actors in scene" → prominent "Add Actor" button
- Layers panel "No actors in scene" → same CTA

### 3. Right-Click Canvas
- Right-click on preview → "Add Actor" → cascading submenu
- Actor placed at **click position** instead of center

## Actor Palette

```
┌─────────────────────────────┐
│  Shapes          ▓▓▓▓▓▓▓▓▓▓ │
│    □ Rect                   │
│    ○ Circle                 │
│  Text                       │
│    T Text                   │
│  Containers                 │
│    ≡ Row                    │
│    ▌ Col                    │
└─────────────────────────────┘
```

## Defaults

| Type | Position | Extra Properties |
|------|----------|-----------------|
| Rect | Scene center | `size: (120, 80)` |
| Circle | Scene center | `size: (80, 80)` |
| Text | Scene center | `text: "Text"`, `font_size: 24` |
| Row/Col | Scene center | `gap: 8` |

All actors get `color: accent.primary`.

## Label Generation

Auto-generated unique snake_case labels:
```
rect1, rect2, circle1, text1, row1, col1, ...
```

## Source Insertion

New actors are inserted into the **current keyframe block** (at playhead time). If no keyframes exist, top-level declarations are wrapped in `#0s` first.

After insertion:
- New actor is **auto-selected**
- Undo snapshot is taken
- Rebuild is scheduled

## Container Awareness

If a container (Row/Col/Grid/Stack/Group) is selected:
- Palette shows `[ Top-level | Inside selected ]` toggle
- Default: Top-level
- "Inside" inserts as a child of the container

## Keyboard Shortcuts (Phase 2)

Not implemented in Phase 1. Future: hold `R`/`C`/`T` → crosshair cursor → click to place.

## Implementation

### New `SourceEdit` variant

```rust
pub enum SourceEdit {
    // ... existing ...
    InsertActor {
        ty: String,               // "Rect", "Text", etc.
        label: String,            // "rect1"
        props: Vec<Property>,     // pre-built defaults
        container: Option<String>, // None = top-level
    },
}
```

### Files Modified

| File | Change |
|------|--------|
| `source_edit.rs` | `InsertActor` variant + `insert_actor()` logic |
| `app/shell/toolbar.rs` | `+` button + palette dropdown |
| `app/panels/inspector/mod.rs` | CTA button in empty state |
| `app/panels/mod.rs` | CTA in layers empty state |
| `app/preview/selection.rs` | Right-click canvas menu extension |
| `app/mod.rs` | Wire new `UiActions` through `handle_actions` |
| `app/actions/mod.rs` | Handle actor creation in `handle_actions` |

## Future Work

- Phase 2: Line, Arrow, Ellipse, Group
- Phase 2: Keyboard shortcut quick-create
- Phase 2: Click-and-drag to define size on creation
- Not planned: Math, Code, Image, Svg, plots (code-only)
