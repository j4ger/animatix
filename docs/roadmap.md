# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.  
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

**Principles**
- P0 architecture first — everything above it collapses if the foundation is shaky.
- The canvas is the hero. Any operation doable on canvas should not require panel input.
- Time is a HUD, not a permanent panel. Scrubbing is frequent but brief.
- Visual edits must write back to source. Otherwise text and visuals diverge again.

---

## 1. Core Architecture (P0)

### 1.1 LiveDocument — Unified Document Model

Merge `EditorBuffer` and `DocumentSession` into a single source of truth. The editor holds the canonical text; the file-system middle layer is removed.

- `cells: Vec<Cell>` owned by the editor.
- `last_valid_timeline: Timeline` caches the last successful parse.
- `diagnostics: Vec<Diagnostic>` for current errors.

Visual edits (dragging on canvas) sync back into Cell text via precise AST span replacement, not string find-and-replace.

**Key files:** `editor.rs`, `document.rs`, `app/mod.rs`

---

### 1.2 Command Pattern + Undo System

Replace the 40+ `Option<T>` fields in `UiActions` with a unified `Command` enum consumed from a `VecDeque<Command>` per frame. Undo stack stores `Command` reversals, not full text snapshots.

```rust
pub enum Command {
    Actor(ActorCommand),
    Scene(SceneCommand),
    Timeline(TimelineCommand),
    Property(PropertyCommand),
}
```

**Key files:** `app/mod.rs` (`update()` loop)

---

### 1.3 Store Architecture Split

Split the monolithic `AppState` into four independent stores, mutated via Command dispatch:

- `DocumentStore` — cells + lazy timeline parse.
- `RuntimeStore` — preview + playback.
- `UiStore` — panels, selection.
- `WorkspaceStore` — file tree, settings.

**Key files:** `app/mod.rs` (`AppState` struct)

---

### 1.4 Source Span System

`animatix_analyzer` must emit exact source locations (line, column, length) for every AST node. This is the foundation for all reverse edits (canvas → code).

**Key files:** `animatix_analyzer` crate (new output)

---

## 2. Canvas & Direct Manipulation (P1)

### 2.1 Canvas-Centric Layout Rebuild

Current: Editor 55 % | Preview + stacked panels 45 %.  
Target: Canvas dominates (60–70 %), with collapsible bottom bars.

```
┌─────────────────────────────────────────┐
│ Toolbar                                  │
├─────────────────────────────────────────┤
│              CANVAS (60-70%)            │
│          Preview + floating cards        │
├────────────────────┬────────────────────┤
│ Property Stream    │ Timeline Panel     │
│ (collapsible)      │ (Dope/Graph/Strip) │
└────────────────────┴────────────────────┘
```

Responsive breakpoints:
- > 1600 px: Canvas 70 % | Stream 15 % | Timeline 15 %
- 1200–1600 px: Canvas 65 % | Stream 20 % (collapsible) | Timeline 15 %
- < 1200 px: Canvas 100 % | Stream hidden (tab-summoned) | Timeline compressed strip

**Key files:** `app/mod.rs` (egui_tiles layout config)

---

### 2.2 Floating Property Cards

Replace the right-side Inspector panel. Selecting an actor pops a translucent card next to it with direct manipulators (color wheel, XY sliders, rotation dial). Edits reflect live in code. `Esc` dismisses.

**Key files:** new `app/preview/floating_card.rs`

---

### 2.3 2D Gizmo System

Add transform handles, bounding boxes, snap-line feedback, and multi-selection batch operations.

```rust
pub enum Handle {
    TranslateX, TranslateY, TranslateXY,
    Rotate,
    ScaleCorner, ScaleEdge,
}
```

Interactions:
- Drag actor = move (syncs Cell position).
- Shift + corner = uniform scale.
- Show alignment guides / grid-snap feedback.
- Multi-select = union bounding box.

**Key files:** new `app/preview/gizmo.rs`, `app/preview/mod.rs`

---

### 2.4 Ghost Edit / Onion Skin

Not a manual toggle. Selecting a keyframe automatically shows context:
- Prior frame outline (green dashed, 30 % opacity).
- Next frame outline (blue dashed, 30 % opacity).
- Motion-path line.
- Ghost stays fixed as reference while dragging.

**Key files:** `app/preview/mod.rs` (render overlay)

---

## 3. Timeline & Time Controls (P1)

### 3.1 Time Lens — Space-Drag HUD

Timeline panel eats permanent space, but scrubbing is frequent yet brief. Make time an on-demand HUD:

- Hold `Space` → circular time lens appears at cursor.
- Ring shows keyframe dots.
- Drag to change time; center shows timecode.
- Scroll wheel zooms time range.
- Release `Space` → lens vanishes.

**Key files:** new `app/preview/time_lens.rs`

---

### 3.2 Global Timeline Panel

Consolidate the scattered transport bar, keyframe table, and dope sheet into a single bottom-right panel:

- Scene track: scene ordering.
- Actor track: one row per actor.
- Keyframes: distinct shapes per type.
- Playhead: draggable, linked to preview.
- Range slider: work / export range.
- Markers.

**Key files:** new `app/panels/timeline_panel.rs`

---

### 3.3 Time-Aware Inspector

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

### 3.4 Property Stream

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

### 3.5 Graph Editor (F-Curve)

Missing: value-over-time curves, making easing strength tuning guesswork.

Add view toggle in Inspector keyframe area: List | Curve | Strip. Simplified first pass supports single float properties (position.x, rotation); extend to other types later.

**Key files:** new `app/panels/inspector/graph_editor.rs`

---

## 4. Differentiating Features (P2)

### 4.1 Natural-Language Command Bar

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

### 4.2 Agent Inline Suggestions

Agent surfaces in four shapes:

| Shape | Example |
|---|---|
| Inline suggestion | Below `position = (100, 200)`: "← try (120, 200) to align with Circle_2?" |
| Lightweight toast | "Looping motion detected — add oscillate()?" |
| Diff card | Show code diff; accept / reject |
| Command bar | Complex request entry |

**Key files:** new `app/components/`

---

### 4.3 Diff Preview

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

### 4.4 Smart Snap

Not pixel snap — semantic snap. While dragging, auto-snap to:

- Other actor bounds (geometry).
- Other actor position values (numeric → `position = Circle_2.position`).
- Layout container alignment lines (semantic).
- Previous keyframe position (time).

HUD shows the specific snap target on contact.

**Key files:** `app/preview/mod.rs`

---

### 4.5 Scene Slices

Figma-Variants / Photoshop-Artboards style: compare animation scenes A/B/C side by side.

Operations: duplicate slice, drag actor across slices, `1`/`2`/`3` hotkeys to switch, batch export.

**Key files:** new `app/panels/scene_slices.rs`

---

## 5. Visual Polish & Tooling (P3)

### 5.1 Design Token System

Colors, spacing, radius, and typography are currently hard-coded and scattered.

Token groups:
- `color`: `SURFACE_BASE`, `ELEVATED`, `WIDGET`, `TEXT_PRIMARY`, `TEXT_SECONDARY`, `ACCENT`, `SUCCESS`, `WARNING`, `ERROR`
- `spacing`: `XS`, `S`, `M`, `L`, `XL`, `XXL`
- `radius`: `NONE`, `SM`, `MD`, `LG`, `FULL`
- `typography`: `H1`, `H2`, `BODY`, `CAPTION`, `MONO`

Helper: `lint_theme.py` scans for hard-coded values.

**Key files:** `app/theme.rs` → `app/design_tokens.rs`

---

### 5.2 Cell Editor Visual Redesign

- Accent left border on focus.
- Large, prominent keyframe timestamp (accent color, clickable edit).
- Cell-level fold / unfold.
- Cell-level move-up / move-down / delete buttons (hover-reveal).
- Stronger visual distinction between code cells and keyframe cells.

**Key files:** `cell_editor/render.rs`

---

### 5.3 Preview Overlay System

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

### 5.4 Semantic Highlighting + Refactoring Tools

Deep `animatix_analyzer` integration:
- `SymbolTable`: actor / scene / component definitions.
- Semantic coloring: `ActorName`, `PropertyName`, `SceneName`, `Invalid` (red squiggle).
- Basic refactorings: `RenameActor`, `ExtractScene`, `MoveToScene`.

**Key files:** `cell_editor/highlighting.rs`, `completion_popup.rs`

---

## 6. Long-Term / Speculative

### 6.1 FFI / Web Canvas Integration

Enable web deployment by targeting HTML5 Canvas or WebGPU via wasm-bindgen.

**Effort:** Very High. Alternative renderer backend.

---

### 6.2 Lossless Syntax Tree (Green Tree)

**Location:** `docs/architecture.md` §Source Write-Back.

Adopt a `rowan`-style green-tree architecture for full-fidelity source preservation.

**Effort:** Very High. 3–6 month project. Not justified at current scale.

---

### 6.3 Trivia-Inspired AST

Add leading/trailing trivia (comments, whitespace) to AST nodes for better formatting preservation during GUI write-back.

**Effort:** High. Massive parser rewrite.

---

## 7. Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | LiveDocument unified model (1.1) | High | Very High |
| 2 | Command pattern + undo (1.2) | High | Very High |
| 3 | Store architecture split (1.3) | Medium | High |
| 4 | Source span system (1.4) | Medium | High |
| 5 | Canvas-centric layout (2.1) | Medium | High |
| 6 | 2D gizmo system (2.3) | Medium | High |
| 7 | Floating property cards (2.2) | Medium | Medium |
| 8 | Global timeline panel (3.2) | Medium | High |
| 9 | Time-aware inspector (3.3) | Low | Medium |
| 10 | Property stream (3.4) | Low | Medium |
| 11 | Time lens HUD (3.1) | Medium | Medium |
| 12 | Graph editor / F-curve (3.5) | High | Medium |
| 13 | Ghost edit / onion skin (2.4) | Medium | Medium |
| 14 | Smart snap (4.4) | Medium | Medium |
| 15 | Diff preview (4.3) | Medium | Medium |
| 16 | Design token system (5.1) | Low | Medium |
| 17 | Cell editor visual redesign (5.2) | Low | Medium |
| 18 | Preview overlay system (5.3) | Low | Low |
| 19 | Semantic highlighting + refactor (5.4) | High | Medium |
| 20 | NL command bar (4.1) | High | High |
| 21 | Agent inline suggestions (4.2) | High | High |
| 22 | Scene slices (4.5) | Medium | Medium |
| 23 | Green tree / trivia AST (6.2) | Very High | Low |
| 24 | Web Canvas (6.1) | Very High | Low |
| 25 | Trivia-inspired AST (6.3) | High | Low |
