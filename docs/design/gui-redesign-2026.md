# Animatix GUI Redesign — "Timeline-First Direct Manipulation"

> **Status:** Design complete. Implementation not started.  
> **Scope:** GUI chrome reduction, unified editing surface, multi-scene integration, future PiP readiness.  
> **Date:** 2026-05-27

---

## 1. Why Redesign?

The current GUI has **four persistent horizontal bars** (window toolbar, NL command bar, preview header, transport bar) consuming ~112px of vertical space plus an always-visible preview header. The timeline shows keyframes but they are not draggable. Property editing requires looking away from the canvas at the inspector panel. Keyframe creation uses a global on/off toggle that is easy to misuse. Multi-scene composition exists in the backend but the GUI treats it as a separate sidebar panel rather than the primary editing surface.

**The core problem:** The GUI is a viewer, not an editor. Users cannot manipulate animation visually.

---

## 2. Design Principles

1. **One persistent bar.** All other chrome is contextual or hover-revealed.
2. **The timeline is the primary authoring surface.** Scenes, transitions, keyframes, and actions live here.
3. **Direct manipulation on canvas.** Drag actors to move/scale/rotate. Double-click for property popup.
4. **Per-property keyframe control.** No global modes. Every property shows its own keyframe state.
5. **Context is always explicit.** The user always knows whether they are in global or scene-local editing mode.
6. **Scenes, viewports, and sequences are separate.** This enables future PiP without redesign.

---

## 3. The New Layout

### 3.1 Single-Scene File

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ ● Demo.amx* ▾                                    ⏵  ⚙  ⌘K                │  ← Document Chrome (28px)
├─────────────────────────────────────────────────────────────────────────────┤
│  EDITOR (50%)  │  PREVIEW (50%)                                              │
│                │  ┌──────────────────────────────────────────────────────┐  │
│                │  │                    [CANVAS]                          │  │
│                │  │         ┌─────────────────────────────┐              │  │
│                │  │         │ t: 2.34s / 5.00s            │              │  │  ← Context HUD
│                │  │         │                             │              │  │
│                │  │         │  ╔═══════════════════════╗  │              │  │  ← Gizmo on
│                │  │         │  ║  ←drag  ↑scale  ◠rot  ║  │              │  │    selected actor
│                │  │         │  ║     [selected actor]  ║  │              │  │
│                │  │         │  ╚═══════════════════════╝  │              │  │
│                │  │         └─────────────────────────────┘              │  │
│                │  │                                                      │  │
│                │  │  ┌─────────────────────────────────────────────────┐ │  │
│                │  │  │ ⏵ │ 2.34s │ Grid Guides Labels │ 100% │        │ │  │  ← Hover HUD
│                │  │  └─────────────────────────────────────────────────┘ │  │    (fades out)
│                │  └──────────────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────────────────────┤
│  TIMELINE — unified editing surface                                          │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │ ◀◀ ◀ ⏵ ▶ ▶▶  [1×▼]  00:02.34 / 00:05.00  [⟲]                        │ │  ← Playback strip
│  ├────────────────────────────────────────────────────────────────────────┤ │
│  │ Ruler: 0s      1s      2s      3s      4s      5s                     │ │  ← Ruler / scrubber
│  ├────────────────────────────────────────────────────────────────────────┤ │
│  │ ▼ rect1  │      ◆◆              ◆◆                                    │ │  ← Actor tracks
│  │   Position│      ◆               ◆                                    │ │     (expandable)
│  │   Size    │  ◆                   ◆                                    │ │
│  │   Opacity │              ◆◆                                           │ │
│  ├────────────────────────────────────────────────────────────────────────┤ │
│  │ ▼ circle │  ◆         ◆         ◆                                     │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Multi-Scene Composition

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ ● Presentation.amx* ▾  Intro → Diagram → Outro      ⏵  ⚙  ⌘K             │  ← Breadcrumb
├─────────────────────────────────────────────────────────────────────────────┤
│  EDITOR (40%)  │  PREVIEW (60%)                                              │
│                │  ┌──────────────────────────────────────────────────────┐  │
│                │  │                    [CANVAS]                          │  │
│                │  │         ┌─────────────────────────────┐              │  │
│                │  │         │ Editing: "Diagram"          │              │  │  ← Context HUD
│                │  │         │ Local: 2.34s / 5.00s        │              │  │     shows scene
│                │  │         │ Global: 12.34s / 25.00s     │              │  │     + time modes
│                │  │         └─────────────────────────────┘              │  │
│                │  │                                                      │  │
│                │  │         ╔═══════════════════════╗                    │  │  ← Gizmo on
│                │  │         ║  ←drag  ↑scale  ◠rot  ║                    │  │    selected actor
│                │  │         ║     [selected actor]  ║                    │  │
│                │  │         ╚═══════════════════════╝                    │  │
│                │  └──────────────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────────────────────┤
│  TIMELINE — unified editing surface                                          │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │ ◀◀ ◀ ⏵ ▶ ▶▶  [1×▼]  GLOBAL  00:12.34 / 00:25.00  [⟲]                 │ │
│  ├────────────────────────────────────────────────────────────────────────┤ │
│  │ SCENES: [══════Intro══════][▓▓fade▓▓][══════Diagram══════][▓▓cut▓▓][════Outro════] │  ← Scene row
│  │         0s              5s        5.3s                 15s        15s    25s      │
│  ├────────────────────────────────────────────────────────────────────────┤ │
│  │ ▼ Diagram (editing) │ ◆◆        ◆◆                                   │ │  ← Actor tracks
│  │   Position          │ ◆         ◆                                    │ │     for active scene
│  │   Size              │  ◆        ◆                                    │ │
│  │   Opacity           │     ◆◆                                          │ │
│  │   [+ Action] [Fade In] [Move] [Rotate] [Fade Out]                     │ │  ← Action buttons
│  └────────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Component Details

### 4.1 Document Chrome (Top Bar)

**Height:** 28px (only persistent chrome).

**Contents left to right:**
- App dot + filename with dirty indicator (`*`)
- Filename dropdown: recent files, save, export
- **Breadcrumb** (multi-scene only): `Intro → Diagram → Outro` — click to edit that scene
- Playback toggle (`⏵`/`⏸`)
- Settings gear
- Command palette trigger (`⌘K`)

**Gone from top bar:** Export button, Inspector button, Rebuild button, Add Actor button, Keyframe mode toggle, NL Command Bar.

### 4.2 Preview Canvas

**No header bar.** Maximum canvas space.

**Context HUD** (bottom-left, fades during playback):
- Scene name (click to jump to scene start)
- Local time / duration
- Global time / duration
- "Global" / "Local" toggle (click to switch time view)

**Hover HUD** (appears after 1s hover, fades on mouse leave):
- Play/Pause
- Current time
- Overlay toggles: Grid, Guides, Labels (individual buttons, not menu)
- Zoom: Fit, 100%, 150%, 200%

**Unified Transform Gizmo** on selected actor:
- Drag body → Move (measurement line: `Δx: +12  Δy: -8`)
- Drag corner → Scale uniform (measurement: `w: 40 → 64`)
- Drag edge → Scale axis-locked
- Hover near corner → Rotation ring (drag to rotate, arc shows `45° → 67°`)

**Tool modes** (keyboard shortcuts):
| Key | Tool | Cursor |
|-----|------|--------|
| `V` | Select / Viewport | Arrow |
| `M` / `G` | Move | Move cross |
| `S` | Scale | Resize diagonal |
| `R` | Rotate | Rotate circular |
| `E` | Vertex edit | Pen |

### 4.3 Property Popup (Double-Click Actor or Press Enter)

**Attached to actor's top edge.** Follows actor. Clamped to viewport.

**Essentials tab** (always visible):
```
┌───────────────────────────────────────────────────────────┐
│ Circle_1                    [✕] [⬓]                      │
├───────────────────────────────────────────────────────────┤
│ ◆ Pos    x: 120    y: 80    │ ◆ Size  w: 40  h: 40     │
│ ◆ Rot    45°                │ ◆ Opacity  100%          │
│ ◆ Color  ████████           │                           │
├───────────────────────────────────────────────────────────┤
│ [Transform] [Style] [Shape] [Text]                       │  ← Tabs
└───────────────────────────────────────────────────────────┘
```

**Interactions:**
- **Diamond (◆/○)** per property: click to toggle keyframe at current time
  - Filled (◆) = keyframe exists at playhead
  - Hollow (○) = no keyframe at playhead
  - Hover tooltip: "Keyframe at 2.34s: value=(120, 80), ease=Ease Out" or "Click to add keyframe"
- **Drag values** left/right to change (like Figma)
- **Tab** to cycle properties
- **K** to toggle keyframe on focused property
- **Enter** to edit focused property
- **Esc** to close popup
- **Auto-hide during drag** on canvas (so it doesn't block)

**Additional tabs:**
- **Transform:** Position, Rotation, Scale, Anchor, Offset
- **Style:** Color, Opacity, Fill Opacity, Stroke, Stroke Color, Stroke Width, Stroke Progress, Glow, Shadow, Backdrop Blur
- **Shape:** Shape-specific (radius, points, arc angles, etc.)
- **Text:** Text content, Font family, Font size

### 4.4 Timeline Panel

**The primary authoring surface.**

**Playback strip** (top):
- Go to start (`|◀`), previous frame (`◀`), Play/Pause (`⏵`), next frame (`▶`), go to end (`▶|`)
- Speed dropdown (`[1× ▼]`) with presets + slider
- Time display: `GLOBAL 00:02.34 / 00:05.00` (MM:SS.FF format)
- Loop toggle (`⟲`)
- Diagnostics status (`✓` / `⚠ 3` / `✕ 1` — click to open panel)

**Ruler** (scrubber):
- Click/drag anywhere on ruler to scrub
- Tick marks at sensible intervals
- Scene blocks shown above ruler (for multi-scene)

**Scene row** (multi-scene only):
- Scene blocks as colored horizontal bars
- Click → enter scene-local editing mode
- Double-click → rename
- Drag → reorder / change start time
- Drag edge → change duration (shifts subsequent scenes)
- Right-click gap → "Add transition"
- Transition regions shown as striped overlap between scenes
- Click transition → inline editor (type, duration, easing)

**Actor tracks:**
- Actor name row (collapsible arrow `▼`)
- Expanded: property sub-tracks (Position, Size, Opacity, etc.)
- **Keyframe diamonds** are **draggable** left/right to change timing
  - Hold `Shift` to snap to other keyframes, ruler marks, 0.1s increments
  - Hold `Alt` to duplicate
  - Visual feedback: diamond lifts, vertical guide follows cursor, tooltip: `2.0s → 3.2s`
- **Multi-select:** `Shift+click`, box-select by dragging empty area
- **Action blocks:** colored horizontal bars on actor tracks
  - Entrance = green, Motion = blue, Exit = red, Effect = amber
  - Drag left/right edge → change duration
  - Drag body → change start time
  - Click → edit easing/intensity
  - Right-click → delete / change type

**Action bar** (bottom of timeline when actor selected):
- `[+ Action]` → open action palette
- Quick buttons: Fade In, Move, Rotate, Scale, Fade Out

### 4.5 Context Model

**Global mode** (default):
- Timeline shows all scenes + all actor tracks
- Scrubber moves through global time
- Canvas shows currently active scene (based on global time)
- Used for: reviewing composition, adjusting scene timing

**Scene-local mode** (click scene block):
- Timeline shows only that scene's actors + tracks
- Scrubber moves through scene-local time (0 to scene duration)
- Canvas shows that scene's content regardless of global time
- Property popup edits create keyframes in scene-local time
- Used for: animating within a specific scene

**Viewport mode** (future PiP):
- Timeline shows viewport tracks
- Canvas shows multiple viewport rectangles
- Select viewport border → edit viewport properties
- Double-click viewport → enter scene editing mode inside

---

## 5. Workflows

### 5.1 Create a Keyframe

**Goal:** Make `Circle_1`'s position change over time.

1. Click `Circle_1` on canvas (selected, gizmo appears)
2. Scrub timeline to `0s`
3. Property popup shows `○ Pos x:0 y:0` (hollow = no keyframe)
4. Either:
   - Click the `○` diamond → fills to `◆` (keyframe created with current value)
   - OR: Drag `Circle_1` to desired position → `○` auto-fills to `◆`, toast: "Auto-keyframed Position at 0s [Undo]"
5. Scrub to `2s`
6. Drag `Circle_1` to new position → `○` auto-fills (new track = auto-keyframe)
7. Press `Space` to play

**No global mode toggle. Per-property control. Safe auto-keyframing with undo.**

### 5.2 Animate an Actor to Enter/Leave

**Goal:** Make `Circle_1` fade in over 1 second.

1. Select `Circle_1`
2. Scrub to desired start time (e.g., `1s`)
3. Press `A` or right-click canvas → Actions palette
4. Select "Fade In" → action block appears in timeline at `1s` with `1s` duration
5. Drag right edge of block to adjust duration
6. Click block → edit easing (dropdown)
7. Play to preview

**Behind the scenes:** GUI generates AMX code: `fade-in Circle_1 [1s, ease: ease-out]`

### 5.3 Adjust Keyframe Timing

**Goal:** Move a position keyframe from `2s` to `3s`.

1. Find keyframe diamond on Position track
2. Click and drag diamond horizontally
3. Diamond lifts, tooltip shows `2.0s → 3.2s`
4. Nearby keyframes show snap lines when aligned
5. Release → keyframe timing updated
6. Status bar confirms: "Position keyframe: 2.0s → 3.2s"

**For batch:** `Shift+click` multiple diamonds → drag selection together. `Alt+drag` → duplicate.

### 5.4 Multi-Scene Editing

**Goal:** Create a presentation with Intro fade-in, Diagram slide, Outro fade-out.

1. **Create scenes** (command palette: `> add scene` or sidebar)
   - Scenes appear as blocks in timeline scene row
2. **Edit Intro** (click Intro block)
   - Canvas shows Intro content, timeline shows Intro's actor tracks
   - Scrub to `0s`, add title actor
   - Scrub to `1s`, add `fade-in` action via `A` key → action block appears
3. **Set transition** (right-click gap between Intro and Diagram)
   - Select "Fade" → transition region appears
   - Click transition → edit duration/easing inline
4. **Edit Diagram** (click Diagram block)
   - Canvas switches to Diagram
   - Add actors, animate with keyframes
5. **Review globally** (press `G` or click "Global" in HUD)
   - Timeline shows all scenes
   - Scrubber moves through global time
   - Canvas shows active scene based on time

---

## 6. Keyboard Shortcuts

| Key | Action | Context |
|-----|--------|---------|
| `Space` | **Play / Pause** | Global |
| `T` + drag | **Time Lens scrub** | Canvas |
| `G` | **Toggle Global / Local scene view** | Global |
| `1`, `2`, `3`... | **Jump to scene N** | Global |
| `K` | **Toggle keyframe on focused property** | Property popup |
| `A` | **Open Actions palette** | Canvas (actor selected) |
| `V` | **Select / Viewport tool** | Global |
| `M` / `G` | **Move tool** | Global |
| `S` | **Scale tool** | Global |
| `R` | **Rotate tool** | Global |
| `E` | **Vertex edit tool** | Global |
| `⌘D` | **Duplicate selected** | Global |
| `Delete` | **Delete selected** | Global |
| `←/→` | **Frame step** (or nudge selected actor) | Global |
| `Shift + ←/→` | **Nudge 10px / 10 frames** | Global |
| `F` | **Fit view** | Preview |
| `0` | **Reset zoom/pan** | Preview |
| `⌘⇧P` / `⌘K` | **Command palette** | Global |
| `Esc` | **Cancel drag / deselect / close palette** | Global |
| `Enter` | **Edit focused property** | Property popup |
| `Tab` | **Next property** | Property popup |

---

## 7. Scaling to Multi-Viewport (PiP)

The design separates three concepts that are currently conflated:

| Concept | Definition | Current | Future |
|---------|-----------|---------|--------|
| **Scene** | Content definition (actors, keyframes, timeline) | Bound to viewport | Reusable content |
| **Viewport** | Spatial container (position, size, opacity) | Implicit fullscreen | Explicit instance |
| **Sequence** | When a viewport shows a scene | Scene duration = viewport duration | Viewport track with scene blocks |

**Timeline scales to viewport tracks:**
```
Viewport 1 (main):   [══════Intro══════][══════Diagram══════][══════Outro══════]
  Position:              ◆◆                  ◆◆
  Opacity:           ◆◆                      ◆◆
Viewport 2 (PiP):                        [▓▓▓webcam▓▓▓]
  Position:                              ◆◆
  Opacity:                           ◆◆      ◆◆
```

**Canvas scales to multiple viewport rectangles:**
```
┌──────────────────────────────────────────────────────┐
│  ┌──────────────────────────────────────────────┐   │
│  │        [Viewport 1: Intro scene]             │   │
│  └──────────────────────────────────────────────┘   │
│              ┌─────────────────────┐                │
│              │  [Viewport 2: webcam│                │
│              │   scene]            │                │
│              └─────────────────────┘                │
└──────────────────────────────────────────────────────┘
```

**Property popup scales to two levels:**
- **Level 1:** Viewport selected → shows scene assignment, position, size, opacity, border, mask
- **Level 2:** Actor selected → shows actor properties

**Selection system scales:**
| Selection | Gizmo | Popup |
|-----------|-------|-------|
| Nothing | Pan/zoom | Hidden |
| Viewport | Move/resize rectangle | Viewport props |
| Actor | Transform gizmo | Actor props |

---

## 8. Implementation Phases

### Phase 1: Foundation — Kill the Bars
**Effort:** 1 session  
**Files:** `app/shell/toolbar.rs`, `app/shell/transport_bar.rs`, `app/shell/nl_command_bar.rs`, `app/panels/preview_canvas/mod.rs`, `app/mod.rs`

- [ ] Delete NL Command Bar panel allocation
- [ ] Delete Preview Header (grid toggle, reset, overlays, status badge)
- [ ] Delete Transport Bar panel allocation
- [ ] Simplify top toolbar: filename, play, settings, command palette
- [ ] Move playback controls into timeline panel ruler strip
- [ ] Add command palette button (`⌘K`) to top bar
- [ ] Remove `keyframe_mode` toggle from UI

### Phase 2: Canvas Editing — Unified Gizmo + Property Popup
**Effort:** 2 sessions  
**Files:** `app/preview/`, `app/panels/preview_canvas/`, `app/components/`

- [ ] Unified transform gizmo with measurement lines
- [ ] Tool switching (`V`, `M`, `S`, `R`, `E`)
- [ ] Replace `floating_card.rs` with compact property popup
- [ ] Show 4 essentials + tabs (Transform, Style, Shape, Text)
- [ ] Per-property diamond buttons in popup
- [ ] Drag values left/right to change
- [ ] Auto-hide popup during canvas drag
- [ ] Auto-keyframe on canvas manipulation with undo toast

### Phase 3: Timeline Editing — Draggable Keyframes + Actions
**Effort:** 2 sessions  
**Files:** `app/panels/timeline_panel.rs`, `app/commands.rs`, `app/command_handlers.rs`

- [ ] Make keyframe diamonds draggable in timeline
- [ ] Add snap behavior (other KFs, ruler marks, 0.1s increments)
- [ ] Add visual feedback (lifted diamond, guide line, tooltip)
- [ ] Multi-select (`Shift+click`, box select)
- [ ] Action palette (`A` key or right-click)
- [ ] Action blocks in timeline (colored bars)
- [ ] Drag action block edges to resize duration
- [ ] Action block inline editor (easing, intensity)

### Phase 4: Multi-Scene Integration
**Effort:** 2 sessions  
**Files:** `app/panels/timeline_panel.rs`, `app/panels/mod.rs`, `app/mod.rs`, `app/preview/`

- [ ] Scene blocks in timeline scene row
- [ ] Click scene block → enter local editing mode
- [ ] Drag scene blocks to reorder / adjust timing
- [ ] Transition regions + inline editor
- [ ] Context HUD on canvas (scene name, local/global time)
- [ ] Breadcrumb in top bar
- [ ] `G` key to toggle global/local view
- [ ] Remove sidebar Scenes tab (functionality moved to timeline)

### Phase 5: Polish
**Effort:** 1 session  
**Files:** `app/design_tokens.rs`, `app/components/`, `app/preview/`

- [ ] Preview hover HUD (grid, guides, zoom)
- [ ] Toast notification system (replace diagnostics badge)
- [ ] Time lens trigger change (`Space` → `T` hold)
- [ ] Keyboard shortcut cheat sheet (command palette: `> shortcuts`)
- [ ] Animation for panel transitions (smooth expand/collapse)

---

## 9. Removed Components

| Component | Reason |
|-----------|--------|
| Transport bar | Playback moved to timeline; all other controls distributed |
| Preview header | Grid=G shortcut; Reset=0 key; Overlays in hover HUD; Status redundant |
| NL Command Bar | Non-functional placeholder; collapses to command palette button |
| Keyframe mode toggle | Replaced by per-property diamonds |
| Sidebar Scenes tab | Scenes managed in timeline scene row |
| Floating property card | Replaced by comprehensive property popup |
| Inspector panel (default visible) | Collapsed by default; property popup is primary |
| Rebuild button | Becomes auto-rebuild status indicator (not a button) |

---

## 10. Metrics

| Metric | Current | After Redesign | Change |
|--------|---------|----------------|--------|
| Persistent vertical chrome | ~140px (4 bars + preview header) | 28px (1 bar) | **-80%** |
| Preview canvas height | ~35-40% of screen | ~55-60% of screen | **+50%** |
| Button styles | 3 different families | 1 unified family | **Unified** |
| Time shown in | 2 places | 1 place | **Simplified** |
| Keyframe creation | Global on/off toggle | Per-property diamond | **Safer** |
| Keyframe timing adjustment | Edit code manually | Drag diamond | **Direct** |
| Action creation | Type code | Action palette + timeline blocks | **Visual** |
| Scene editing | Sidebar panel + mental math | Timeline blocks + explicit context | **Clear** |

---

## 11. Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Users miss removed buttons | Command palette (`⌘⇧P`) has all actions; keyboard shortcuts shown inline |
| Auto-keyframe creates spam | Undo toast on every auto-keyframe; merge window for rapid edits |
| Draggable timeline is complex | Start with click-to-jump, add drag in iteration; use snap generously |
| Multi-scene context confusion | Context HUD always visible; explicit "Global/Local" toggle; breadcrumb |
| Property popup too small | Tabs organize by category; scroll within tab; inspector panel still available |

---

## 12. Related Documents

- [`spec.md`](spec.md) — Language specification (actions, keyframes, scenes)
- [`architecture.md`](architecture.md) — System architecture (renderer, timeline, composition)
- [`roadmap.md`](roadmap.md) — Implementation schedule
