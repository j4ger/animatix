# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.  
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

**Principles**
- P0 architecture first — everything above it collapses if the foundation is shaky.
- The canvas is the hero. Any operation doable on canvas should not require panel input.
- Time is a HUD, not a permanent panel. Scrubbing is frequent but brief.
- Visual edits must write back to source. Otherwise text and visuals diverge again.

---

## 1. Timeline & Time Controls (P1)

### 1.1 Time Lens — Space-Drag HUD

Timeline panel eats permanent space, but scrubbing is frequent yet brief. Make time an on-demand HUD:

- Hold `Space` → circular time lens appears at cursor.
- Ring shows keyframe dots.
- Drag to change time; center shows timecode.
- Scroll wheel zooms time range.
- Release `Space` → lens vanishes.

**Key files:** new `app/preview/time_lens.rs`

---

### 1.2 Global Timeline Panel

Consolidate the scattered transport bar, keyframe table, and dope sheet into a single bottom-right panel:

- Scene track: scene ordering.
- Actor track: one row per actor.
- Keyframes: distinct shapes per type.
- Playhead: draggable, linked to preview.
- Range slider: work / export range.
- Markers.

**Key files:** new `app/panels/timeline_panel.rs`

---

### 1.3 Time-Aware Inspector

Users cannot tell whether they are editing the default value or a keyframe value. Add diamond status per property row:

```
Position  [ 100 │ 200 ]  ◆ 0.0s   ← keyframe exists
Rotation  [ 45° ]        ◆ 0.5s
Scale     [ 1.0 ]        ○         ← no keyframe; edits default
```

- Click `○` → create keyframe at current time.
- Click `◆` → keyframe action menu.
- Keyframe Mode: editing off-keyframe time auto-creates a keyframe.

**Key files:** `app/panels/inspector/mod.rs` (`property_widget`)

---

### 1.4 Property Stream

Sort properties by animation intensity, not semantic grouping (Transform / Style / Shape / Text / Media).

```
🔥 position     ◆◆◆○○○○○○○○  (12 kf)
🔥 rotation     ◆◆◆○○○○○○○○   (8 kf)
─────────────────────────────────────
  color          ○○○○○○○○○○○   (0 kf)
  scale          ○○○○○○○○○○○   (0 kf)
```

- Default sort: animation intensity.
- `Tab` toggles semantic category view.

**Key files:** `app/panels/inspector/mod.rs`

---

### 1.5 Graph Editor (F-Curve)

Missing: value-over-time curves, making easing strength tuning guesswork.

Add view toggle in Inspector keyframe area: List | Curve | Strip. Simplified first pass supports single float properties (position.x, rotation); extend to other types later.

**Key files:** new `app/panels/inspector/graph_editor.rs`

---

## 2. Differentiating Features (P2)

### 2.1 Natural-Language Command Bar

Persistent lightweight input bar at the top:

```
File  Edit  View  │  [让 Circle_1 绕中心旋转一周]  │  ⌘K
```

- `⌘K` focuses.
- Live preview of what the agent intends (code diff).
- `Enter` confirm, `Esc` cancel.
- Up/down browses command history.

**Key files:** new `app/shell/nl_command_bar.rs`

---

### 2.2 Agent Inline Suggestions

Agent surfaces in four shapes:

| Shape | Example |
|---|---|
| Inline suggestion | Below `position = (100, 200)`: "← try (120, 200) to align with Circle_2?" |
| Lightweight toast | "Looping motion detected — add oscillate()?" |
| Diff card | Show code diff; accept / reject |
| Command bar | Complex request entry |

**Key files:** new `app/components/`

---

### 2.3 Diff Preview

On property change, auto A/B split-screen:

```
┌─────────────┬─────────────┐
│  Before     │  After      │
│  [ ○ ]      │  [ ○  ]     │
└─────────────┴─────────────┘
```

Leverage AMX fast reparse: compile two timeline versions and render both.

**Key files:** `app/preview/mod.rs`

---

### 2.4 Smart Snap

Not pixel snap — semantic snap. While dragging, auto-snap to:

- Other actor bounds (geometry).
- Other actor position values (numeric → `position = Circle_2.position`).
- Layout container alignment lines (semantic).
- Previous keyframe position (time).

HUD shows the specific snap target on contact.

**Key files:** `app/preview/mod.rs`

---

### 2.5 Scene Slices

Figma-Variants / Photoshop-Artboards style: compare animation scenes A/B/C side by side.

Operations: duplicate slice, drag actor across slices, `1`/`2`/`3` hotkeys to switch, batch export.

**Key files:** new `app/panels/scene_slices.rs`

---

## 3. Visual Polish & Tooling (P3)

### 3.1 Design Token System

Colors, spacing, radius, and typography are currently hard-coded and scattered.

Token groups:
- `color`: `SURFACE_BASE`, `ELEVATED`, `WIDGET`, `TEXT_PRIMARY`, `TEXT_SECONDARY`, `ACCENT`, `SUCCESS`, `WARNING`, `ERROR`
- `spacing`: `XS`, `S`, `M`, `L`, `XL`, `XXL`
- `radius`: `NONE`, `SM`, `MD`, `LG`, `FULL`
- `typography`: `H1`, `H2`, `BODY`, `CAPTION`, `MONO`

Helper: `lint_theme.py` scans for hard-coded values.

**Key files:** `app/theme.rs` → `app/design_tokens.rs`

---

### 3.2 Cell Editor Visual Redesign

- Accent left border on focus.
- Large, prominent keyframe timestamp (accent color, clickable edit).
- Cell-level fold / unfold.
- Cell-level move-up / move-down / delete buttons (hover-reveal).
- Stronger visual distinction between code cells and keyframe cells.

**Key files:** `cell_editor/render.rs`

---

### 3.3 Preview Overlay System

```rust
pub struct PreviewOverlay {
    show_scene_bounds: bool,
    show_grid: bool,
    show_guides: bool,
    show_actor_labels: bool,
    show_safe_area: bool,
}
```

**Key files:** new `app/preview/overlay.rs`

---

### 3.4 Semantic Highlighting + Refactoring Tools

Deep `animatix_analyzer` integration:
- `SymbolTable`: actor / scene / component definitions.
- Semantic coloring: `ActorName`, `PropertyName`, `SceneName`, `Invalid` (red squiggle).
- Basic refactorings: `RenameActor`, `ExtractScene`, `MoveToScene`.

**Key files:** `cell_editor/highlighting.rs`, `completion_popup.rs`

---

## 4. Long-Term / Speculative

### 4.1 FFI / Web Canvas Integration

Enable web deployment by targeting HTML5 Canvas or WebGPU via wasm-bindgen.

**Effort:** Very High. Alternative renderer backend.

---

### 4.2 Lossless Syntax Tree (Green Tree)

**Location:** `docs/architecture.md` §Source Write-Back.

Adopt a `rowan`-style green-tree architecture for full-fidelity source preservation.

**Effort:** Very High. 3–6 month project. Not justified at current scale.

---

### 4.3 Trivia-Inspired AST

Add leading/trailing trivia (comments, whitespace) to AST nodes for better formatting preservation during GUI write-back.

**Effort:** High. Massive parser rewrite.

---

## 5. Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | Global timeline panel (1.2) | Medium | High |
| 2 | Time-aware inspector (1.3) | Low | Medium |
| 3 | Property stream (1.4) | Low | Medium |
| 4 | Time lens HUD (1.1) | Medium | Medium |
| 5 | Graph editor / F-curve (1.5) | High | Medium |
| 6 | Smart snap (2.4) | Medium | Medium |
| 7 | Diff preview (2.3) | Medium | Medium |
| 8 | Design token system (3.1) | Low | Medium |
| 9 | Cell editor visual redesign (3.2) | Low | Medium |
| 10 | Preview overlay system (3.3) | Low | Low |
| 11 | Semantic highlighting + refactor (3.4) | High | Medium |
| 12 | NL command bar (2.1) | High | High |
| 13 | Agent inline suggestions (2.2) | High | High |
| 14 | Scene slices (2.5) | Medium | Medium |
| 15 | Green tree / trivia AST (4.2) | Very High | Low |
| 16 | Web Canvas (4.1) | Very High | Low |
| 17 | Trivia-inspired AST (4.3) | High | Low |
