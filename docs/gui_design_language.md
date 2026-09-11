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
   - 7.7 Unified Interaction Grammar
8. [Motion Language](#8-motion-language)
9. [Layout System](#9-layout-system)
10. [Accessibility Constraints](#10-accessibility-constraints)
11. [Status & Remaining Work](#11-status--remaining-work)
12. [UX Audit & Redesign](#12-ux-audit--redesign)

---

## 1. Design Philosophy

Seven non-negotiable principles that govern every GUI decision.

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

### P6. State is Visible

Every mode that changes what input does (tool, auto-key/record, snap,
transform context) has a persistent, labeled indicator and is toggleable
without a keyboard. No invisible modes. The recurring failure mode this
guards against is state the user cannot see or change — e.g. auto-key
that silently writes keyframes, or a scene inspector reached only by
deselecting everything.

### P7. One Grammar

The same physical input means the same thing on the canvas, the timeline,
and in lists. Modifiers are assigned globally (see §7.7), never per
surface. "Shift" must not mean add-to-selection on one surface and
disable-snapping on another.

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

### 3.4 Font Loading and CJK Fallback

egui's default fonts do not contain CJK glyphs. At startup the GUI scans the
system font collection for faces that cover representative non-Latin glyphs
(Han, Hiragana, Hangul, Cyrillic, Greek, Arabic, Hebrew, Devanagari, Thai) and
registers those faces as fallbacks for both proportional and monospace
families. Font discovery is cached for the application lifetime and shared with
the font-family picker. Font setup is centralized in
`crates/animatix-gui/src/fonts.rs` and shared by the main app, review console,
and screenshot harness.

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
  MUTED      #676B7B   — muted
  DISABLED   #B4B9C0   — disabled
  ON_ACCENT  #0A0C10   — text on accent fills

Core text roles (`primary`, `secondary`, `muted`) are verified against WCAG
AA (4.5:1) on `base`, `panel`, `surface`, and `widget`. Disabled text is
intentionally exempt, and active accent-button text uses the WCAG UI
threshold (3:1).

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

**Variants** (4):

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
  .danger    — ButtonStateSlots  (backed by `ButtonVariant::Danger` / `Button::danger`)

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

### 7.7 Unified Interaction Grammar

One table, applied identically on the canvas, the timeline, and in lists
(P7). Each modifier has exactly one job; a behaviour that used to be a
hidden modifier becomes a menu item or a visible tool instead.

| Input | Meaning everywhere |
|---|---|
| Click | Select (replace) |
| Shift+Click | Add / toggle selection |
| Cmd/Ctrl+Click | Add / toggle selection (mac parity) |
| Drag on empty | Marquee / range select (overlap, not centre-containment) |
| Drag on body | Move; **if unselected, select it first** |
| Shift+Drag | Constrain: axis lock / uniform scale / angle snap |
| Alt+Drag | Bypass snapping (only) |
| Cmd/Ctrl+Drag | Duplicate and move |
| Double-click | Enter context: text edit / rename in list / group isolation |
| Right-click | Context menu: full command menu, keyboard-navigable, non-destructive dismiss |
| Esc | Cancel in order: drag → open menu → active tool → selection |
| Delete | Delete the focused pane's selection (visible focus ring), never by hover |

Two consequences worth stating explicitly because they reverse shipped
behaviour: `Alt` no longer duplicates (it bypasses snapping), and
`Shift` no longer un-snaps or detaches. Structural "detach from layout" /
"detach callout" move to explicit context-menu actions, since they change
the document's structure rather than a single gesture.

**Auto-key is explicit.** `keyframe_mode` defaults **off**. The toolbar
record toggle and the per-property keyframe diamond are the only ways to
write a keyframe; the diamond works regardless of the toggle. See §12.

**Tool keys.** Six tool modes, six keys: `V` select, `A` vertex,
`R` rotate, `S` scale, `G` move/grab, `P` pivot. `V`/`A` follow the
Adobe select/direct-select convention; the rest are mnemonics. These are
secondary to the visible tool switcher (P6), which is still pending.

**Frame stepping.** `Shift+,` / `Shift+.` step one frame back/forward;
bare `,` / `.` remain prev/next keyframe.

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

### 9.1 Region Size Constraints

Sizes are **proportion-first with pixel bounds**: a region is allocated as
`clamp(ratio × available, min, max)`. Shares are relative, so proportions hold
as the window resizes; a per-frame pass clamps each region back into its pixel
bounds so a small window cannot scale panels below their floors (§9.3). The
right column is one region with two tabs (Inspector | Code) and the bottom band
is one region with two tabs (Timeline | Curves), so their bounds depend on the
active tab.

| Region | Ratio | Min | Max | Default at 1440px |
|--------|-------|-----|-----|-------------------|
| Sidebar | 0.16 W | 200 | 360 | 230 |
| Preview | remainder | 360 | ∞ | 908 |
| Right column · Inspector | 0.21 W | 260 | 420 | 302 |
| Right column · Code | 0.38 W | 420 | 720 | 547 |
| Bottom band · Timeline | 0.25 H | 180 | 420 | 227 |
| Bottom band · Curves | 0.25 H | 180 | 420 | 227 |
| Toolbar | fixed | — | — | 28 |
| Status bar | fixed | — | — | 22 |

Proportions are the default allocation; a user drag stores an absolute size and
is preserved as long as it stays inside `[min, max]`. `Reset layout` restores
the proportions.

### 9.2 Workspace Presets

A preset adjusts proportions and the active detail tab on the existing tree — it
never rebuilds, so a user's custom arrangement survives. `Focus` also hides the
surrounding regions.

| Preset | Sidebar | Right column | Bottom band |
|--------|---------|--------------|-------------|
| Animate (default) | 0.16 W | Inspector | 0.25 H |
| Code | 0.14 W | **Code** | 0.18 H |
| Inspect | 0.12 W | Inspector (wider, 300–460) | 0.30 H |
| Focus | hidden | hidden | hidden |

Presets set the active tab in each group but preserve a user's choice when the
group still has one; a layout persisted before the Curves pane (or before the
detail group) makes `apply_layout_preset` return `false`, and the caller
rebuilds from `build_tree_for` — that is the migration path.

### 9.3 Layout Constraints

1. `clamp(ratio, min, max)` is the allocation rule; pixel bounds are the
   floor/ceiling that keeps extreme window sizes usable.
2. `egui_tiles::Behavior::min_size` adds a 120px floor to every tile so no pane
   collapses into a sliver.
3. The Inspector and the code editor share the right tab group and are mutually
   exclusive; `Cmd+Shift+I` / `Cmd+Shift+E` show each, toggling the region off
   when the same tab is already active.
4. The Timeline and the interactive Curves editor share the bottom tab group.
   Timeline is the default tab; `ShowCurves` / `ShowTimeline` (toolbar,
   command palette) switch between them. The Curves pane is absent from layouts
   persisted before it, which the preset-apply path migrates by rebuilding.
5. Focus mode hides the sidebar, detail column and bottom band; the preview
   fills the window.
6. Presets are applied in place (proportions + active tab) and are not
   destructive; `Reset layout` is the only action that rebuilds the tree.
7. `egui_tiles::Tree` remains the docking engine.

**Not yet implemented:** the narrow-window downgrade modes (collapsing the
sidebar to an icon rail and demoting the right column to an overlay drawer below
their breakpoints); today the pixel floors are the only degradation.

---

## 10. Accessibility Constraints

### 10.1 Contrast

Non-disabled body text must meet WCAG AA (4.5:1). Large text and active UI
components use the WCAG 3:1 threshold; disabled text is exempt while still
clearly de-emphasized.

Current dark-theme gap to fix:

| Pair | Current Ratio | Fix |
|------|--------------|-----|
| `TEXT_MUTED` on `BG_BASE` | 3.2:1 | Darken BG or lighten TEXT_MUTED |
| `TEXT_DISABLED` on `BG_WIDGET` | 1.8:1 | Exempt for disabled elements; kept clearly de-emphasized |

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

Phases 1–3 of the original migration plan are complete. See `docs/roadmap.md`
for the remaining eparts widget-adoption backlog and `crates/animatix-gui/src/app/commands/`
for the command-split implementation.

The 2026-09-11 UX pass is documented in §12 (diagnosis, target information
architecture, per-surface redesign, phased plan). Its Phase 0 batch is
implemented on `feat/gui-redesign`; Phases 1–4 remain.

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

**Visual verification:**
- Run `bash scripts/gui-screenshots.sh` to render dark/light PNGs for the eparts
  component overview, buttons, rows, cards, section headers, fields, empty
  states, and theme palette into `target/gui-screenshots/`.
- The underlying binary is bounded: it captures one screenshot and exits, and
  the script wraps it with `timeout 45s`. Use `xvfb-run` when no display is
  available.

**Remaining / verify:**
- Opportunistic eparts widget adoption is not scheduled; remaining call sites migrate as surrounding GUI files are next edited.

**Completed since the original audit:**
- Light-theme contrast is verified by eparts tests for the critical text/surface and accent-button pairs.
- Alert/Badge/Tooltip eparts widgets are adopted at natural GUI call sites.
- All custom GUI panels read `eparts::theme(ui)` or theme-aware egui visuals; static generic color constants are no longer used by render paths.
- Raw `FontId`/numeric `.size()` bypasses were replaced with `TextRole`.
- Restored a bounded `dev-screenshots` harness with `widget-screenshot` and `scripts/gui-screenshots.sh`.
- Inspector property fields and Settings migrated to eparts `Form`/`Field` and input widgets.
- Toolbar shortcut hints and the shortcut cheat sheet derive from `SHORTCUT_REGISTRY`.
- `ButtonVariant::Danger` is exposed and themed.
- Gesture router covers move/scale/rotate/pivot/reorder/marquee/vertex/motion_path; legacy `drag_handler.rs` is retired.

---

## 12. UX Audit & Redesign

A 2026-09-11 usability pass reviewed the shipped interaction design and
produced the redesign below. This section is the normative record: §12.4
lists what shipped, the rest describes the target state.

### 12.1 Systemic diagnosis

The gap was not missing features but **invisible state** and half-built
loops:

1. **Modes were invisible and un-toggleable.** `keyframe_mode` defaulted
   on with no writer anywhere else in the crate, so every property edit
   silently keyed a keyframe. Tool modes were keyboard-only. The scene
   inspector appeared when the selection was empty, unlabeled. Snapping
   was on with no toggle.
2. **Affordances lied.** A multi-selection drew a group bounding box with
   eight handles, but scale/rotate/pivot all acted on
   `selected_actors.iter().next()` — an arbitrary `HashSet` element.
   Dragging an unselected actor started a marquee. The pivot crosshair was
   always drawn and won the gesture-priority race at an actor's centre.
3. **Core loops were unreachable.** Rename existed only in the inspector
   header; no UI created a scene or a `play` edge; timeline property lanes
   appeared only once keyframes existed; editor completion appended to
   end-of-document; the inspector toggle rebuilt the whole dock tree.

### 12.2 Target information architecture

```
┌───────────┬──────────────────────────┬─────────────┐
│  Outline  │   Preview                │  Inspector  │
│  (Scenes  │   ┌─ tool switcher ─┐    │ (contextual,│
│   + Actor │   │ V A R S G  ● rec │    │  always on) │
│   tree)   │   └─────────────────┘    │             │
│           │        canvas            │  Actor |    │
│  ⌄ Project│                          │  Scene |    │
│  ⌄ Library│                          │  Multi      │
├───────────┴──────────────────────────┴─────────────┤
│  Timeline   [ Dope sheet | Curves ]   transport     │
└─────────────────────────────────────────────────────┘
```

1. The **Inspector is always visible** and headed by what it edits
   (`Actor: rect1` / `Scene: intro` / `3 actors`). Toggling panel
   visibility must not rebuild the dock tree.
2. The **sidebar's six tabs collapse to three** switchable views
   (`Cmd+1/2/3`): Project (files + assets), Outline (scenes + actors in one
   hierarchy), Library (components/snippets/actions). The **code editor is
   promoted out of the sidebar** to a real pane.
3. The **preview owns its tools**: a persistent tool switcher and record
   indicator live in the preview header, not a global toolbar. The global
   toolbar slims to app menu, document, palettes, settings, layout.
4. The **timeline gains tabs**: Dope sheet and Curves (the inspector's
   read-only graph editor, made interactive and moved here).
5. **Layout presets, reset layout, and focus mode** are wired (the §9.2
   presets are specified but unimplemented).

### 12.3 Per-surface redesign

| Surface | Changes |
|---|---|
| Canvas | Select-on-mousedown and a real primary selection; working group transform; pivot demoted out of Select mode; numeric entry in the drag HUD and property popup; arrange/align toolbar for multi-selection; full context menu (keyboard-navigable, non-destructive dismiss); guide management; double-click group isolation |
| Timeline | Property lanes always addable; property-granular selection and multi-keyframe drag; draggable playhead; editable action blocks (translate/add/delete/duplicate); inline easing glyph; snap toggle with magnet state; per-track hide/lock/solo; frame ruler at high zoom |
| Layers / Outline | Rename (`F2` / double-click); drag-to-reorder siblings; z-order actions; Group/Ungroup in the context menu; action-identified menu dispatch; descendant-safe reparent with search; type picker at creation |
| Inspector | Explicit context header; property search/filter; one stopwatch behaviour across all surfaces; multi-select property editing; reset / remove-animation per property |
| Editor | Completion at the caret, auto-triggered after `.`; `inline_edit_active` gates shortcuts; find/replace case/word/regex with match highlighting |
| Shell | App menu with New/Open/Recent/Save As; dirty window title; autosave + crash recovery; command palette as a generated superset with fuzzy search; settings restore-defaults and full key rebinding; export browse/overwrite/error detail; more session state persisted |

### 12.4 Phased plan & status

Phase 0 shipped on `feat/gui-redesign` (2026-09-11):

| Item | Change |
|---|---|
| Auto-key | Defaults off; toolbar record toggle with red icon; inspector diamond keys explicitly regardless of the toggle |
| Keyframe model | Inspector / canvas popup / spreadsheet share one model: click toggles a key at the playhead, right-click edits easing/removes |
| Coincident keys | The aggregate diamond carries every property keyed at that time; easing/delete/move apply to all of them |
| Completion | Caret-anchored via a live caret tracked by the cell renderer; splices in place instead of appending to EOF; auto-triggers after `.`; filtered selection index fixed |
| Scene inspector | Header reads `Scene: <name>` with a note explaining why it appeared |
| Shortcuts | Platform-aware display in Settings; `Shift+,` / `Shift+.` frame step |
| Delete scope | Timeline focus is click-latched with a visible focus ring, not hover |
| Inspector toggle | Flips pane visibility in place; the dock layout survives |
| Layers menu | Action-identified dispatch replaces the `_ => Delete` index fallback |
| Tool keys | `V` select, `A` vertex, `R` rotate, `S` scale, `G` move, `P` pivot |
| Group transform | Group scale (union-box handles, per-actor size vs scale mode) and group rotate (union centre) implemented |

Layout (2026-09-11, verified from workspace screenshots):

| Item | Change |
|---|---|
| Detail region | Inspector and Code share one right-hand tab group, visible by default with the Inspector active (`Cmd+Shift+I` / `Cmd+Shift+E`) |
| Responsive sizing | `clamp(ratio × available, min, max)` allocation plus a per-frame pixel-bound pass and a 120px tile floor (§9.1–9.3) |
| Presets | Animate / Code / Inspect / Focus applied in place, plus Reset layout |
| Sidebar | Merged 6 → 3 labeled tabs: Project (Files/Assets), Outline (Layers/Scenes), Library (Components) |
| Editor home | Promoted out of the sidebar into the detail region; the dead sidebar Editor tab/renderer removed |
| pill_tab_bar | Degrades label-first (icon+label → label → icon) and adds hover tooltips, so merged tabs stay legible at the 200px floor |

Remaining, in order:

1. **Phase 1 — visible state & canvas.** Tool switcher UI in the preview
   header, pivot demotion, snap toggle, select-on-mousedown, canvas context
   menu, numeric entry, timeline playhead drag and property-granular keyframe
   editing. Also move the transport to a global bar so playback survives a
   bottom-tab switch.
2. **Phase 2 — core loops.** Layer outliner editing; add scene / create a
   `play` edge from the UI; interactive Curves tab; editor find/replace
   options.
3. **Phase 3 — information architecture (partly done).** Sidebar merge,
   detail tab group, editor placement and presets have shipped; still open are
   drag-to-place from the Library and the narrow-window downgrade modes (icon
   rail / overlay drawer).
4. **Phase 4 — platform conventions.** App menu (New/Open/Recent/Save As),
   autosave + recovery, command palette superset, export/settings polish;
   diagnostics as a status-bar peek instead of a stacked bottom panel.

### 12.5 Open decisions

| Decision | Status |
|---|---|
| Auto-key default | **Resolved:** off. |
| Group transform | **Resolved:** implemented. |
| Tool shortcut letters | **Resolved:** `V/A/R/S/G/P`. |
| Editor placement | **Resolved:** shares the right-hand detail tab group with the Inspector (mutually exclusive). |
| Outline shape | **Resolved:** one labeled Outline tab with a Layers / Scenes section switcher. |
| Transport placement | **Open.** Recommendation: promote to a global bar so playback survives switching the bottom tab to Code/Curves; currently it still lives in the timeline panel. |
