use egui::Color32;

#[cfg(feature = "theme-json")]
use super::serde_color32;

// ── Component-scoped slot types (B2 + B3) ─────────────────────────

/// A fg/fill/border triple for a widget state slot.
///
/// Unused fields are set to `Color32::TRANSPARENT` (e.g. a ghost button's normal
/// background, or a slot that draws no border).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Slot {
    /// Background / fill color.
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub bg: Color32,
    /// Foreground / text / icon color.
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub fg: Color32,
    /// Outline / border stroke color.
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub border: Color32,
}

/// A bg+fg pair (no border) for list rows and similar slots.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Fill {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub bg: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub fg: Color32,
}

/// A tab slot with an accent indicator stripe.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct TabSlot {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub bg: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub fg: Color32,
    /// Bottom indicator stripe (active tab only); `TRANSPARENT` when none.
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub indicator: Color32,
}

/// All interaction states for one button variant
/// (mirrors the states handled in `widget/button.rs`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct ButtonStateSlots {
    pub normal: Slot,
    pub hover: Slot,
    pub active: Slot,
    pub selected: Slot,
    pub disabled: Slot,
    pub focus: Slot,
}

/// Component-scoped color slots for all button variants.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct ButtonSlots {
    pub primary: ButtonStateSlots,
    pub secondary: ButtonStateSlots,
    pub ghost: ButtonStateSlots,
    pub icon: ButtonStateSlots,
    /// Destructive-action buttons (delete, remove, …). Seeded from `status::error*`.
    pub danger: ButtonStateSlots,
}

/// List row color slots (even/odd zebra + selected + hover overlays).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct ListSlots {
    pub even: Fill,
    pub odd: Fill,
    pub selected: Fill,
    pub hover: Fill,
}

/// Tab bar color slots.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct TabSlots {
    pub active: TabSlot,
    pub inactive: TabSlot,
    pub hover: TabSlot,
}

/// Context-menu item color slots.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct MenuItemSlots {
    pub normal: Slot,
    pub hover: Slot,
    pub active: Slot,
    pub disabled: Slot,
}

/// Text-input / field color slots.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct InputSlots {
    pub normal: Slot,
    pub hover: Slot,
    pub focus: Slot,
    pub invalid: Slot,
    pub disabled: Slot,
}

/// Scrollbar thumb color slots.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct ScrollbarSlots {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub thumb: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub thumb_hover: Color32,
}

// ── Per-component slot groups (B) ─────────────────────────────────────
//
// These groups cover widgets that previously read palette/semantic colours
// directly. `Slot`'s `border` field is `TRANSPARENT` where the widget paints no
// outline.
//
// Deliberately NOT given a dedicated group (see the widget files for the
// inline rationale): `widget/diagnostics.rs` already maps cleanly onto
// `theme.components.list.*` + `theme.palette.status.*`, and a single-consumer group that only
// re-exported those shared colours would add indirection without theming value.

/// Interaction states for checkbox / radio / switch controls.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct ToggleSlots {
    /// Control background / border when the value is on.
    pub checked: Slot,
    /// Control background / border when the value is off.
    pub unchecked: Slot,
    /// Hovered (and enabled) appearance, used for the off state.
    pub hover: Slot,
    /// Disabled appearance, overriding checked/unchecked/hover.
    pub disabled: Slot,
    /// Switch knob fill.
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub thumb: Color32,
    /// Checkbox checkmark / radio dot fill.
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub mark: Color32,
}

/// Progress-bar colour slots.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct ProgressSlots {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub track: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub fill: Color32,
    /// Label colour over the unfilled track.
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub label: Color32,
    /// Label colour over the filled portion (for contrast against `fill`).
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub label_on_fill: Color32,
}

/// Tag / chip colour slots (normal + hover).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct TagSlots {
    pub normal: Slot,
    pub hover: Slot,
}

/// Per-level inline alert colours.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct AlertLevelSlots {
    /// Banner background.
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub bg: Color32,
    /// Title / body text colour.
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub fg: Color32,
    /// Left accent bar colour.
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub accent: Color32,
    /// Leading icon colour.
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub icon: Color32,
}

/// Alert colours for every level.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct AlertSlots {
    pub info: AlertLevelSlots,
    pub success: AlertLevelSlots,
    pub warning: AlertLevelSlots,
    pub error: AlertLevelSlots,
}

/// Per-level toast colours.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct ToastLevelSlots {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub bg: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub fg: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub border: Color32,
    /// Accent bar + icon colour.
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub accent: Color32,
}

/// Toast colours for every level.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct ToastSlots {
    pub info: ToastLevelSlots,
    pub success: ToastLevelSlots,
    pub warning: ToastLevelSlots,
    pub error: ToastLevelSlots,
}

/// Loading-skeleton colour slots.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct SkeletonSlots {
    /// Base block fill.
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub base: Color32,
    /// Pulsing highlight colour (painted at a fraction of its alpha).
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub shimmer: Color32,
}

/// Colour-picker swatch slots. The trigger frame reuses `theme.components.input.*`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct ColorPickerSlots {
    /// Preset-swatch outline.
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub swatch_border: Color32,
    /// Preset-swatch outline when hovered.
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub swatch_hover: Color32,
    /// Checkerboard light square (shows behind translucent colours).
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub checker_light: Color32,
    /// Checkerboard dark square.
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub checker_dark: Color32,
}

/// Easing-curve editor slot colours.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct EasingCurveSlots {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub bg: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub grid: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub curve: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub handle: Color32,
    /// Handle colour while its control point is being dragged.
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub handle_active: Color32,
}
