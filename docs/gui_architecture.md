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
Timeline → frame_cache → PreviewSurface.render() → egui texture → Preview pane
Timeline.tracks → Inspector (read properties at current time)
Preview click → hit_regions → selected_actor → Inspector highlights
Preview drag → PropertyEdit → handle_property_edit → source + timeline + frame_cache.invalidate
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
User edits widget or drags preview handle
  → PropertyEdit { actor, property, value }
    → GuiShell.handle_property_edit()
      1. Snapshot (undo)
      2. Update in-memory timeline tracks
      3. Invalidate frame cache (so preview re-renders)
      4. Source edit:
         a. source_index.find(actor, property) → ByteSpan
            - Assignments take precedence over declarations
            - "position" ↔ "at" aliasing handled
         b. If span found → surgical replace via apply_source_edit()
         c. If span missing → insert via insert_property_after_span()
            (appends property after actor's last known property)
      5. Rebuild source index (for next edit's spans)
      6. Schedule debounced full rebuild (re-parse source → new Timeline)
```

### Position-Binding-Aware Drag

When the user drags an actor in the preview, the property written depends on
the actor's `PositionBinding`:

| Binding | Drag Body Writes | Drag Handle Writes |
|---------|-----------------|-------------------|
| `Absolute` (`at: (x, y)`) | `at` | `size` + `at` |
| `SceneAnchor` (`anchor: scene.center`) | `offset` | `size` + `offset` |
| `ScenePercent` (`at: (50%, 30%)`) | `at` (as percent) | `size` + `at` |
| `LayoutManaged` (inside Row/Col/Grid) | **blocked** (NotAllowed cursor) | `size` only |

Layout-managed actors have their position computed by the parent container's
layout engine, so drag-to-reposition is not meaningful. Scale handles still
work (they edit `size` which affects layout spacing).

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

### Global

| Shortcut | Action |
|----------|--------|
| `Space` | Play/Pause (when editor not focused) |
| `←` / `→` | Scrub ±0.1s |
| `,` / `.` | Prev/Next keyframe |
| `⌘S` | Save |
| `⌘E` | Toggle Explorer |
| `⌘I` | Toggle Inspector |
| `Escape` | Deselect actor |

### Editor (when focused)

| Shortcut | Action |
|----------|--------|
| `Ctrl+Space` | Trigger completion |
| `Up/Down` | Navigate completion list |
| `Tab/Enter` | Confirm completion |
| `Esc` | Dismiss completion |
| `Ctrl+Click` | Go-to-definition |

Note: Global shortcuts (Space, arrows, comma/period) are disabled when the editor has focus to avoid conflicts with text input.

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
| Parse | Syntax error | Red squiggles in editor + diagnostic message |
| Semantic | Unknown action/label | Yellow squiggles in editor |
| Info | Unknown property for type | Blue squiggles in editor |
| Build | Timeline build failure | Amber banner, partial timeline |
| Render | GPU error | Red overlay on preview |

### Diagnostic Sources

1. **Tree-sitter** — syntax errors (ERROR/MISSING nodes)
2. **Chumsky** — parse errors (more detailed messages)
3. **Semantic checks** — unknown actions, undefined labels, unknown types/properties, duplicate labels

---

## Preview Architecture

`PreviewSurface` owns offscreen textures and bridges the core renderer into egui:
- Allocate/resize render textures
- Evaluate timeline at current time
- Render via `RendererCore` (vello)
- Copy to egui texture for display

---

## Editor

### Syntax Highlighting

- Tree-sitter based highlighting via `animatix-gui/src/highlighting.rs`
- 14 highlight groups: keyword, type, string, number, comment, operator, punctuation, variable, property, parameter, function
- Gruvbox-inspired dark/light themes
- Cached highlighting (invalidated on text change)

### Auto-complete

- `CompletionPopup` widget (`completion_popup.rs`)
- Triggered by Ctrl+Space, or auto-trigger on `:`, `.`, ` ` characters
- Context-aware completions from `animatix-analyzer`:
  - Keywords + snippets at top level
  - Types after `:` in declarations
  - Properties inside `{ }` blocks
  - Labels after action verbs
- Keyboard navigation: Up/Down to select, Tab/Enter to confirm, Esc to dismiss
- Color-coded icons per completion kind (K=keyword, T=type, P=property, etc.)

### Diagnostics

- Colored squiggles via LayoutJob background tints
- Errors: red tint, Warnings: yellow tint, Info: blue tint
- Sources: tree-sitter syntax errors, chumsky parse errors, semantic checks
- Semantic checks: unknown actions, undefined labels, unknown types/properties

### Hover

- Tooltip on mouse hover over identifiers
- Shows: type info, documentation, usage examples
- Works for: labels, types, actions, keywords, literals

### Go-to-definition

- Ctrl+Click on identifiers
- Jumps to label/component definition in same file
- (Future: cross-file navigation via LSP)

### Keyboard Shortcuts (Editor-specific)

| Shortcut | Action |
|----------|--------|
| `Ctrl+Space` | Trigger completion |
| `Up/Down` | Navigate completion list |
| `Tab/Enter` | Confirm completion |
| `Esc` | Dismiss completion |
| `Ctrl+Click` | Go-to-definition |

---

## File Structure

```
crates/
├── animatix/                    # Core library
│   └── src/
│       ├── ast.rs               # AST types
│       ├── parser.rs            # Chumsky parser
│       ├── diagnostics.rs       # Diagnostic types
│       ├── module.rs            # Module system
│       ├── source_index.rs      # Source location mapping
│       └── timeline/            # Timeline compilation
│
├── animatix-analyzer/           # Shared language intelligence
│   └── src/
│       ├── lib.rs               # Analyzer struct
│       ├── symbol_table.rs      # Symbol extraction from AST
│       ├── completer.rs         # Context-aware completions
│       └── diagnostics.rs       # Parse + semantic diagnostics
│
├── animatix-lsp/                # LSP server for external editors
│   └── src/
│       └── main.rs              # tower-lsp server
│
├── animatix-gui/                # Desktop GUI application
│   └── src/
│       ├── lib.rs
│       ├── main.rs
│       ├── document.rs          # Document session management
│       ├── editor.rs            # Code editor with analyzer integration
│       ├── completion_popup.rs  # Completion popup widget
│       ├── highlighting.rs      # Tree-sitter highlighting + diagnostic squiggles
│       ├── hot_reload.rs        # File watcher
│       ├── preview_surface.rs   # GPU render surface
│       ├── source_edit.rs       # Surgical source text editing
│       └── app/
│           ├── mod.rs
│           ├── runtime.rs       # eframe::App impl
│           ├── persistence.rs   # Workspace state persistence
│           ├── file_tree.rs     # File explorer
│           ├── transport_bar.rs # Playback controls
│           ├── inspector.rs     # Actor property inspector
│           ├── preview.rs       # Preview pane
│           └── workspace.rs     # Dock layout management
│
└── tree-sitter-animatix/        # Tree-sitter grammar
    ├── grammar.js               # Grammar definition
    ├── queries/highlights.scm   # Highlight queries
    └── src/parser.c             # Generated parser
```

---

## LSP Server (External Editors)

The `animatix-lsp` crate provides language intelligence to external editors via the Language Server Protocol.

### Capabilities

| Feature | LSP Method | Status |
|---------|------------|--------|
| Completions | `textDocument/completion` | ✅ |
| Hover | `textDocument/hover` | ✅ |
| Go-to-definition | `textDocument/definition` | ✅ |
| Document symbols | `textDocument/documentSymbol` | ✅ |
| Diagnostics | Published on change | ✅ |

### Usage

```bash
# Run directly (communicates via stdin/stdout)
animatix-lsp
```

### Editor Configuration

**VS Code** (`.vscode/settings.json`):
```json
{
  "amx.languageServer": {
    "command": "animatix-lsp",
    "args": []
  }
}
```

**Neovim** (nvim-lspconfig):
```lua
require('lspconfig').animatix.setup {
  cmd = { 'animatix-lsp' },
  filetypes = { 'amx' },
  root_dir = require('lspconfig').util.root_pattern('.git', 'Cargo.toml'),
}
```

### Architecture

The LSP server is a thin wrapper around `animatix-analyzer`:
- Each opened document gets an `Analyzer` instance
- LSP requests delegate to analyzer methods
- Type conversions happen inline (~50 lines)
- No separate conversion layer needed

See `docs/analyzer-design.md` for the full analyzer architecture.
