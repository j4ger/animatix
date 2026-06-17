# Animatix GUI Design Language

> Authoritative specification for the visual design language, token system,
> component taxonomy, interaction model, and migration plan for the Animatix GUI.
>
> **Status**: Draft — supersedes ad-hoc conventions in `design_tokens.rs` once
> Phase 1 migration lands.

---

## Table of Contents

1. [Design Philosophy](#1-design-philosophy)
2. [Token System](#2-token-system)
3. [Typography](#3-typography)
4. [Spatial System](#4-spatial-system)
5. [Color System](#5-color-system)
6. [Component Taxonomy](#6-component-taxonomy)
7. [Interaction Language](#7-interaction-language)
8. [Motion Language](#8-motion-language)
9. [Layout System](#9-layout-system)
10. [Accessibility Constraints](#10-accessibility-constraints)
11. [Migration Plan](#11-migration-plan)

---

## 1. Design Philosophy

Five non-negotiable principles that govern every GUI decision.

### P1. Canvas-First

The preview canvas is the protagonist. All UI chrome (toolbars, panels,
inspectors) must visually recede so rendered content breathes. Panel
backgrounds are darker than the canvas; borders are near-invisible; the only
"loud" UI element is the active selection.

### P2. Time is a First-Class Citizen

The timeline is not a subordinate panel — it is an interaction surface
equal to the preview canvas. Playhead position, keyframes, and scene
boundaries must be visually consistent and synchronized across every panel
that references time.

### P3. Source is Truth

All visual edits ultimately map to `.amx` source. The UI is a *lens* onto
the source, not a replacement. Every edit must round-trip. When the UI
and source disagree, source wins.

### P4. Progressive Complexity

Beginners see a minimal toolset (select, move, play). Advanced features
(curve editor, spreadsheet view, action timeline) unfold on demand and
never occupy screen space by default.

### P5. Reversibility

Every user action is undoable. No irreversible confirmation dialogs for
operations that *could* be undoable. Only truly irreversible operations
(file overwrite, workspace switch with unsaved changes) may intercept.

---

## 2. Token System

### 2.1 Three-Layer Architecture

```
Layer 1: Primitive   — raw values (hex colors, pixel counts)
                       Visibility: pub(crate) — never referenced outside tokens/
                       
Layer 2: Semantic    — role-based names mapped from primitives
                       Visibility: pub — the public API consumed by all UI code
                       
Layer 3: Component   — per-component token overrides
                       Visibility: pub — lives alongside the component module
```

### 2.2 Rust Module Layout

The token system uses Rust's module system for grouping and visibility
control — no structs, no runtime structs, no trait objects. The compiler
enforces the layering via `pub(crate)` vs `pub`.

```rust
// crates/animatix-gui/src/app/design_tokens/mod.rs

/// Primitive token values. Never import from outside this crate's
/// `design_tokens` module. All consumption goes through `semantic`.
pub(crate) mod primitive;

/// Semantic tokens — the public API. All UI code imports from here.
pub mod semantic;

/// Utility functions (lerp, alpha-multiply) — kept for legacy compat
/// during migration, eventually replaced by pre-computed constants.
pub mod util;
```

```rust
// crates/animatix-gui/src/app/design_tokens/primitive.rs

use egui::Color32;

// ── Raw palette ──
pub(crate) const BLUE_500: Color32 = Color32::from_rgb(84, 110, 255);
pub(crate) const BLUE_400: Color32 = Color32::from_rgb(120, 145, 255);
pub(crate) const BLUE_600: Color32 = Color32::from_rgb(60, 84, 220);

pub(crate) const GRAY_950: Color32 = Color32::from_rgb(10, 12, 16);
pub(crate) const GRAY_900: Color32 = Color32::from_rgb(16, 18, 23);
pub(crate) const GRAY_850: Color32 = Color32::from_rgb(22, 25, 31);
pub(crate) const GRAY_800: Color32 = Color32::from_rgb(30, 34, 42);
pub(crate) const GRAY_700: Color32 = Color32::from_rgb(42, 47, 57);
pub(crate) const GRAY_600: Color32 = Color32::from_rgb(60, 66, 78);
pub(crate) const GRAY_500: Color32 = Color32::from_rgb(90, 97, 112);
pub(crate) const GRAY_400: Color32 = Color32::from_rgb(130, 138, 153);
pub(crate) const GRAY_300: Color32 = Color32::from_rgb(170, 178, 192);
pub(crate) const GRAY_200: Color32 = Color32::from_rgb(210, 216, 228);
pub(crate) const GRAY_100: Color32 = Color32::from_rgb(232, 236, 245);

pub(crate) const GREEN_500: Color32 = Color32::from_rgb(80, 200, 140);
pub(crate) const AMBER_500: Color32 = Color32::from_rgb(255, 196, 92);
pub(crate) const RED_500: Color32 = Color32::from_rgb(255, 100, 100);
pub(crate) const CYAN_500: Color32 = Color32::from_rgb(137, 200, 235);
pub(crate) const PURPLE_500: Color32 = Color32::from_rgb(156, 39, 176);
```

```rust
// crates/animatix-gui/src/app/design_tokens/semantic.rs

use egui::Color32;
use super::primitive as p;

// ── Surface (5 depth layers) ──

pub mod surface {
    use super::*;
    /// Depth 0: window bottom layer
    pub const BASE: Color32 = p::GRAY_950;
    /// Depth 1: panel background
    pub const PANEL: Color32 = p::GRAY_900;
    /// Depth 2: cards / floating surfaces
    pub const SURFACE: Color32 = p::GRAY_850;
    /// Depth 3: widgets (inputs, buttons)
    pub const WIDGET: Color32 = p::GRAY_800;
    /// Depth 4: hover overlay
    pub const HOVER: Color32 = p::GRAY_700;
    /// Depth 4+: active/pressed
    pub const ACTIVE: Color32 = p::GRAY_600;
}

// ── Text ──

pub mod text {
    use super::*;
    pub const PRIMARY: Color32 = p::GRAY_100;
    pub const SECONDARY: Color32 = p::GRAY_400;
    pub const MUTED: Color32 = p::GRAY_500;
    pub const DISABLED: Color32 = p::GRAY_600;
}

// ── Accent ──

pub mod accent {
    use super::*;
    pub const PRIMARY: Color32 = p::BLUE_500;
    pub const PRIMARY_HOVER: Color32 = p::BLUE_400;
    pub const PRIMARY_ACTIVE: Color32 = p::BLUE_600;

    // Pre-computed alpha variants (no runtime linear_multiply)
    pub const SELECTION: Color32 =
        Color32::from_rgba_unmultiplied(84, 110, 255, 60);
    pub const FAINT: Color32 =
        Color32::from_rgba_unmultiplied(84, 110, 255, 30);
    pub const GHOST: Color32 =
        Color32::from_rgba_unmultiplied(84, 110, 255, 80);
    pub const SUBTLE: Color32 =
        Color32::from_rgba_unmultiplied(84, 110, 255, 120);
}

// ── Status ──

pub mod status {
    use super::*;
    pub const SUCCESS: Color32 = p::GREEN_500;
    pub const WARNING: Color32 = p::AMBER_500;
    pub const ERROR: Color32 = p::RED_500;
    pub const INFO: Color32 = p::BLUE_500;

    pub const SUCCESS_FAINT: Color32 =
        Color32::from_rgba_unmultiplied(80, 200, 140, 60);
    pub const WARNING_FAINT: Color32 =
        Color32::from_rgba_unmultiplied(255, 196, 92, 60);
    pub const ERROR_FAINT: Color32 =
        Color32::from_rgba_unmultiplied(255, 100, 100, 60);
}

// ── Category (timeline property groups, scene tracks, etc.) ──

pub mod category {
    use super::*;
    pub const TRANSFORM: Color32 = p::BLUE_500;
    pub const STYLE: Color32 = p::GREEN_500;
    pub const SHAPE: Color32 = p::AMBER_500;
    pub const TEXT: Color32 = p::CYAN_500;
    pub const ACTION: Color32 = p::PURPLE_500;
}

// ── Borders ──

pub mod border {
    use super::*;
    pub const DEFAULT: Color32 = p::GRAY_700;
    pub const HOVER: Color32 = p::GRAY_600;
    pub const FOCUS: Color32 = p::BLUE_500;
}

// ── Canvas-specific (does NOT share UI chrome tokens) ──

pub mod canvas {
    use super::*;
    pub const BG: Color32 = Color32::from_rgb(8, 8, 12);
    pub const GRID_LINE: Color32 =
        Color32::from_rgba_unmultiplied(255, 255, 255, 12);
    pub const GUIDE_LINE: Color32 =
        Color32::from_rgba_unmultiplied(255, 255, 255, 30);
    pub const SELECTION_MARQUEE: Color32 =
        Color32::from_rgba_unmultiplied(84, 110, 255, 200);
    pub const HANDLE_FILL: Color32 = p::GRAY_100;
    pub const HANDLE_STROKE: Color32 = p::BLUE_500;
    pub const GHOST_PREV: Color32 =
        Color32::from_rgba_unmultiplied(80, 220, 120, 77);
    pub const GHOST_NEXT: Color32 =
        Color32::from_rgba_unmultiplied(80, 160, 255, 77);
    pub const SNAP_GUIDE: Color32 =
        Color32::from_rgba_unmultiplied(84, 191, 123, 160);
}
```

### 2.3 Token Constraints

| Rule | Enforcement |
|------|-------------|
| UI code must not import `primitive` | `pub(crate)` visibility — compiler error if leaked |
| No runtime `linear_multiply` for alpha | Pre-computed `const` values only |
| Status and Category colors must not be shared | Separate modules, separate types |
| Every semantic color must exist in both dark and light themes | Phase 2: dual-theme `cfg` or runtime swap |
| Component-level tokens live in component modules, not in `design_tokens/` | Convention + code review |

### 2.4 Import Convention

All UI modules import semantic tokens via a glob:

```rust
use crate::app::design_tokens::semantic::*;
```

For canvas-specific code:

```rust
use crate::app::design_tokens::semantic::canvas;
```

---

## 3. Typography

### 3.1 Type Scale

8 levels based on a 1.2 ratio. Each level specifies size, line-height, and
weight.

| Role | Size | Line-height | Weight | Usage |
|------|------|-------------|--------|-------|
| Display | 20px | 1.2 | 700 | Welcome screen title |
| Heading | 18px | 1.3 | 600 | Dialog titles |
| Title | 15px | 1.3 | 600 | Panel section headers |
| Body | 13px | 1.4 | 400 | Default text |
| BodyS | 12px | 1.4 | 400 | Compact text, toolbar labels |
| Caption | 11px | 1.3 | 400 | Labels, helper text |
| Mono | 12px | 1.4 | 400 | Timecodes, coordinates, numbers |
| Micro | 10px | 1.2 | 500 | Badges, status indicators |

### 3.2 Rust API

```rust
// crates/animatix-gui/src/app/design_tokens/typography.rs

pub enum TextRole {
    Display,
    Heading,
    Title,
    Body,
    BodyS,
    Caption,
    Mono,
    Micro,
}

impl TextRole {
    pub fn font_id(&self) -> egui::FontId {
        let (size, family) = match self {
            Self::Display => (20.0, egui::FontFamily::Proportional),
            Self::Heading => (18.0, egui::FontFamily::Proportional),
            Self::Title => (15.0, egui::FontFamily::Proportional),
            Self::Body => (13.0, egui::FontFamily::Proportional),
            Self::BodyS => (12.0, egui::FontFamily::Proportional),
            Self::Caption => (11.0, egui::FontFamily::Proportional),
            Self::Mono => (12.0, egui::FontFamily::Monospace),
            Self::Micro => (10.0, egui::FontFamily::Proportional),
        };
        egui::FontId::new(size, family)
    }
}
```

### 3.3 Typography Constraints

1. Never use raw `FontId::new(13.0, ...)`. Always go through `TextRole`.
2. Numeric values (timecodes, coordinates, zoom %) must use `TextRole::Mono`
   and right-align.
3. No `to_uppercase()` for visual hierarchy. Use weight + color contrast
   instead. (Replaces current `section_header` convention.)
4. Text selection is disabled for non-editable labels (`.selectable(false)`).

---

## 4. Spatial System

### 4.1 Unified Scale

Single 9-step scale based on a 2px base. Replaces the current dual
`SPACE_*` / `PAD_*` system.

| Token | Value | Usage |
|-------|-------|-------|
| `SPACE_0` | 0px | No spacing |
| `SPACE_1` | 2px | Icon internals |
| `SPACE_2` | 4px | Tight element gaps |
| `SPACE_3` | 6px | Default element gap |
| `SPACE_4` | 8px | Component inner padding |
| `SPACE_5` | 12px | Inter-component gap |
| `SPACE_6` | 16px | Panel inner padding |
| `SPACE_7` | 24px | Section / panel gap |
| `SPACE_8` | 32px | Page-level spacing |

### 4.2 Row Heights

| Token | Value | Usage |
|-------|-------|-------|
| `ROW_XS` | 18px | Dense lists |
| `ROW_S` | 20px | Compact rows |
| `ROW_M` | 24px | Default row (minimum touch target) |
| `ROW_L` | 28px | Toolbar buttons |

### 4.3 Corner Radii

| Token | Value | Usage |
|-------|-------|-------|
| `RADIUS_S` | 2px | Small badges, inline elements |
| `RADIUS_M` | 4px | Default — buttons, inputs, cards |
| `RADIUS_L` | 6px | Panels, larger surfaces |
| `RADIUS_XL` | 8px | Dialogs, modals |

### 4.4 Spatial Constraints

1. All spacing values must come from the scale. No magic numbers.
2. `PAD_*` constants are deleted; `SPACE_*` is the only spacing system.
3. Panel inner padding = `SPACE_6` (16px).
4. Component inner padding = `SPACE_4` (8px).
5. Minimum touch target = `ROW_M` (24px) in both dimensions.

---

## 5. Color System

### 5.1 Surface Depth (5 layers)

```
Depth 0  BASE       #0A0C10   — window background
Depth 1  PANEL      #101217   — panel background
Depth 2  SURFACE    #16191F   — cards, floating surfaces
Depth 3  WIDGET     #1E222A   — inputs, buttons
Depth 4  HOVER      #2A2F39   — hover overlay
         ACTIVE     #3C423A   — pressed/active
```

Adjacent layers must differ by >= 6% luminance. The current values
(`#0C0E12` -> `#121418`) differ by only ~2.3% — the new values above
correct this.

### 5.2 Semantic Color Roles

```
Accent:
  PRIMARY          #546EFF   — primary interaction color
  PRIMARY_HOVER    #7891FF
  PRIMARY_ACTIVE   #3C54DC
  SELECTION        rgba(84,110,255,60)  — selection fill
  FAINT            rgba(84,110,255,30)  — subtle accent bg
  GHOST            rgba(84,110,255,80)  — ghost outline

Status (never reused as category):
  SUCCESS          #50C88C
  WARNING          #FFC45C
  ERROR            #FF6464
  INFO             #546EFF

Category (never reused as status):
  TRANSFORM        #546EFF   — position/rotation/scale
  STYLE            #50C88C   — color/opacity/stroke
  SHAPE            #FFC45C   — geometry parameters
  TEXT             #89C8EB   — text properties
  ACTION           #9C27B0   — action blocks
```

### 5.3 Color Constraints

1. **Status and Category colors are disjoint sets.** The current code reuses
   `GREEN` for both "success" and "style category" — this is forbidden.
2. Every semantic color has 5 states: default, hover, active, disabled,
   focused. Disabled = `text::DISABLED` color; focused = `accent::PRIMARY`
   outline.
3. Selection uses alpha-tinted accent (60/255), never solid fill.
4. Canvas colors live in `semantic::canvas` and do not share tokens with
   UI chrome.

---

## 6. Component Taxonomy

### 6.1 Three Layers

```
Primitive  →  Pattern  →  Domain
(button)      (toolbar)   (inspector panel)
```

#### Primitive Components

| Component | Replaces | Notes |
|-----------|----------|-------|
| `Button` | `icon_button`, `icon_button_colored`, `toolbar_toggle_button`, `toolbar_action_button` | Unified widget with variant builder |
| `TextInput` | raw `egui::TextEdit` wrapped in `field_sized` | Themed input with focus ring |
| `NumberInput` | raw `egui::DragValue` | Themed with mono font |
| `Toggle` | raw `egui::Checkbox` | Switch-style toggle |
| `Select` | raw `egui::ComboBox` | Themed dropdown |
| `Slider` | raw `egui::Slider` | Themed with accent track |
| `Tooltip` | `on_hover_text` | Consistent delay + styling |
| `Badge` | ad-hoc `Frame` badges | Status + count badges |
| `Separator` | `toolbar_separator` | Horizontal + vertical |

#### Pattern Components

| Component | Replaces | Notes |
|-----------|----------|-------|
| `LabeledRow` | `labeled_row` | Label-left / input-right |
| `FieldGroup` | ad-hoc section groupings | Titled group with optional collapse |
| `PillTabs` | `pill_tab_bar` | Segmented tab control |
| `ContextMenu` | `context_menu` module | Already good — keep |
| `Toolbar` | `toolbar_ui` method | Composable toolbar group |
| `Breadcrumb` | inline in `toolbar_ui` | Scene navigation breadcrumb |
| `EmptyState` | `empty_state` | Already good — keep |

#### Domain Panels

| Panel | File | Notes |
|-------|------|-------|
| `InspectorPanel` | `panels/inspector/` | Keep structure, migrate tokens |
| `TimelinePanel` | `panels/timeline_panel.rs` | Keep structure, migrate tokens |
| `PreviewCanvas` | `panels/preview_panel.rs` + `preview/` | Keep structure, migrate tokens |
| `SidebarPanel` | `panels/sidebar.rs` | Keep structure, migrate tokens |
| `EditorPanel` | `panels/editor.rs` | Keep structure, migrate tokens |

### 6.2 Unified Button API

```rust
pub struct Button {
    variant: ButtonVariant,
    size: ButtonSize,
    icon: Option<&'static str>,
    label: Option<String>,
    tooltip: Option<String>,
    disabled: bool,
    active: bool,
}

pub enum ButtonVariant {
    Primary,   // solid accent fill
    Secondary, // widget fill with border
    Ghost,     // transparent, hover shows bg
    Icon,      // square, icon-only
}

pub enum ButtonSize {
    Small,  // ROW_S height
    Medium, // ROW_M height (default)
    Large,  // ROW_L height
}

impl Button {
    pub fn primary(label: impl Into<String>) -> Self { ... }
    pub fn secondary(label: impl Into<String>) -> Self { ... }
    pub fn ghost(label: impl Into<String>) -> Self { ... }
    pub fn icon(icon: &'static str) -> Self { ... }

    pub fn small(mut self) -> Self { self.size = Small; self }
    pub fn with_tooltip(mut self, tip: impl Into<String>) -> Self { ... }
    pub fn disabled(mut self) -> Self { self.disabled = true; self }
    pub fn active(mut self) -> Self { self.active = true; self }
    pub fn with_icon(mut self, icon: &'static str) -> Self { ... }
}

impl egui::Widget for Button {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        // Single state machine: default → hover → active → disabled → focused
        // Single paint path driven by variant + state
    }
}
```

### 6.3 Component Constraints

1. Primitive components implement `egui::Widget` — no free functions.
2. Every Primitive supports all 5 interaction states.
3. Pattern components compose Primitives — no direct `painter` calls.
4. Domain panels compose Patterns + Primitives — no direct `painter` calls
   for standard UI elements (canvas painting is exempt).
5. Component-level tokens live in the component's module as `const` values
   referencing semantic tokens.

---

## 7. Interaction Language

### 7.1 Gesture System

Replace the 763-line `drag_handler.rs` with a typed gesture layer:

```rust
pub enum Gesture {
    Tap { pos: Pos2 },
    DoubleTap { pos: Pos2 },
    DragStart { pos: Pos2, button: PointerButton },
    DragMove { start: Pos2, current: Pos2, delta: Vec2 },
    DragEnd { start: Pos2, end: Pos2 },
    Hover { pos: Pos2 },
}

pub trait GestureHandler {
    fn on_gesture(&mut self, gesture: &Gesture, ctx: &mut InteractionCtx)
        -> GestureResult;
}

pub enum GestureResult {
    Consumed,       // gesture handled, stop propagation
    Rejected,       // not interested, let next handler try
    Capture,        // capture all subsequent gestures until release
}
```

### 7.2 Keyboard Navigation

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Cycle focus between interactive elements |
| `Arrow Keys` | Move selected actor on canvas (1px; `Shift` = 10px) |
| `Enter` | Confirm / enter edit mode |
| `Escape` | Cancel / exit edit / clear selection |
| `Space` | Play/Pause (disabled when text input is focused) |
| `Delete` / `Backspace` | Delete selected actors |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / Redo |
| `Ctrl+S` | Save |
| `Ctrl+R` | Reload from disk |
| `Ctrl+Shift+R` | Rebuild timeline |
| `[` / `]` | Previous / next keyframe |
| `,` / `.` | Frame step backward / forward |

### 7.3 Command System Refactor

Split the 50+ variant `Command` enum into domain packages:

```rust
// commands/document.rs — undoable
pub enum DocumentCommand {
    Save, Reload, Rebuild,
    OpenFile(PathBuf), SwitchWorkspace(PathBuf),
}

// commands/actor.rs — undoable
pub enum ActorCommand {
    Create { ty, label, position, props },
    Delete(String), Duplicate(String),
    Rename { old: String, new: String },
    Reparent { actor: String, parent: Option<String> },
    ToggleVisibility(String), ToggleLock(String),
}

// commands/keyframe.rs — undoable
pub enum KeyframeCommand {
    SetEasing { actor, property, time, easing },
    Delete { actor, property, time },
    Move { actor, property, old_time, new_time },
}

// commands/scene.rs — undoable
pub enum SceneCommand {
    Select(String), Reorder(Vec<String>),
    SetTransition { from, transition },
    Duplicate(String), Delete(String),
}

// commands/view.rs — NOT undoable
pub enum ViewAction {
    TogglePanel(WorkspaceTab),
    SetZoom(f32), SetPan(Vec2),
    SetTool(ToolMode), SetSidebarTab(SidebarTab),
}

// commands/playback.rs — NOT undoable
pub enum PlaybackAction {
    Toggle, Scrub(f64), StepForward, StepBackward,
    PrevKeyframe, NextKeyframe,
}
```

### 7.4 Interaction Constraints

1. All canvas interactions go through `Gesture -> Command`. No direct store
   mutation from drag handlers.
2. Undoable and non-undoable commands are separate types — the undo stack
   only accepts undoable commands.
3. Every command carries `timestamp()` and `description()` for the undo
   history UI.
4. Text input focus disables global shortcuts (Space, arrows).
5. Focus ring must be visible: 2px `accent::PRIMARY` outline.

---

## 8. Motion Language

### 8.1 Duration Scale

| Token | Duration | Usage |
|-------|----------|-------|
| `INSTANT` | 0ms | State toggles (select/deselect) |
| `FAST` | 100ms | Hover/press feedback |
| `NORMAL` | 200ms | Panel expand/collapse, toast appear |
| `SLOW` | 400ms | View transition, welcome screen |

### 8.2 Easing Functions

| Token | Curve | Usage |
|-------|-------|-------|
| `STANDARD` | cubic-bezier(0.4, 0, 0.2, 1) | Default |
| `DECELERATE` | cubic-bezier(0, 0, 0.2, 1) | Element enter |
| `ACCELERATE` | cubic-bezier(0.4, 0, 1, 1) | Element exit |
| `SPRING` | slight overshoot | Drag release snap-back |

### 8.3 Motion Constraints

1. No scattered `animate_value_with_time` calls. Use a unified
   `anim::transition(id, duration, easing) -> f32` helper.
2. Playhead scrubbing updates are instant (0ms) — no animation lag.
3. Panel transitions: `NORMAL` duration + `STANDARD` easing.
4. Toast: `FAST` in, `NORMAL` out.
5. Animations exceeding `SLOW` must have explicit justification in a comment.
6. `prefers-reduced-motion`: all non-essential animations stop (Phase 2).

---

## 9. Layout System

### 9.1 Panel Size Constraints

| Panel | Min Width | Default | Max Width |
|-------|-----------|---------|-----------|
| Sidebar | 180px | 240px | 360px |
| Editor | 300px | 480px | ∞ |
| Preview | 320px | ∞ | ∞ |
| Inspector | 220px | 300px | 480px |
| Timeline | 400px (w) | 200px (h) | ∞ |

### 9.2 Workspace Presets

| Preset | Layout |
|--------|--------|
| Animate | Sidebar \| Preview(60%) + Editor(40%) / Timeline |
| Code | Sidebar \| Editor(70%) + Preview(30%) / Timeline |
| Inspect | Sidebar \| Preview(50%) + Inspector(50%) / Timeline |
| Focus | Preview only (fullscreen canvas) |

### 9.3 Layout Constraints

1. Panels resist being dragged below min width — they snap-hide instead.
2. Workspace presets are persistable and restorable.
3. Focus mode (`F11`) hides all panels; other panels slide in as overlays.
4. `egui_tiles::Tree` remains the docking engine.

---

## 10. Accessibility Constraints

### 10.1 Contrast

All text/background pairs must meet WCAG AA (4.5:1).

Current violations to fix:

| Pair | Current Ratio | Fix |
|------|--------------|-----|
| `TEXT_MUTED` on `BG_BASE` | 3.2:1 | Darken BG or lighten TEXT_MUTED |
| `TEXT_DISABLED` on `BG_WIDGET` | 1.8:1 | Only used for truly disabled elements |

### 10.2 Minimum Touch Targets

All interactive elements >= 24x24px (`ROW_M` x `ROW_M`).

### 10.3 Color is Not the Sole Signal

Status indicators must use color + icon + text (triple encoding). The
current "stale" / "last good" badges already do this — keep the pattern.

### 10.4 Keyboard Parity

Every mouse operation has an equivalent keyboard path. No mouse-only
interactions.

---

## 11. Migration Plan

Four phases, ordered by risk-to-reward ratio. Each phase is independently
shippable and does not break the build.

### Phase 1: Token Refoundation (Low Risk, High Reward)

**Goal**: Replace flat `design_tokens.rs` with the 3-layer module system.

**Files created**:
- `design_tokens/mod.rs` (re-exports)
- `design_tokens/primitive.rs` (`pub(crate)`)
- `design_tokens/semantic.rs` (surface, text, accent, status, category,
  border, canvas submodules)
- `design_tokens/typography.rs` (`TextRole` enum)
- `design_tokens/spatial.rs` (unified `SPACE_*` scale, delete `PAD_*`)
- `design_tokens/motion.rs` (duration + easing constants)
- `design_tokens/util.rs` (`lerp_color`, `multiply_alpha` — legacy)

**Files modified**:
- Every file currently importing `use crate::app::design_tokens::*;`
  (~50+ files) — migrate to `use ...::semantic::*;`
- `app/mod.rs` — update `pub mod design_tokens;` to point at new module

**Migration strategy**:
1. Create new module tree alongside old `design_tokens.rs`.
2. Add a compatibility re-export in `design_tokens/mod.rs` that aliases
   old constant names to new semantic paths:
   ```rust
   // Legacy compat — remove after all call sites migrated
   pub use semantic::surface::BASE as BG_BASE;
   pub use semantic::surface::PANEL as BG_PANEL;
   // ...
   ```
3. Migrate call sites file-by-file, running `cargo check` after each.
4. Delete compatibility aliases once all sites use semantic paths.
5. Delete old `design_tokens.rs`.

**Verification**: `cargo check` + `cargo test -p animatix-gui` after each
file migration. Visual smoke test of GUI after phase complete.

**Estimated scope**: ~50 files touched, mostly mechanical find-replace.

---

### Phase 2: Component Unification (Medium Risk)

**Goal**: Replace ad-hoc button functions with unified `Button` widget.
Migrate typography to `TextRole`.

**Files created**:
- `components/button.rs` — rewrite with `Button` struct + `egui::Widget`
- `components/text.rs` — `TextRole` helper methods

**Files modified**:
- All call sites of `icon_button`, `icon_button_colored`,
  `toolbar_toggle_button`, `toolbar_action_button`
- All call sites using raw `FontId::new(size, ...)` → `TextRole`
- `components/layout.rs` — update `section_header` to drop `to_uppercase()`
- `shell/toolbar.rs` — migrate to new `Button` API

**Migration strategy**:
1. Implement new `Button` widget alongside old functions.
2. Migrate toolbar first (highest concentration of button calls).
3. Migrate remaining call sites panel-by-panel.
4. Delete old button functions.
5. Migrate `FontId` usages to `TextRole` in a separate sweep.

**Verification**: `cargo check` + `cargo test` + visual smoke test.

---

### Phase 3: Command System Split (Medium Risk)

**Goal**: Split the 50+ variant `Command` enum into domain packages.
Separate undoable from non-undoable.

**Files created**:
- `commands/document.rs`
- `commands/actor.rs`
- `commands/keyframe.rs`
- `commands/scene.rs`
- `commands/view.rs`
- `commands/playback.rs`

**Files modified**:
- `commands.rs` → `commands/mod.rs` (re-exports + `ShellAction` wrapper)
- `command_handlers.rs` (804 lines) — split into domain handlers
- `command_bus.rs` — update dispatch logic
- All files matching on `Command::` variants

**Migration strategy**:
1. Create new domain command modules.
2. Add `From<DomainCommand> for Command` compatibility impls.
3. Migrate handlers one domain at a time (document → actor → keyframe →
   scene → view → playback).
4. Migrate call sites (panels that push commands).
5. Delete old flat `Command` enum.

**Verification**: `cargo check` + `cargo test` + undo/redo smoke test.

---

### Phase 4: Interaction Layer Upgrade (Higher Risk)

**Goal**: Introduce gesture system, keyboard navigation framework, and
unified motion API.

**Files created**:
- `preview/gesture.rs` — `Gesture` enum + `GestureHandler` trait
- `preview/gesture_router.rs` — routes raw egui events to gesture handlers
- `interaction/keyboard.rs` — focus management + keyboard shortcut registry
- `design_tokens/motion.rs` — `anim::transition()` helper

**Files modified**:
- `preview/drag_handler.rs` (763 lines) — gradually replaced by gesture
  handlers
- `preview/mod.rs` — integrate gesture router
- `runtime.rs` — keyboard shortcut dispatch via new framework
- All files using `animate_value_with_time` — migrate to `anim::transition`

**Migration strategy**:
1. Implement gesture types + router (no behavior change yet).
2. Migrate canvas drag interactions to gesture handlers, one drag mode
   at a time (move → resize → rotate → multi-select).
3. Implement keyboard focus ring + navigation.
4. Migrate scattered animations to unified `anim::transition`.
5. Delete old `drag_handler.rs` once fully replaced.

**Verification**: `cargo check` + `cargo test` + extensive manual testing
of all canvas interactions.

---

### Phase Summary

| Phase | Risk | Files Touched | Key Deliverable |
|-------|------|---------------|-----------------|
| 1. Token Refoundation | Low | ~50 | 3-layer token module system |
| 2. Component Unification | Medium | ~30 | Unified `Button` widget + `TextRole` |
| 3. Command System Split | Medium | ~20 | Domain command packages |
| 4. Interaction Upgrade | Higher | ~15 | Gesture system + keyboard nav |

Each phase can land independently. Phases 1 and 2 can partially overlap
(token migration in one file can include both token + typography updates).
