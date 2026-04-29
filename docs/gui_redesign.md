# Animatix GUI Redesign

## Vision: "Cinematic Workspace"

The preview canvas is the hero. Everything else recedes into supportive chrome. Dark, spacious, with sharp accent colors that echo the animation content.

## Design Principles

1. **Preview-dominant layout** — 55-60% of screen real estate
2. **Unified transport** — one transport bar, always visible at bottom
3. **Contextual panels** — explorer hidden by default, inspector inline below preview
4. **Atmospheric depth** — subtle gradients, layered transparencies
5. **Modern typography** — tight spacing, clean value formatting

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
│  │ 2  scene.bg = (0.07,...) │  │  │                               │  │
│  │ 3                        │  │  │       [render canvas]         │  │
│  │ 4  title: Text {         │  │  │                               │  │
│  │ 5    text: "Animatix",   │  │  │                               │  │
│  │ 6    font_size: 92       │  │  └───────────────────────────────┘  │
│  │ 7  }                     │  │                                     │
│  │ 8                        │  │  ┌─ INSPECTOR ──────────────────┐  │
│  │ 9  #1.5s                 │  │  │ ▸ orb          Circle        │  │
│  │10  orb.radius = 120      │  │  │ ▾ panel        Rect          │  │
│  │                          │  │  │   position  (300, 200)        │  │
│  │                          │  │  │   color     ■ #40ffa5         │  │
│  └──────────────────────────┘  │  └───────────────────────────────┘  │
│                                │                                     │
├────────────────────────────────┴─────────────────────────────────────┤
│  ▶  ⏮  ⏭   ━━━━━━━━●━━━━━━━━━━━━━━━━━━━━━━━━━━━   2.50s / 5.00s   │
│  1920×1080  •  4 actors  •  6 keyframes  •  ✓ Built                 │
└──────────────────────────────────────────────────────────────────────┘
```

### Key Changes
| Aspect | Current | Proposed |
|--------|---------|----------|
| Preview size | ~30% | **55-60%** |
| Explorer | Always visible (15%) | **Hidden by default**, toggle `⌘E` |
| Inspector | Separate dock tab | **Inline below preview** |
| Transport | Inside preview pane | **Dedicated bottom bar** |
| Toolbar | Full toolbar | **Minimal title bar** |
| Status bar | Separate panel | **Merged into transport** |

---

## Color Palette

```rust
// Backgrounds (darkest to lightest)
BG_BASE     = rgb(12, 14, 18)   // Window background
BG_PANEL    = rgb(18, 20, 24)   // Panel fill
BG_SURFACE  = rgb(24, 27, 33)   // Elevated surfaces
BG_WIDGET   = rgb(32, 36, 44)   // Buttons, inputs
BG_HOVER    = rgb(42, 47, 57)   // Hover state
BG_ACTIVE   = rgb(55, 62, 75)   // Active/pressed

// Accents
ACCENT_PRIMARY = rgb(84, 110, 255)  // Blue (play, primary)
ACCENT_SUCCESS = rgb(80, 200, 140)  // Green (saved, built)
ACCENT_WARNING = rgb(255, 196, 92)  // Amber (keyframes)
ACCENT_ERROR   = rgb(255, 100, 100) // Red (errors)
ACCENT_ACTOR   = rgb(137, 200, 235) // Light blue (actor labels)

// Text
TEXT_PRIMARY   = rgb(228, 232, 243)
TEXT_SECONDARY = rgb(150, 158, 175)
TEXT_MUTED     = rgb(90, 96, 110)

// Borders
BORDER_SUBTLE = rgb(40, 44, 52)
BORDER_FOCUS  = rgb(84, 110, 255)
```

---

## Transport Bar

```
┌──────────────────────────────────────────────────────────────────────┐
│  ▶   ⏮   ⏭      ━━━━━━━━━━━●━━━━━━━━━━━━━━━━━━━━━━   2.50s / 5.00s │
│  1920×1080  •  4 actors  •  6 keyframes  •  ✓ Built  •  ⌘S to save  │
└──────────────────────────────────────────────────────────────────────┘
```

**Row 1**: Play/Pause, Prev/Next keyframe, full-width scrubber with keyframe markers, time display
**Row 2**: Resolution, actor count, keyframe count, build status, hints

---

## Inspector Panel

### Actor Tree
```
ACTORS                        4
──────────────────────────────────
▾ title        Text
▾ formula      Math
▾ logo         Svg
▾ stage        Row (Group)
  ▸ orb        Circle
  ▸ signal     Line
  ▾ panel      Rect              ← selected
```

### Property Groups
```
panel                          Rect
First seen: 0.00s

▸ Transform (3)
▸ Shape (2)
▾ Style (5)
    color        ■ #40ffa5       ← color swatch + hex
    opacity      1.00
    stroke_width 2.00
    stroke_color ■ #ffffff
    fill_opacity 1.00
▸ Content (0)
▸ Layout (0)

Keyframes
──────────────────────────────────
  Time   Property    Value
▸ 0.00s  position    (300, 200)  ← current time highlighted
  0.00s  size        (250, 130)
  0.00s  color       ■ #40ffa5
```

### Property Groups
| Group | Properties |
|-------|-----------|
| Transform | position, motion_offset, rotation, scale |
| Shape | shape_type, line_from, line_to, arc_angles, points |
| Style | color, opacity, stroke_width, stroke_color, stroke_progress, fill_opacity |
| Content | text_content, text_paths, vector_paths, image |
| Layout | size, layout_size, placement_mode, position_binding |

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

## Implementation Phases

### Phase 1: Transport Bar (highest impact, lowest risk)
- Extract transport from preview into dedicated bottom bar
- Merge status bar metadata into transport row 2

### Phase 2: Layout Restructure
- Preview-dominant layout (60%)
- Inspector inline below preview
- Explorer hidden by default

### Phase 3: Visual Polish
- Apply color palette
- Tighten spacing
- Format values cleanly (hex colors, vec2 as `(x, y)`)

### Phase 4: Inspector Redesign
- Property groups (Transform, Shape, Style, Content, Layout)
- Color swatches
- All 24 trackable properties
- Keyframe list with current-time highlighting

### Phase 5: Title Bar & Atmosphere
- Compact title bar
- Noise texture overlay
- Preview background gradient
