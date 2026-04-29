# Animatix GUI Architecture & Design

## Overview

`animatix-gui` is a desktop application crate that wraps the Animatix runtime with an egui-based shell. The GUI is editor-first: the source of truth is `.amx` text, while the app provides live preview, timeline control, and actor inspection.

Built on: `eframe`, `egui`, `egui_dock`, `wgpu`, `vello`.

---

## Layout

```
┌──────────────────────────────────────────────────────────────────────┐
│ ● ● ●   animatix                              showcase.amx  ● 2:45  │
├────────────────────────────────┬─────────────────────────────────────┤
│                                │                                     │
│         EDITOR                 │            PREVIEW                  │
│         (40%)                  │             (60%)                   │
│                                │                                     │
│  ┌──────────────────────────┐  │  ┌───────────────────────────────┐  │
│  │ 1  #0s                   │  │  │                               │  │
│  │ 2  scene.bg = (0.07,...) │  │  │       [render canvas]         │  │
│  │ 3                        │  │  │                               │  │
│  │ 4  title: Text {         │  │  └───────────────────────────────┘  │
│  │ 5    text: "Animatix",   │  │                                     │
│  │ 6    font_size: 92       │  │  ┌─ INSPECTOR ──────────────────┐  │
│  │ 7  }                     │  │  │ ▸ orb          Circle        │  │
│  │ 8                        │  │  │ ▾ panel        Rect          │  │
│  │ 9  #1.5s                 │  │  │   position  (300, 200)        │  │
│  │10  orb.radius = 120      │  │  │   color     ■ #40ffa5         │  │
│  │                          │  │  └───────────────────────────────┘  │
│  └──────────────────────────┘  │                                     │
├────────────────────────────────┴─────────────────────────────────────┤
│  ▶  ⏮  ⏭   ━━━━━━━━●━━━━━━━━━━━━━━━━━━━━━━━━━━━   2.50s / 5.00s   │
│  1920×1080  •  4 actors  •  6 keyframes  •  ✓ Built                 │
└──────────────────────────────────────────────────────────────────────┘
```

| Aspect | Value |
|--------|-------|
| Preview | 55-60% of width |
| Explorer | Hidden by default, toggle `⌘E` |
| Inspector | Inline below preview |
| Transport | Dedicated bottom bar |
| Toolbar | Minimal title bar |
| Status | Merged into transport row 2 |

---

## Color Palette

```rust
// Backgrounds
BG_BASE     = rgb(12, 14, 18)
BG_PANEL    = rgb(18, 20, 24)
BG_SURFACE  = rgb(24, 27, 33)
BG_WIDGET   = rgb(32, 36, 44)
BG_HOVER    = rgb(42, 47, 57)
BG_ACTIVE   = rgb(55, 62, 75)

// Accents
ACCENT_PRIMARY = rgb(84, 110, 255)
ACCENT_SUCCESS = rgb(80, 200, 140)
ACCENT_WARNING = rgb(255, 196, 92)
ACCENT_ERROR   = rgb(255, 100, 100)
ACCENT_ACTOR   = rgb(137, 200, 235)

// Text
TEXT_PRIMARY   = rgb(228, 232, 243)
TEXT_SECONDARY = rgb(150, 158, 175)
TEXT_MUTED     = rgb(90, 96, 110)

// Borders
BORDER_SUBTLE = rgb(40, 44, 52)
BORDER_FOCUS  = rgb(84, 110, 255)
```

---

## State Model

### DocumentSession
Owns: file path, source text, dirty state, compiled AST, timeline, duration, scene dimensions.

### PreviewPaneState
Owns: current time, playback state, dimensions, status/error.

### GuiShell
Owns: document, editor, preview, dock state, hot reloader, selected actor, hit regions.

### Data Flow
```
Editor → DocumentSession.set_source_text() → debounce → rebuild → Timeline
Timeline → PreviewSurface.render() → egui texture → Preview pane
Timeline.tracks → Inspector (read properties at current time)
Preview click → hit_regions → selected_actor → Inspector highlights
```

---

## Transport Bar

```
┌──────────────────────────────────────────────────────────────────────┐
│  ▶   ⏮   ⏭      ━━━━━━━━━━━●━━━━━━━━━━━━━━━━━━━━━━   2.50s / 5.00s │
│  1920×1080  •  4 actors  •  6 keyframes  •  ✓ Built                 │
└──────────────────────────────────────────────────────────────────────┘
```

- Row 1: Play/Pause, Prev/Next keyframe, scrubber with amber keyframe markers, time
- Row 2: Resolution, actor count, keyframe count, build status

---

## Inspector

### Actor Tree
```
ACTORS                        4
──────────────────────────────────
▾ title        Text
▾ stage        Row (Group)
  ▸ orb        Circle
  ▾ panel      Rect              ← selected
```

### Property Groups
| Group | Properties |
|-------|-----------|
| Transform | position, motion_offset, rotation, scale |
| Shape | shape_type, line_from, line_to, arc_angles, points |
| Style | color, opacity, stroke_width, stroke_color, stroke_progress, fill_opacity |
| Content | text_content, text_paths, vector_paths, image |
| Layout | size, layout_size, placement_mode, position_binding |

### Value Formatting
- Colors: `■ #40ffa5` (swatch + hex)
- Vec2: `(300, 200)` not `[300.0, 200.0]`
- Rotation: `45.0°`
- Percentages: `75%`

---

## Input Widgets

### Widget Types

| Widget | For Properties | Behavior |
|--------|---------------|----------|
| `Vec2Input` | position, size, line_from, line_to, arc_angles, points | Two linked number fields `(x, y)` with drag-to-adjust |
| `ColorInput` | color, stroke_color | Color swatch + hex field + optional popup picker |
| `FloatInput` | opacity, scale, rotation, stroke_width, stroke_progress, fill_opacity | Number field with drag-to-adjust, optional range |
| `SliderInput` | opacity, fill_opacity, stroke_progress | 0..1 slider with value label |
| `TextInput` | text_content | Single-line text field |
| `ShapeTypeSelector` | shape_type | Dropdown: Rect, Circle, Line, Arc, Polygon, ... |
| `EasingSelector` | (keyframe easing) | Dropdown: Linear, EaseIn, EaseOut, EaseInOut, Bounce, ... |

### Widget Layout

```
┌─────────────────────────────────────────────┐
│ position                    (300, 200)       │
│ ┌───────────┬───────────┐                   │
│ │  300      │  200      │  ← Vec2Input      │
│ └───────────┴───────────┘                   │
├─────────────────────────────────────────────┤
│ color                       ■ #40ffa5        │
│ ┌────┐┌──────────────────┐                  │
│ │████││  #40ffa5          │  ← ColorInput    │
│ └────┘└──────────────────┘                  │
├─────────────────────────────────────────────┤
│ opacity                     ━━━━━━━━● 1.00  │
│ ┌──────────────────────────────────┐        │
│ │  ████████████████████████  1.00  │← Slider │
│ └──────────────────────────────────┘        │
├─────────────────────────────────────────────┤
│ rotation                    45.0°            │
│ ┌──────────────────┐                        │
│ │  45.0            │  ← FloatInput (degrees)│
│ └──────────────────┘                        │
└─────────────────────────────────────────────┘
```

### Edit Flow

```
User edits widget → UiActions.property_edit { actor, property, value }
  → GuiShell.handle_actions()
    → DocumentSession.update_source_text(actor, property, value)
      → Generates .amx source change
      → Triggers rebuild
        → Timeline updated
          → Preview refreshes
```

### Keyframe Awareness

Each widget shows:
- **Animated indicator**: amber dot if property has keyframes
- **Current value**: evaluated at scrub time
- **Keyframe button**: ◆ to add/remove keyframe at current time
- **Interpolation badge**: shows easing between keyframes

```
┌─────────────────────────────────────────────┐
│ position           ◆ animated  (300, 200)   │
│ ┌───────────┬───────────┐  [◆ add keyframe] │
│ │  300      │  200      │                   │
│ └───────────┴───────────┘                   │
│ 0.00s → 2.00s  ease: ease-in-out           │
└─────────────────────────────────────────────┘
```

---

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Space` | Play/Pause |
| `←` / `→` | Scrub ±0.1s |
| `,` / `.` | Prev/Next keyframe |
| `⌘S` | Save |
| `⌘E` | Toggle Explorer |
| `⌘I` | Toggle Inspector |
| `Escape` | Deselect actor |

---

## Hot Reload

The GUI watches the loaded .amx file for changes. On modification:
1. Detect via OS file watcher
2. Debounce (300ms)
3. Reload from disk
4. Rebuild timeline
5. Update editor buffer

---

## Error Model

| Class | Example | UI Treatment |
|-------|---------|--------------|
| File load | Missing file | Red banner in preview |
| Parse | Syntax error | Red gutter markers in editor |
| Build | Unknown action | Amber banner, partial timeline |
| Render | GPU error | Red overlay on preview |

---

## Preview Architecture

`PreviewSurface` owns offscreen textures and bridges the core renderer into egui:
- Allocate/resize render textures
- Evaluate timeline at current time
- Render via `RendererCore` (vello)
- Copy to egui texture for display

---

## Editor

- `egui::TextEdit` with Syntect-backed syntax highlighting
- Local `animatix.sublime-syntax` grammar
- Line numbers in gutter
- Keyframe markers (amber dots) in gutter
- Error markers (red dots) from diagnostics

---

## File Structure

```
crates/animatix-gui/src/
├── lib.rs
├── main.rs
├── document.rs
├── editor.rs
├── hot_reload.rs
├── preview_surface.rs
└── app/
    ├── mod.rs
    ├── runtime.rs
    ├── persistence.rs
    ├── file_tree.rs
    ├── transport_bar.rs
    ├── inspector.rs
    ├── preview.rs
    └── workspace.rs
```
