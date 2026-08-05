# Animatix GUI Design Language

> Authoritative specification for the visual design language, token system,
> component taxonomy, interaction model, and remaining work for the Animatix GUI.
>
> **Status**: Active — `eparts` crate is the shipped component library.
> The shipped `eparts` code is the source of truth; this document is kept in sync with it.

---

## Table of Contents

1. [Design Philosophy](#1-design-philosophy)
2. [Token System](#2-token-system)
3. [Typography](#3-typography)
4. [Spatial System](#4-spatial-system)
5. [Color System](#5-color-system)
   - 5.1 Surface Depth (6 levels)
   - 5.2 Light Theme Values
   - 5.3 Semantic Color Roles
   - 5.4 Color Constraints
   - 5.5 IDE Token Group
6. [Component Taxonomy](#6-component-taxonomy)
   - 6.1 Three Layers
   - 6.2 Unified Button API
   - 6.3 Component Constraints
   - 6.4 Component Theme Slots
7. [Interaction Language](#7-interaction-language)
   - 7.1 Gesture System
   - 7.2 Keyboard Navigation
   - 7.3 Command System
   - 7.4 Interaction Constraints (focus ring, cursor convention)
   - 7.5 Iconography
   - 7.6 Overlay Layering
8. [Motion Language](#8-motion-language)
9. [Layout System](#9-layout-system)
10. [Accessibility Constraints](#10-accessibility-constraints)
11. [Status & Remaining Work](#11-status--remaining-work)

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
                       Visibility: pub (crate-public) — eparts is a library; the
                       consuming app's semantic submodules reference primitives
                       directly via `eparts::tokens::primitive`
                        
Layer 2: Semantic    — role-based names mapped from primitives
                       Visibility: pub — the public API consumed by widget code
                        
Layer 3: Component   — per-component token slots (Theme struct)
                       Visibility: pub — lives alongside the component module
```

**Layering invariant.** Widget code imports semantic roles (e.g.
`semantic::surface::WIDGET`) or reads the runtime `Theme` struct; it never
imports `primitive::*` directly. This invariant is enforced by **convention
+ code review**, not by `pub(crate)` — because eparts is a published library
and the consuming app's own semantic submodules (category, diagnostic,
curve, editor, timeline, canvas) legitimately reference raw palette entries
through `eparts::tokens::primitive` as their upstream source.


### 2.2 Rust Module Layout

The token system uses Rust's module system for grouping and visibility control.

```rust
// crates/eparts/src/tokens/mod.rs

/// Primitive token values — pub so app-specific semantic submodules
/// in the consuming crate can reference raw palette entries.
pub mod primitive;

/// Semantic tokens — the public API consumed by widget code.
pub mod semantic;  // surface (6 levels) + text, accent, status, border, lines, overlay

/// Utility functions (lerp, alpha-multiply).
pub mod util;
```

```rust
// crates/animatix-gui/src/app/design_tokens/semantic.rs (consuming app)
// App-specific submodules access raw palette entries via super::primitive.
use super::primitive as p;

pub mod surface {
    pub const BASE: Color32 = p::GRAY_950;
    // ...
}
```

```rust
// crates/eparts/src/tokens/primitive.rs

use egui::Color32;

// All entries are `pub` — app semantic submodules reference them directly.
pub const GRAY_950: Color32 = Color32::from_rgb(10, 12, 16);
pub const GRAY_900: Color32 = Color32::from_rgb(16, 18, 23);
pub const GRAY_600: Color32 = Color32::from_rgb(60, 66, 78);
// ... full palette in primitive.rs
```

### 2.3 Token Constraints

| Rule | Enforcement |
|------|-------------|
| UI/widget code must not import `primitive` directly | Convention + code review — widgets read `theme(ui)` slots or `semantic::*`, never `primitive::*` |
| No runtime `linear_multiply` for alpha | Pre-computed `const` values only |
| Status and Category colors must not be shared | Separate modules, separate types |
| Every semantic color must exist in both dark and light themes | Runtime `Theme` swap; both `Theme::dark()` and `Theme::light()` define all slots |
| Component-level tokens live in the `Theme` struct, not in `design_tokens/` | Convention + review; `tokens/theme.rs` is the source of truth |
| Raw color literals (`Color32::from_rgb`, `from_gray`, etc.) | Allowed **only** inside `tokens/primitive.rs` and `tokens/theme.rs`; never in widget code |

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

### 5.1 Surface Depth (6 levels)

```
Depth 0  BASE       #0A0C10   GRAY_950 — window background
Depth 1  PANEL      #101217   GRAY_900 — panel background
Depth 2  SURFACE    #16191F   GRAY_850 — cards, floating surfaces
Depth 3  WIDGET     #1E222A   GRAY_800 — inputs, buttons
Depth 4  HOVER      #2A2F39   GRAY_700 — hover overlay
Depth 5  ACTIVE     #3C424E   GRAY_600 — pressed / active
```

Adjacent layers must differ by >= 6% luminance. These values are the
current shipped palette (`crates/eparts/src/tokens/primitive.rs`).

### 5.2 Light Theme Values

`Theme::light()` (shipped — see `crates/eparts/src/tokens/theme.rs`). Accent
and status hues are identical to dark for brand consistency; surfaces are
near-white with dark text.

```
Surface:
  BASE       #F8F9FA   — window background
  PANEL      #FFFFFF   — panel background
  SURFACE    #FAFBFC   — cards, floating surfaces
  WIDGET     #F0F1F3   — inputs, buttons
  HOVER      #E3E5E9   — hover overlay
  ACTIVE     #D2D4D8   — pressed / active

Text:
  PRIMARY    #14181E   — primary text (dark on light)
  SECONDARY  #5A6170   — secondary
  MUTED      #828A99   — muted
  DISABLED   #B4B9C0   — disabled
  ON_ACCENT  #FFFFFF   — text on accent fills

Border:
  DEFAULT    #C8CCD1
  STRONG     #A0A5AC
  FOCUS      #546EFF   (same as dark — brand accent)

Overlay:
  backdrop()    rgba(0,0,0,140)
  badge_bg()    rgba(248,249,250,235)
  tooltip_bg()  rgba(255,255,255,245)
```

### 5.3 Semantic Color Roles

```
Accent (dark and light identical):
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

Lines (neutral grid / guide separators — `eparts::tokens::semantic::lines`):
  lines::grid_line()    rgba(255,255,255,12)  — light grid line on dark canvas
  lines::guide_line()   rgba(255,255,255,30)  — reference / snap guide line

Overlay (backdrops / scrims — `eparts::tokens::semantic::overlay`):
  overlay::backdrop()        rgba(0,0,0,120)       — panel-level dimming scrim
  overlay::badge_bg()        rgba(10,12,16,220)    — floating badge background
  overlay::tooltip_bg()      rgba(10,12,16,235)    — tooltip popup background
  overlay::shadow_ambient()  rgba(0,0,0,40)        — ambient shadow color
  overlay::shadow_direct()   rgba(0,0,0,60)        — direct shadow color
```

### 5.4 Color Constraints

1. **Status and Category colors are disjoint sets.** The current code reuses
   `GREEN` for both "success" and "style category" — this is forbidden.
2. Every semantic color has 5 states: default, hover, active, disabled,
   focused. Disabled = `text::DISABLED` color; focused = `accent::PRIMARY`
   outline.
3. Selection uses alpha-tinted accent (60/255), never solid fill.
4. Canvas colors live in `semantic::canvas` and do not share tokens with
   UI chrome.

### 5.5 IDE Token Group

Canvas-specific and IDE-specific tokens live in the app's
`design_tokens::semantic::canvas` submodule (defined in
`crates/animatix-gui/src/app/design_tokens/semantic.rs`). These are the
tokens that distinguish the IDE surface from generic UI chrome.

```
canvas::BG                    #08080C   — canvas viewport background
canvas::grid_line()           rgba(255,255,255,12)  — grid (re-export of lines::grid_line)
canvas::guide_line()          rgba(255,255,255,30)  — reference guide (re-export of lines::guide_line)
canvas::hatch_line()          rgba(255,255,255,30)  — hatch pattern overlay
canvas::ghost_prev()          rgba(80,220,120,77)   — ghost frame (prev)
canvas::ghost_next()          rgba(80,160,255,77)   — ghost frame (next)
canvas::snap_guide_line()     rgba(84,191,123,160)  — snap guide indicator
canvas::snap_guide_label_bg() rgba(30,30,35,200)    — snap guide label background
```

Selection marquees and transform handles use `semantic::accent::selection()`
(rgba(84,110,255,60)) for fills and `semantic::accent::PRIMARY` for
outlines — not separate canvas tokens.

The command palette uses `surface.overlay` / `overlay::backdrop()` as its
background scrim, consistent with dialog-level overlays.

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

The `Button` widget provides a lean, builder-based API. Per eparts principle 6
("4-tier size vocabulary, wire what you use"), only the variants and sizes that
have call sites are pre-built; additional variants/sizes are added when a call
site needs them, not speculatively.

**Variants** (3):

```
ButtonVariant::Primary   filled accent background — primary actions
ButtonVariant::Ghost     transparent, accent underline when active — toolbar toggles
ButtonVariant::Icon      square icon-only — small icon commands
ButtonVariant::Danger    destructive actions — delete, remove, reset
```

**Sizes** (1):

```
ButtonSize::Medium  (ROW_M height, default)
```

**Constructors** (free functions, not enum variants):

```rust
Button::primary(label: impl Into<String>) -> Self   // filled accent
Button::ghost(label: impl Into<String>)  -> Self    // transparent with underline
Button::icon(icon: &'static str)         -> Self    // icon-only square
Button::danger(label: impl Into<String>) -> Self    // destructive action
```

**Builder methods** (all return `Self`):

```rust
.with_icon(icon: &'static str)              // prepend an icon
.with_tooltip(tip: &'static str)            // egui hover tooltip
.active(active: bool)                        // pressed/toggled state (Ghost)
.icon_color(c: Color32)                      // override icon fg color
.hover_icon_color(c: Color32)                // override icon fg on hover
.loading(loading: bool)                      // show spinner, disable interaction
.on_hover(cb: Box<dyn FnOnce()>)             // callback on hover
```

**Disabled state.** There is no `disabled()` builder. A button is disabled
when the `disabled` field is set externally (e.g. via a form-level
disability gate). The loading state (`loading(true)`) disables interaction
and shows a spinner simultaneously.

**Policy note.** `Danger` is a shipped `ButtonVariant` backed by the
`theme.button.danger` slots (see §6.4). Destructive GUI actions use
`Button::danger(...)`.

```rust
// crates/eparts/src/widget/button.rs — source of truth

pub enum ButtonVariant { Primary, Ghost, Icon, Danger }
pub enum ButtonSize     { Medium }

impl Button {
    pub fn primary(label: impl Into<String>) -> Self { ... }
    pub fn ghost(label: impl Into<String>)  -> Self { ... }
    pub fn icon(icon: &'static str)         -> Self { ... }
    pub fn danger(label: impl Into<String>) -> Self { ... }
    pub fn with_icon(self, icon: &'static str)          -> Self { ... }
    pub fn with_tooltip(self, tip: &'static str)        -> Self { ... }
    pub fn active(self, active: bool)                    -> Self { ... }
    pub fn icon_color(self, c: Color32)                  -> Self { ... }
    pub fn hover_icon_color(self, c: Color32)            -> Self { ... }
    pub fn loading(self, loading: bool)                  -> Self { ... }
    pub fn on_hover(self, cb: Box<dyn FnOnce()>)         -> Self { ... }
}
```

### 6.3 Component Constraints

#### Tier-1 vs Tier-2 API contract

Per `crates/eparts/AGENTS.md` "Widget API contract", eparts widgets follow a
deliberate two-tier convention. The design doc's earlier statement
("Primitive components implement `egui::Widget` — no free functions") is
incorrect and is corrected here.

**Tier 1 — `impl egui::Widget`** (invoked `ui.add(MyWidget::new(...))`).
Use for self-contained widgets that take only plain values/builder options
and return an `egui::Response`. No content closures, no rich return struct.
Examples: `Button`, `Label`, `Spinner`, `Slider`, `Select`, `Badge`, `Tag`,
`Alert`, `ProgressBar`, `Skeleton`, `Kbd`.

**Tier 2 — `pub fn show(self, ui, ...) -> T`** (invoked
`MyWidget::new(...).show(ui, ...)`). Use when the widget needs any of: a
content/render closure (`FnOnce(&mut Ui)`), a rich return value (a `*Response`
action struct beyond `egui::Response`), or cross-frame state coordination.
Examples: `Form`/`Field`, `Dialog::modal`, `Popover`, `Tooltip`,
`Collapsible`, `Tree`, `List`, `ColorPicker`, `TextField`/`NumberField`,
`Row`, `TabBar`, `ResizeHandle`, `Toast`.

**Rules:**
- A widget exposes exactly **one** primary entry point — either `impl Widget`
  OR `show()`, never both. (Builder setters like `with_size`, `show_value`
  are fine; they are not entry points.)
- Tier-2 `show()` returns either `egui::Response` or a documented `*Response`
  struct; name rich structs `<Widget>Response`.
- Free functions in layout helpers (`card`, `section_header`, `separator`)
  are a deliberate exception: they are stateless layout helpers, not widgets.
- When unsure, prefer Tier 1; promote to Tier 2 only when a closure / rich
  return / state coordination is required.

**Exception — `Row::show_in_rect`.** `Row` exposes a second rect-mode entry
point (`show_in_rect`) that takes a pre-allocated rect + response + painter.
This is used by container widgets (`Tree`, `List`) that own allocation; it is
not a competing API — `show()` allocates and delegates to `show_in_rect`.
Both paths render the same `Row`.

#### Structural constraints

1. Primitive components implement `egui::Widget` (Tier-1) or `show()`
   (Tier-2) — never both.
2. Every component supports the interaction states relevant to its variant:
   `normal`, `hover`, `active` / `pressed`, `disabled`, and `focused` (focus
   ring). Not every state is meaningful for every variant (e.g. `Icon` has
   no `active` underline), but all five states are wired in the slot structs.
3. Pattern components compose Primitives — no direct `painter` calls.
4. Domain panels compose Patterns + Primitives — no direct `painter` calls
   for standard UI elements (canvas painting is exempt).
5. Component-level tokens live in the `Theme` struct (`tokens/theme.rs`),
   not in component modules as bare `const` values.

### 6.4 Component Theme Slots

The `Theme` struct (source of truth: `crates/eparts/src/tokens/theme.rs`)
exposes component-scoped color slot groups. Each group maps all interaction
states to `Slot { bg, fg, border }` or lighter types. The full taxonomy:

```
theme.button
  .primary   — ButtonStateSlots { normal, hover, active, selected, disabled, focus }
  .secondary — ButtonStateSlots  (seeded; no ButtonVariant::Secondary yet)
  .ghost     — ButtonStateSlots
  .icon      — ButtonStateSlots
  .danger    — ButtonStateSlots  (seeded from status::error*; no ButtonVariant::Danger yet — T2.10)

theme.list
  .even      — Fill { bg, fg }   zebra even row
  .odd       — Fill { bg, fg }   zebra odd row
  .selected  — Fill { bg, fg }   selected row
  .hover     — Fill { bg, fg }   hovered row

theme.tab
  .active    — TabSlot { bg, fg, indicator }   active tab with accent indicator stripe
  .inactive  — TabSlot { bg, fg, indicator }
  .hover     — TabSlot { bg, fg, indicator }

theme.menu_item
  .normal    — Slot { bg, fg, border }
  .hover     — Slot
  .active    — Slot
  .disabled  — Slot

theme.input
  .normal    — Slot { bg, fg, border }
  .hover     — Slot
  .focus     — Slot  (border = border.focus / accent)
  .invalid   — Slot  (border = status.error)
  .disabled  — Slot

theme.scrollbar
  .thumb       — Color32
  .thumb_hover — Color32
```

Access pattern:
```rust
let t = eparts::theme(ui);
let bg = t.button.primary.normal.bg;
let err_border = t.input.invalid.border;
```

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

### 7.3 Command System

The command enum is split into 6 domain modules under
`crates/animatix-gui/src/app/commands/`:

```
commands/document.rs  — Save, Reload, Rebuild, OpenFile, SwitchWorkspace
commands/actor.rs     — Create, Delete, Duplicate, Rename, Reparent, ToggleVisibility, ToggleLock
commands/keyframe.rs  — SetEasing, Delete, Move (undoable)
commands/scene.rs     — Select, Reorder, SetTransition, Duplicate, Delete (undoable)
commands/view.rs      — TogglePanel, SetZoom, SetPan, SetTool (NOT undoable)
commands/playback.rs  — Toggle, Scrub, StepForward, StepBackward, PrevKeyframe, NextKeyframe (NOT undoable)
```

Undoable and non-undoable commands are separate types; the undo stack
only accepts undoable commands.

### 7.4 Interaction Constraints

1. All canvas interactions go through `Gesture -> Command`. No direct store
   mutation from drag handlers.
2. Undoable and non-undoable commands are separate types — the undo stack
   only accepts undoable commands.
3. Every command carries `timestamp()` and `description()` for the undo
   history UI.
4. Text input focus disables global shortcuts (Space, arrows).
5. **Focus ring**: `STROKE_WIDTH` (2px) stroke in `theme.border.focus` /
   `focus_ring()`, painted **inset by 1px** (`rect.shrink(1.0)`,
   `StrokeKind::Inside`) to avoid clipping by the widget boundary. Every
   focusable primitive (Button, Input, Select, …) uses this identical
   treatment. See `crates/eparts/src/widget/button.rs` as the reference
   implementation.
6. **Cursor convention** (eparts principle 3): buttons and rows display the
   default arrow cursor on hover. `CursorIcon::PointingHand` is reserved
   exclusively for `Link` (genuine hyperlinks). egui's default for clickable
   widgets is `PointingHand`; eparts widgets override it with
   `.on_hover_cursor(egui::CursorIcon::Default)` to give a native-desktop
   feel rather than a web feel.

### 7.5 Iconography

Icons use `egui-phosphor` `regular` weight. Rules:

- **Default icon size**: `TextRole::Body` font size (13px) for inline and
  toolbar icons. Icons in `Icon` buttons are centered in the `ROW_M` (24px)
  slot using the same `Body` font.
- **Icon color follows the slot `fg`**: icons read `slot.fg` from the active
  `theme` slot; never a hardcoded `Color32`. Custom icon colors are only
  allowed via `Button::icon_color()` / `hover_icon_color()` builder methods.
- **Status icons always pair with text** (triple encoding — color + icon +
  text, per §10.3). A standalone status icon without a label is not
  accessible.

### 7.6 Overlay Layering

The managed overlay coordination layer (`crates/eparts/src/widget/overlay.rs`)
defines a priority ordering for floating overlays so that Escape and
outside-click dismissal are consumed by exactly the topmost one.

**Priority ladder (low → high):**

| Layer | `OverlayLayer` value | `egui::Order` | Typical use |
|---|---|---|---|
| `Dialog` | 0 (lowest) | `Order::Foreground` | Full-viewport modal dialogs |
| `Popover` | 1 | `Order::Foreground` | Dropdown menus, anchored popovers |
| `Tooltip` | 2 (highest) | `Order::Tooltip` | Transient hover tooltips |

`Dialog` and `Popover` share `Order::Foreground`; use a monotonically
increasing relative z within that plane (newer overlays paint above older
ones). `Tooltip` uses egui's `Order::Tooltip` which paints above
`Foreground`.

**Dismissal rules:**
- Escape is consumed only by the topmost overlay (`is_topmost(ctx, id)`);
  lower-priority overlays are not triggered.
- Outside-click (`clicked_outside`) checks whether the primary pointer click
  landed outside the overlay's `content_rect`.
- On close, call `overlay::remove_overlay(ctx, id)` so the next-topmost
  overlay resumes receiving dismissal events.

**Usage:**
```rust
overlay::push_overlay(ctx, egui::Id::new("my_dialog"), OverlayLayer::Dialog);
if overlay::is_topmost(ctx, my_id) && overlay::escape_pressed(ctx, my_id) {
    request_close();
}
// on close:
overlay::remove_overlay(ctx, egui::Id::new("my_dialog"));
```

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
| `SPRING_OVERSHOOT` | cubic-bezier(0.34, 1.56, 0.64, 1.0) | Drag release snap-back |

Note on `SPRING_OVERSHOOT`: the y1=1.56 control point produces a
perceptual overshoot. egui's `Ui`-level animation helpers clamp their
output to [0, 1], so the true spring bounce is only observable when the
value is applied via the raw `CubicBezier::sample()` method (which does
not clamp). A real physics spring would require a code-level spring
integrator in `anim.rs` (optional future follow-up).

### 8.3 Motion Constraints

1. No scattered `animate_value_with_time` calls. Use a unified
   `anim::transition(id, duration, easing) -> f32` helper.
2. Playhead scrubbing updates are instant (0ms) — no animation lag.
3. Panel transitions: `NORMAL` duration + `STANDARD` easing.
4. Toast: `FAST` in, `NORMAL` out.
5. Animations exceeding `SLOW` must have explicit justification in a comment.
6. `prefers-reduced-motion`: when the user enables the reduced-motion
   preference (Settings toggle; OS detection where available), all
   non-essential animation durations resolve to `INSTANT` (0ms). This is an
   accessibility requirement, not a deferred nicety.

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

## 11. Status & Remaining Work

Phases 1–3 of the original migration plan are complete. See
[`plans/eparts-refinement-roadmap.md`](plans/eparts-refinement-roadmap.md) (M1–M7) for the eparts component
work detail and `crates/animatix-gui/src/app/commands/` for the command-split
implementation.

**Completed:**
- Phase 1 (token refoundation): 3-layer token system extracted into `eparts`
  crate; `primitive`, `semantic`, `theme`, `spatial`, `typography`, `motion`
  modules all live in `crates/eparts/src/tokens/`.
- Phase 2 (component unification): `Button` (M3), `TextRole` typography (M5/M6),
  runtime `Theme` with dark + light (M2), component slot structs (B2/B3).
- Phase 3 (command split): 6 domain command modules shipped in
  `commands/{actor,document,keyframe,playback,scene,view}.rs`.
- Phase 4 (motion + keyboard + gesture types): motion token layer done;
  `preview/gesture.rs` and extracted drag-mode handlers in `preview/gestures/`
  are shipped.

**Remaining / verify:**
- Finish runtime-theme migration for inspector, timeline, preview overlays, export, insertion palette, and remaining sub-panels.
- Replace remaining raw font-size/`FontId` bypasses with `TextRole`.
- Restore a dev screenshot/visual-regression harness so theme and layout changes are visible to CI.
- Verify the light-theme contrast matrix against WCAG AA (4.5:1) for
  `Theme::light()` values.
- Opportunistic eparts widget adoption is not scheduled; migrate as surrounding GUI files are next edited.

**Completed since the original audit:**
- Inspector property fields and Settings migrated to eparts `Form`/`Field` and input widgets.
- Toolbar shortcut hints and the shortcut cheat sheet derive from `SHORTCUT_REGISTRY`.
- `ButtonVariant::Danger` is exposed and themed.
- Gesture router covers move/scale/rotate/pivot/reorder/marquee/vertex/motion_path; legacy `drag_handler.rs` is retired.
