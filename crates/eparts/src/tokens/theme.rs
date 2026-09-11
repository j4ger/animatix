//! Runtime `Theme` struct + immediate-mode accessors.
//!
//! `Theme` mirrors `tokens::semantic` so that `Theme::dark()` is *by construction*
//! equal to the current compile-time consts.  The const modules stay intact;
//! this is purely additive and existing call sites do not need to change.
//!
//! Access pattern (per §3a Step 2 of the roadmap):
//! ```ignore
//! let t = eparts::theme(ui);
//! let bg = t.surface.base;
//! ```
//! Set once per frame (or on theme switch):
//! ```ignore
//! eparts::set_theme(ctx, Theme::light());
//! ```

use egui::{Color32, Context, CornerRadius, Shadow, Stroke, Visuals};

use crate::tokens::semantic;
use crate::tokens::spatial::{RADIUS_M, STROKE_WIDTH};

// ── Serde helpers (theme-json feature) ────────────────────────────────

#[cfg(feature = "theme-json")]
pub(crate) mod serde_color32 {
    use egui::Color32;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serialize colors as hex strings (`#rrggbb` or `#rrggbbaa`).
    pub fn serialize<S: Serializer>(color: &Color32, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&color.to_hex())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Color32, D::Error> {
        let text = String::deserialize(deserializer)?;
        Color32::from_hex(&text)
            .map_err(|e| serde::de::Error::custom(format!("invalid color hex {text:?}: {e:?}")))
    }
}

#[cfg(feature = "theme-json")]
pub(crate) mod serde_shadow {
    use egui::{Color32, Shadow};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde_json::Value;

    pub fn serialize<S: Serializer>(shadow: &Shadow, serializer: S) -> Result<S::Ok, S::Error> {
        let value = serde_json::json!({
            "offset": shadow.offset,
            "blur": shadow.blur,
            "spread": shadow.spread,
            "color": shadow.color.to_hex(),
        });
        value.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Shadow, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let offset = value
            .get("offset")
            .and_then(Value::as_array)
            .and_then(|items| Some([as_i8(&items[0])?, as_i8(&items[1])?]))
            .ok_or_else(|| serde::de::Error::custom("shadow offset must be [x, y]"))?;
        let blur = value.get("blur").and_then(Value::as_u64).and_then(|v| u8::try_from(v).ok());
        let spread = value.get("spread").and_then(Value::as_u64).and_then(|v| u8::try_from(v).ok());
        let color = value
            .get("color")
            .and_then(Value::as_str)
            .and_then(|text| Color32::from_hex(text).ok())
            .ok_or_else(|| serde::de::Error::custom("shadow color must be a hex string"))?;
        Ok(Shadow {
            offset,
            blur: blur.ok_or_else(|| serde::de::Error::custom("shadow blur must be a u8"))?,
            spread: spread.ok_or_else(|| serde::de::Error::custom("shadow spread must be a u8"))?,
            color,
        })
    }

    fn as_i8(value: &Value) -> Option<i8> {
        value.as_i64().and_then(|v| i8::try_from(v).ok())
    }
}

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
// `theme.list.*` + `theme.status.*`, and a single-consumer group that only
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

/// Colour-picker swatch slots. The trigger frame reuses `theme.input.*`.
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

// ── Nested slot structs ───────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Surface {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub base: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub panel: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub surface: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub widget: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub hover: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub active: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub floating_card_bg: Color32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Text {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub primary: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub secondary: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub muted: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub disabled: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub on_accent: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub faint: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub subtle: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub hover: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub dim: Color32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Accent {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub primary: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub cyan: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub primary_hover: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub primary_active: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub faint: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub ghost: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub subtle: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub hover: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub strong: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub selection: Color32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Status {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub success: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub warning: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub error: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub info: Color32,
    // Editor-specific semantic aliases retained for Animatix's diagnostics
    // and transport UI. Generic consumers can map these onto the status slots
    // above; future work should move them to the app semantic layer.
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub playing_text: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub diagnostic_error: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub diagnostic_warning: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub success_faint: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub success_ultra_faint: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub warning_subtle: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub error_faint: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub error_ultra_faint: Color32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Border {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub default: Color32,
    /// `strong` mirrors `semantic::border::HOVER` (the strongest neutral border).
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub strong: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub focus: Color32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Overlay {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub backdrop: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub badge_bg: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub tooltip_bg: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub shadow_ambient: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub shadow_direct: Color32,
}

/// Neutral grid / guide line colors (white-alpha; not brand-specific).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Lines {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub grid: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub guide: Color32,
}

/// Elevation shadow tokens for floating surfaces.
///
/// Three conceptual levels: `flat` (no shadow, in-panel chrome — no token needed),
/// `raised` (popover / menu / dropdown / toast — soft small shadow), and
/// `overlay` (dialog / modal — larger shadow on top of backdrop scrim).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Elevation {
    /// Soft small shadow for popover, menu, dropdown, toast.
    ///   Dark: offset [0, 2] / blur 4 / spread 0 / rgba(0,0,0,40)
    ///   Light: offset [0, 3] / blur 6 / spread 0 / rgba(0,0,0,50)
    #[cfg_attr(feature = "theme-json", serde(with = "serde_shadow"))]
    pub raised: Shadow,
    /// Larger shadow for dialog / modal, painted on top of backdrop scrim.
    ///   Dark: offset [0, 8] / blur 24 / spread 0 / rgba(0,0,0,80)
    ///   Light: offset [0, 12] / blur 32 / spread 0 / rgba(0,0,0,60)
    #[cfg_attr(feature = "theme-json", serde(with = "serde_shadow"))]
    pub overlay: Shadow,
}

// ── Theme ─────────────────────────────────────────────────────────────

/// The runtime theme.  Each field is a `Color32`; the struct is `Copy` so cloning is cheap.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Theme {
    pub surface: Surface,
    pub text: Text,
    pub accent: Accent,
    pub status: Status,
    pub border: Border,
    pub overlay: Overlay,
    /// Neutral grid / guide line colors.
    pub lines: Lines,

    // ── Milestone 2: component-scoped slots (B2 + B3) ──
    /// Button color slots for all variants and states.
    pub button: ButtonSlots,
    /// List row color slots (even/odd zebra + selected + hover).
    pub list: ListSlots,
    /// Tab bar color slots (active, inactive, hover).
    pub tab: TabSlots,
    /// Context-menu item color slots.
    pub menu_item: MenuItemSlots,
    /// Text-input color slots.
    pub input: InputSlots,
    /// Scrollbar thumb color slots.
    pub scrollbar: ScrollbarSlots,

    // ── Per-component slots for widgets that formerly picked colours ──
    /// Checkbox / radio / switch color slots.
    pub toggle: ToggleSlots,
    /// Progress-bar track/fill/label slots.
    pub progress: ProgressSlots,
    /// Badge fill/foreground/outline slot.
    pub badge: Slot,
    /// Tag / chip normal + hover slots.
    pub tag: TagSlots,
    /// Keyboard-shortcut badge fill/foreground/outline slot.
    pub kbd: Slot,
    /// Tooltip surface fill/foreground/outline slot.
    pub tooltip: Slot,
    /// Inline alert colors, per level.
    pub alert: AlertSlots,
    /// Toast colors, per level.
    pub toast: ToastSlots,
    /// Loading-skeleton base + shimmer colors.
    pub skeleton: SkeletonSlots,
    /// Colour-picker swatch + checkerboard colors (trigger reuses `input`).
    pub color_picker: ColorPickerSlots,
    /// Easing-curve editor colors.
    pub easing_curve: EasingCurveSlots,

    // ── Elevation / shadow tokens (T2.7) ──
    /// Shadow tokens for floating surfaces.  `flat` has no shadow (in-panel
    /// chrome); use `raised` for popover/menu/toast/dropdown and `overlay`
    /// for dialog/modal.
    pub elevation: Elevation,
}

impl Default for Theme {
    /// Fall back to the dark theme rather than an all-transparent struct, so a
    /// host that renders a widget before calling `set_theme` is still legible.
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// Seeded from the current `semantic` module consts/functions.
    ///
    /// Guarantees `Theme::dark() == tokens::semantic::*` field-for-field because every value
    /// is sourced directly from `crate::tokens::semantic`.
    pub fn dark() -> Self {
        let components = dark_component_slots();
        Self {
            surface: Surface {
                base: semantic::surface::BASE,
                panel: semantic::surface::PANEL,
                surface: semantic::surface::SURFACE,
                widget: semantic::surface::WIDGET,
                hover: semantic::surface::HOVER,
                active: semantic::surface::ACTIVE,
                floating_card_bg: semantic::surface::floating_card_bg(),
            },
            text: Text {
                primary: semantic::text::PRIMARY,
                secondary: semantic::text::SECONDARY,
                muted: semantic::text::MUTED,
                disabled: semantic::text::DISABLED,
                on_accent: semantic::text::ON_ACCENT,
                faint: semantic::text::faint(),
                subtle: semantic::text::subtle(),
                hover: semantic::text::hover(),
                dim: semantic::text::dim(),
            },
            accent: Accent {
                primary: semantic::accent::PRIMARY,
                cyan: semantic::accent::CYAN,
                primary_hover: semantic::accent::PRIMARY_HOVER,
                primary_active: semantic::accent::PRIMARY_ACTIVE,
                faint: semantic::accent::faint(),
                ghost: semantic::accent::ghost(),
                subtle: semantic::accent::subtle(),
                hover: semantic::accent::hover(),
                strong: semantic::accent::strong(),
                selection: semantic::accent::selection(),
            },
            status: Status {
                success: semantic::status::SUCCESS,
                warning: semantic::status::WARNING,
                error: semantic::status::ERROR,
                info: semantic::status::INFO,
                playing_text: semantic::status::PLAYING_TEXT,
                diagnostic_error: semantic::status::DIAGNOSTIC_ERROR,
                diagnostic_warning: semantic::status::DIAGNOSTIC_WARNING,
                success_faint: semantic::status::success_faint(),
                success_ultra_faint: semantic::status::success_ultra_faint(),
                warning_subtle: semantic::status::warning_subtle(),
                error_faint: semantic::status::error_faint(),
                error_ultra_faint: semantic::status::error_ultra_faint(),
            },
            border: Border {
                default: semantic::border::DEFAULT,
                strong: semantic::border::HOVER,
                focus: semantic::border::FOCUS,
            },
            overlay: Overlay {
                backdrop: semantic::overlay::backdrop(),
                badge_bg: semantic::overlay::badge_bg(),
                tooltip_bg: semantic::overlay::tooltip_bg(),
                shadow_ambient: semantic::overlay::shadow_ambient(),
                shadow_direct: semantic::overlay::shadow_direct(),
            },
            lines: Lines {
                grid: semantic::lines::grid_line(),
                guide: semantic::lines::guide_line(),
            },
            button: dark_button_slots(),
            list: dark_list_slots(),
            tab: dark_tab_slots(),
            menu_item: dark_menu_item_slots(),
            input: dark_input_slots(),
            scrollbar: ScrollbarSlots {
                thumb: semantic::border::HOVER,
                thumb_hover: semantic::text::MUTED,
            },
            elevation: Elevation {
                raised: Shadow {
                    offset: [0, 2],
                    blur: 4,
                    spread: 0,
                    color: Color32::from_rgba_unmultiplied(0, 0, 0, 40),
                },
                overlay: Shadow {
                    offset: [0, 8],
                    blur: 24,
                    spread: 0,
                    color: Color32::from_rgba_unmultiplied(0, 0, 0, 80),
                },
            },
            toggle: components.toggle,
            progress: components.progress,
            badge: components.badge,
            tag: components.tag,
            kbd: components.kbd,
            tooltip: components.tooltip,
            alert: components.alert,
            toast: components.toast,
            skeleton: components.skeleton,
            color_picker: components.color_picker,
            easing_curve: components.easing_curve,
        }
    }

    /// Hand-authored light-mode palette.
    ///
    /// Light-mode values are authored directly here (not derived from the
    /// dark-oriented `primitive` palette). All raw `Color32` literals below are
    /// intentional light-palette entries. Accent and status hues are kept the
    /// same as dark mode so the brand identity is consistent; surfaces are
    /// near-white with subtle gray steps and text is dark for contrast.
    pub fn light() -> Self {
        // Surfaces (light grays / near-white).
        let base = Color32::from_rgb(248, 249, 250);
        let panel = Color32::from_rgb(255, 255, 255);
        let surf = Color32::from_rgb(250, 251, 252);
        let widget = Color32::from_rgb(240, 241, 243);
        let hover = Color32::from_rgb(227, 229, 233);
        let active = Color32::from_rgb(210, 212, 216);
        // Text (dark on light).
        let primary = Color32::from_rgb(20, 24, 30);
        let secondary = Color32::from_rgb(90, 97, 112);
        let muted = Color32::from_rgb(103, 109, 123);
        let disabled = Color32::from_rgb(180, 185, 192);
        // Dark text on accent fills matches the dark theme's ON_ACCENT role and
        // keeps primary/hover/accent button states within WCAG AA UI contrast.
        let on_accent = Color32::from_rgb(10, 12, 16);
        // Borders (mid grays).
        let border_default = Color32::from_rgb(200, 204, 209);
        let border_strong = Color32::from_rgb(160, 165, 172);
        let border_focus = semantic::border::FOCUS;
        // Danger active (shared with dark): a darkened error red.
        let danger_active = Color32::from_rgb(200, 40, 40);
        // Overlay fills (light variants); shared by the overlay tokens and the
        // badge/kbd component slots below.
        let badge_bg = Color32::from_rgba_unmultiplied(248, 249, 250, 235);
        let tooltip_bg = Color32::from_rgba_unmultiplied(255, 255, 255, 245);
        let components = light_component_slots(LightComponentPaint {
            base,
            surface: surf,
            widget,
            hover,
            primary,
            secondary,
            disabled,
            on_accent,
            border_default,
            border_strong,
            badge_bg,
            tooltip_bg,
        });

        Self {
            surface: Surface {
                base,
                panel,
                surface: surf,
                widget,
                hover,
                active,
                floating_card_bg: Color32::from_rgba_unmultiplied(255, 255, 255, 245),
            },
            text: Text {
                primary,
                secondary,
                muted,
                disabled,
                on_accent,
                faint: Color32::from_rgba_unmultiplied(20, 24, 30, 80),
                subtle: Color32::from_rgba_unmultiplied(20, 24, 30, 140),
                hover: Color32::from_rgba_unmultiplied(20, 24, 30, 200),
                dim: Color32::from_rgba_unmultiplied(20, 24, 30, 150),
            },
            // Accent/status hues kept identical to dark for brand consistency.
            accent: Accent {
                primary: semantic::accent::PRIMARY,
                cyan: semantic::accent::CYAN,
                primary_hover: semantic::accent::PRIMARY_HOVER,
                primary_active: semantic::accent::PRIMARY_ACTIVE,
                faint: semantic::accent::faint(),
                ghost: semantic::accent::ghost(),
                subtle: semantic::accent::subtle(),
                hover: semantic::accent::hover(),
                strong: semantic::accent::strong(),
                selection: Color32::from_rgba_unmultiplied(
                    semantic::accent::PRIMARY.r(),
                    semantic::accent::PRIMARY.g(),
                    semantic::accent::PRIMARY.b(),
                    60,
                ),
            },
            status: Status {
                success: semantic::status::SUCCESS,
                warning: semantic::status::WARNING,
                error: semantic::status::ERROR,
                info: semantic::status::INFO,
                playing_text: semantic::status::PLAYING_TEXT,
                diagnostic_error: semantic::status::DIAGNOSTIC_ERROR,
                diagnostic_warning: semantic::status::DIAGNOSTIC_WARNING,
                success_faint: semantic::status::success_faint(),
                success_ultra_faint: semantic::status::success_ultra_faint(),
                warning_subtle: semantic::status::warning_subtle(),
                error_faint: semantic::status::error_faint(),
                error_ultra_faint: semantic::status::error_ultra_faint(),
            },
            border: Border {
                default: border_default,
                strong: border_strong,
                focus: border_focus,
            },
            overlay: Overlay {
                backdrop: Color32::from_rgba_unmultiplied(0, 0, 0, 140),
                badge_bg,
                tooltip_bg,
                shadow_ambient: Color32::from_rgba_unmultiplied(0, 0, 0, 30),
                shadow_direct: Color32::from_rgba_unmultiplied(0, 0, 0, 50),
            },
            // White-alpha lines are invisible on a near-white surface; light
            // surfaces need dark ink instead.
            lines: Lines {
                grid: semantic::lines::grid_line_light(),
                guide: semantic::lines::guide_line_light(),
            },
            button: light_button_slots(
                widget,
                hover,
                active,
                primary,
                disabled,
                on_accent,
                border_default,
                border_focus,
                secondary,
                danger_active,
            ),
            list: ListSlots {
                even: Fill {
                    bg: surf,
                    fg: primary,
                },
                odd: Fill {
                    bg: widget,
                    fg: primary,
                },
                selected: Fill {
                    bg: Color32::from_rgba_unmultiplied(
                        semantic::accent::PRIMARY.r(),
                        semantic::accent::PRIMARY.g(),
                        semantic::accent::PRIMARY.b(),
                        60,
                    ),
                    fg: primary,
                },
                hover: Fill {
                    bg: hover,
                    fg: primary,
                },
            },
            tab: TabSlots {
                active: TabSlot {
                    bg: surf,
                    fg: primary,
                    indicator: semantic::accent::PRIMARY,
                },
                inactive: TabSlot {
                    bg: widget,
                    fg: secondary,
                    indicator: Color32::TRANSPARENT,
                },
                hover: TabSlot {
                    bg: hover,
                    fg: primary,
                    indicator: Color32::TRANSPARENT,
                },
            },
            menu_item: MenuItemSlots {
                normal: Slot {
                    bg: Color32::TRANSPARENT,
                    fg: primary,
                    border: Color32::TRANSPARENT,
                },
                hover: Slot {
                    bg: hover,
                    fg: primary,
                    border: Color32::TRANSPARENT,
                },
                active: Slot {
                    bg: active,
                    fg: primary,
                    border: Color32::TRANSPARENT,
                },
                disabled: Slot {
                    bg: Color32::TRANSPARENT,
                    fg: disabled,
                    border: Color32::TRANSPARENT,
                },
            },
            input: InputSlots {
                normal: Slot {
                    bg: widget,
                    fg: primary,
                    border: border_default,
                },
                hover: Slot {
                    bg: widget,
                    fg: primary,
                    border: border_strong,
                },
                focus: Slot {
                    bg: widget,
                    fg: primary,
                    border: border_focus,
                },
                invalid: Slot {
                    bg: widget,
                    fg: primary,
                    border: semantic::status::ERROR,
                },
                disabled: Slot {
                    bg: widget,
                    fg: disabled,
                    border: border_default,
                },
            },
            scrollbar: ScrollbarSlots {
                thumb: border_strong,
                thumb_hover: semantic::accent::PRIMARY,
            },
            elevation: Elevation {
                // Light: raised shadow — slightly stronger offset/blur for light bg
                raised: Shadow {
                    offset: [0, 3],
                    blur: 6,
                    spread: 0,
                    color: Color32::from_rgba_unmultiplied(0, 0, 0, 50),
                },
                // Light: overlay shadow — larger for dialog/modal
                overlay: Shadow {
                    offset: [0, 12],
                    blur: 32,
                    spread: 0,
                    color: Color32::from_rgba_unmultiplied(0, 0, 0, 60),
                },
            },
            toggle: components.toggle,
            progress: components.progress,
            badge: components.badge,
            tag: components.tag,
            kbd: components.kbd,
            tooltip: components.tooltip,
            alert: components.alert,
            toast: components.toast,
            skeleton: components.skeleton,
            color_picker: components.color_picker,
            easing_curve: components.easing_curve,
        }
    }

    /// The single focus-ring color for this theme.
    ///
    /// All focusable widgets must use this one slot so the focus indicator is
    /// consistent across the UI. Do not add additional focus colors elsewhere.
    pub fn focus_ring(&self) -> Color32 {
        self.border.focus
    }

    /// Convenience accessor for the `raised` elevation shadow.
    /// Use for popover, menu, dropdown, and toast.
    pub fn elevation_raised(&self) -> Shadow {
        self.elevation.raised
    }

    /// Convenience accessor for the `overlay` elevation shadow.
    /// Use for dialog / modal surfaces.
    pub fn elevation_overlay(&self) -> Shadow {
        self.elevation.overlay
    }

    /// Map this theme onto an `egui::Visuals` so stock egui widgets match.
    ///
    /// `dark` selects the egui base visuals; the method then overrides the same
    /// fields the GUI's `install_theme()` sets (panel/window fills, widget
    /// states, selection, text color).
    pub fn to_visuals(&self, dark: bool) -> Visuals {
        let mut v = if dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };

        v.panel_fill = self.surface.panel;
        v.window_fill = self.surface.panel;
        v.extreme_bg_color = self.surface.base;
        v.faint_bg_color = self.surface.surface;

        v.selection.bg_fill = self.accent.selection;
        v.selection.stroke = Stroke::new(STROKE_WIDTH, self.accent.primary);
        v.override_text_color = Some(self.text.primary);

        let radius = CornerRadius::same(RADIUS_M as u8);

        let ni = &mut v.widgets.noninteractive;
        ni.bg_fill = self.surface.surface;
        ni.weak_bg_fill = self.surface.surface;
        ni.bg_stroke = Stroke::new(STROKE_WIDTH, self.border.default);
        ni.fg_stroke = Stroke::new(STROKE_WIDTH, self.text.secondary);
        ni.corner_radius = radius;

        let ina = &mut v.widgets.inactive;
        ina.bg_fill = self.surface.widget;
        ina.weak_bg_fill = self.surface.widget;
        ina.bg_stroke = Stroke::new(STROKE_WIDTH, self.border.default);
        ina.fg_stroke = Stroke::new(STROKE_WIDTH, self.text.primary);
        ina.corner_radius = radius;

        let hv = &mut v.widgets.hovered;
        hv.bg_fill = self.surface.hover;
        hv.weak_bg_fill = self.surface.hover;
        hv.bg_stroke = Stroke::new(STROKE_WIDTH, self.accent.primary);
        hv.fg_stroke = Stroke::new(STROKE_WIDTH, self.text.primary);
        hv.corner_radius = radius;

        let ac = &mut v.widgets.active;
        ac.bg_fill = self.surface.active;
        ac.weak_bg_fill = self.surface.active;
        ac.bg_stroke = Stroke::new(STROKE_WIDTH, self.accent.primary);
        ac.fg_stroke = Stroke::new(STROKE_WIDTH, self.text.primary);
        ac.corner_radius = radius;

        // Menus and combo boxes use the `open` state for their trigger frame.
        // Keep its geometry aligned with the other interactive states so opening
        // a popup cannot introduce a state-dependent layout shift.
        let op = &mut v.widgets.open;
        op.bg_fill = self.surface.active;
        op.weak_bg_fill = self.surface.active;
        op.bg_stroke = Stroke::new(STROKE_WIDTH, self.accent.primary);
        op.fg_stroke = Stroke::new(STROKE_WIDTH, self.text.primary);
        op.corner_radius = radius;
        op.expansion = 0.0;

        v
    }
}

// ── Dark component-slot seeding (matches widget/button.rs painting) ─────

fn dark_button_slots() -> ButtonSlots {
    let transparent = Color32::TRANSPARENT;
    let focus = Slot {
        bg: transparent,
        fg: transparent,
        border: semantic::border::FOCUS,
    };
    ButtonSlots {
        primary: ButtonStateSlots {
            normal: Slot {
                bg: semantic::accent::PRIMARY,
                fg: semantic::text::ON_ACCENT,
                border: semantic::accent::PRIMARY,
            },
            hover: Slot {
                bg: semantic::accent::PRIMARY_HOVER,
                fg: semantic::text::ON_ACCENT,
                border: semantic::accent::PRIMARY_HOVER,
            },
            active: Slot {
                bg: semantic::accent::PRIMARY_ACTIVE,
                fg: semantic::text::ON_ACCENT,
                border: semantic::accent::PRIMARY_ACTIVE,
            },
            selected: Slot {
                bg: semantic::accent::PRIMARY_ACTIVE,
                fg: semantic::text::ON_ACCENT,
                border: semantic::accent::PRIMARY_ACTIVE,
            },
            disabled: Slot {
                bg: semantic::surface::WIDGET,
                fg: semantic::text::DISABLED,
                border: semantic::surface::WIDGET,
            },
            focus,
        },
        secondary: ButtonStateSlots {
            normal: Slot {
                bg: semantic::surface::WIDGET,
                fg: semantic::text::PRIMARY,
                border: semantic::border::DEFAULT,
            },
            hover: Slot {
                bg: semantic::surface::HOVER,
                fg: semantic::text::PRIMARY,
                border: semantic::accent::PRIMARY,
            },
            active: Slot {
                bg: semantic::surface::ACTIVE,
                fg: semantic::text::PRIMARY,
                border: semantic::accent::PRIMARY,
            },
            selected: Slot {
                bg: semantic::surface::ACTIVE,
                fg: semantic::text::PRIMARY,
                border: semantic::accent::PRIMARY,
            },
            disabled: Slot {
                bg: semantic::surface::WIDGET,
                fg: semantic::text::DISABLED,
                border: semantic::border::DEFAULT,
            },
            focus,
        },
        ghost: ButtonStateSlots {
            normal: Slot {
                bg: transparent,
                fg: semantic::text::SECONDARY,
                border: transparent,
            },
            hover: Slot {
                bg: semantic::surface::HOVER,
                fg: semantic::text::PRIMARY,
                border: transparent,
            },
            active: Slot {
                bg: semantic::surface::ACTIVE,
                fg: semantic::accent::PRIMARY,
                border: transparent,
            },
            selected: Slot {
                bg: semantic::surface::ACTIVE,
                fg: semantic::accent::PRIMARY,
                border: semantic::accent::PRIMARY,
            },
            disabled: Slot {
                bg: transparent,
                fg: semantic::text::DISABLED,
                border: transparent,
            },
            focus,
        },
        icon: ButtonStateSlots {
            normal: Slot {
                bg: transparent,
                fg: semantic::text::SECONDARY,
                border: transparent,
            },
            hover: Slot {
                bg: semantic::surface::HOVER,
                fg: semantic::text::PRIMARY,
                border: semantic::accent::PRIMARY,
            },
            active: Slot {
                bg: semantic::surface::ACTIVE,
                fg: semantic::text::PRIMARY,
                border: semantic::accent::PRIMARY,
            },
            selected: Slot {
                bg: semantic::surface::ACTIVE,
                fg: semantic::text::PRIMARY,
                border: semantic::accent::PRIMARY,
            },
            disabled: Slot {
                bg: transparent,
                fg: semantic::text::DISABLED,
                border: transparent,
            },
            focus,
        },
        danger: ButtonStateSlots {
            normal: Slot {
                bg: semantic::status::error_faint(),
                fg: semantic::text::ON_ACCENT,
                border: semantic::status::ERROR,
            },
            hover: Slot {
                bg: semantic::status::ERROR,
                fg: semantic::text::ON_ACCENT,
                border: semantic::status::ERROR,
            },
            active: Slot {
                bg: Color32::from_rgb(200, 40, 40),
                fg: semantic::text::ON_ACCENT,
                border: Color32::from_rgb(200, 40, 40),
            },
            selected: Slot {
                bg: semantic::status::ERROR,
                fg: semantic::text::ON_ACCENT,
                border: semantic::status::ERROR,
            },
            disabled: Slot {
                bg: semantic::surface::WIDGET,
                fg: semantic::text::DISABLED,
                border: semantic::surface::WIDGET,
            },
            focus,
        },
    }
}

fn dark_list_slots() -> ListSlots {
    ListSlots {
        even: Fill {
            bg: semantic::surface::SURFACE,
            fg: semantic::text::PRIMARY,
        },
        odd: Fill {
            bg: semantic::surface::WIDGET,
            fg: semantic::text::PRIMARY,
        },
        selected: Fill {
            bg: semantic::accent::selection(),
            fg: semantic::text::PRIMARY,
        },
        hover: Fill {
            bg: semantic::surface::HOVER,
            fg: semantic::text::PRIMARY,
        },
    }
}

fn dark_tab_slots() -> TabSlots {
    TabSlots {
        active: TabSlot {
            bg: semantic::surface::SURFACE,
            fg: semantic::text::PRIMARY,
            indicator: semantic::accent::PRIMARY,
        },
        inactive: TabSlot {
            bg: semantic::surface::WIDGET,
            fg: semantic::text::SECONDARY,
            indicator: Color32::TRANSPARENT,
        },
        hover: TabSlot {
            bg: semantic::surface::HOVER,
            fg: semantic::text::PRIMARY,
            indicator: Color32::TRANSPARENT,
        },
    }
}

fn dark_menu_item_slots() -> MenuItemSlots {
    MenuItemSlots {
        normal: Slot {
            bg: Color32::TRANSPARENT,
            fg: semantic::text::PRIMARY,
            border: Color32::TRANSPARENT,
        },
        hover: Slot {
            bg: semantic::surface::HOVER,
            fg: semantic::text::PRIMARY,
            border: Color32::TRANSPARENT,
        },
        active: Slot {
            bg: semantic::surface::ACTIVE,
            fg: semantic::text::PRIMARY,
            border: Color32::TRANSPARENT,
        },
        disabled: Slot {
            bg: Color32::TRANSPARENT,
            fg: semantic::text::DISABLED,
            border: Color32::TRANSPARENT,
        },
    }
}

fn dark_input_slots() -> InputSlots {
    InputSlots {
        normal: Slot {
            bg: semantic::surface::WIDGET,
            fg: semantic::text::PRIMARY,
            border: semantic::border::DEFAULT,
        },
        hover: Slot {
            bg: semantic::surface::WIDGET,
            fg: semantic::text::PRIMARY,
            border: semantic::border::HOVER,
        },
        focus: Slot {
            bg: semantic::surface::WIDGET,
            fg: semantic::text::PRIMARY,
            border: semantic::border::FOCUS,
        },
        invalid: Slot {
            bg: semantic::surface::WIDGET,
            fg: semantic::text::PRIMARY,
            border: semantic::status::ERROR,
        },
        disabled: Slot {
            bg: semantic::surface::WIDGET,
            fg: semantic::text::DISABLED,
            border: semantic::border::DEFAULT,
        },
    }
}

#[allow(clippy::too_many_arguments)] // Light slot seeding threads independent light-palette colors; grouping them is unnecessary indirection.
fn light_button_slots(
    widget: Color32,
    hover: Color32,
    active: Color32,
    text_primary: Color32,
    disabled: Color32,
    on_accent: Color32,
    border_default: Color32,
    border_focus: Color32,
    secondary: Color32,
    danger_active: Color32,
) -> ButtonSlots {
    let transparent = Color32::TRANSPARENT;
    let focus = Slot {
        bg: transparent,
        fg: transparent,
        border: border_focus,
    };
    let a = semantic::accent::PRIMARY;
    let a_hover = semantic::accent::PRIMARY_HOVER;
    let a_active = semantic::accent::PRIMARY_ACTIVE;
    ButtonSlots {
        primary: ButtonStateSlots {
            normal: Slot {
                bg: a,
                fg: on_accent,
                border: a,
            },
            hover: Slot {
                bg: a_hover,
                fg: on_accent,
                border: a_hover,
            },
            active: Slot {
                bg: a_active,
                fg: on_accent,
                border: a_active,
            },
            selected: Slot {
                bg: a_active,
                fg: on_accent,
                border: a_active,
            },
            disabled: Slot {
                bg: widget,
                fg: disabled,
                border: widget,
            },
            focus,
        },
        secondary: ButtonStateSlots {
            normal: Slot {
                bg: widget,
                fg: text_primary,
                border: border_default,
            },
            hover: Slot {
                bg: hover,
                fg: text_primary,
                border: a,
            },
            active: Slot {
                bg: active,
                fg: text_primary,
                border: a,
            },
            selected: Slot {
                bg: active,
                fg: text_primary,
                border: a,
            },
            disabled: Slot {
                bg: widget,
                fg: disabled,
                border: border_default,
            },
            focus,
        },
        ghost: ButtonStateSlots {
            normal: Slot {
                bg: transparent,
                fg: secondary,
                border: transparent,
            },
            hover: Slot {
                bg: hover,
                fg: text_primary,
                border: transparent,
            },
            active: Slot {
                bg: active,
                fg: a,
                border: transparent,
            },
            selected: Slot {
                bg: active,
                fg: a,
                border: a,
            },
            disabled: Slot {
                bg: transparent,
                fg: disabled,
                border: transparent,
            },
            focus,
        },
        icon: ButtonStateSlots {
            normal: Slot {
                bg: transparent,
                fg: secondary,
                border: transparent,
            },
            hover: Slot {
                bg: hover,
                fg: text_primary,
                border: a,
            },
            active: Slot {
                bg: active,
                fg: text_primary,
                border: a,
            },
            selected: Slot {
                bg: active,
                fg: text_primary,
                border: a,
            },
            disabled: Slot {
                bg: transparent,
                fg: disabled,
                border: transparent,
            },
            focus,
        },
        danger: ButtonStateSlots {
            normal: Slot {
                bg: semantic::status::error_faint(),
                fg: on_accent,
                border: semantic::status::ERROR,
            },
            hover: Slot {
                bg: semantic::status::ERROR,
                fg: on_accent,
                border: semantic::status::ERROR,
            },
            active: Slot {
                bg: danger_active,
                fg: on_accent,
                border: danger_active,
            },
            selected: Slot {
                bg: semantic::status::ERROR,
                fg: on_accent,
                border: semantic::status::ERROR,
            },
            disabled: Slot {
                bg: widget,
                fg: disabled,
                border: widget,
            },
            focus,
        },
    }
}

// ── Per-component slot seeding (B) ────────────────────────────────────

/// The 11 per-component slot groups, bundled so both `Theme::dark()` and
/// `Theme::light()` can share one set of `Self { .. }` assignments.
struct ComponentSlots {
    toggle: ToggleSlots,
    progress: ProgressSlots,
    badge: Slot,
    tag: TagSlots,
    kbd: Slot,
    tooltip: Slot,
    alert: AlertSlots,
    toast: ToastSlots,
    skeleton: SkeletonSlots,
    color_picker: ColorPickerSlots,
    easing_curve: EasingCurveSlots,
}

/// Light-mode inputs the component slots need. Threading these avoids
/// re-deriving the light palette inside the seeding function (the palette is
/// authored once in `Theme::light()`).
struct LightComponentPaint {
    base: Color32,
    surface: Color32,
    widget: Color32,
    hover: Color32,
    primary: Color32,
    secondary: Color32,
    disabled: Color32,
    on_accent: Color32,
    border_default: Color32,
    border_strong: Color32,
    badge_bg: Color32,
    tooltip_bg: Color32,
}

/// Seed the per-level alert colors from the shared status roles.
///
/// Info borrows the accent `faint` tint (there is no `info_faint` status token);
/// every level's icon/accent uses its own status role.
fn alert_level_slots(bg: Color32, accent: Color32, fg: Color32) -> AlertLevelSlots {
    AlertLevelSlots {
        bg,
        fg,
        accent,
        icon: accent,
    }
}

fn toast_level_slots(
    bg: Color32,
    fg: Color32,
    border: Color32,
    accent: Color32,
) -> ToastLevelSlots {
    ToastLevelSlots {
        bg,
        fg,
        border,
        accent,
    }
}

fn dark_component_slots() -> ComponentSlots {
    let transparent = Color32::TRANSPARENT;
    ComponentSlots {
        toggle: ToggleSlots {
            checked: Slot {
                bg: semantic::accent::PRIMARY,
                fg: semantic::text::ON_ACCENT,
                border: semantic::accent::PRIMARY,
            },
            unchecked: Slot {
                bg: semantic::surface::WIDGET,
                fg: semantic::text::ON_ACCENT,
                border: semantic::border::DEFAULT,
            },
            hover: Slot {
                bg: semantic::surface::HOVER,
                fg: semantic::text::ON_ACCENT,
                border: semantic::accent::PRIMARY,
            },
            disabled: Slot {
                bg: semantic::surface::WIDGET,
                fg: semantic::text::DISABLED,
                border: semantic::border::DEFAULT,
            },
            thumb: semantic::text::ON_ACCENT,
            mark: semantic::text::ON_ACCENT,
        },
        progress: ProgressSlots {
            track: semantic::surface::WIDGET,
            fill: semantic::accent::PRIMARY,
            label: semantic::text::PRIMARY,
            label_on_fill: semantic::text::ON_ACCENT,
        },
        badge: Slot {
            bg: semantic::overlay::badge_bg(),
            fg: semantic::accent::PRIMARY,
            border: transparent,
        },
        tag: TagSlots {
            normal: Slot {
                bg: semantic::surface::WIDGET,
                fg: semantic::text::PRIMARY,
                border: semantic::border::DEFAULT,
            },
            hover: Slot {
                bg: semantic::surface::HOVER,
                fg: semantic::text::PRIMARY,
                border: semantic::accent::PRIMARY,
            },
        },
        kbd: Slot {
            bg: semantic::overlay::badge_bg(),
            fg: semantic::text::SECONDARY,
            border: semantic::border::DEFAULT,
        },
        tooltip: Slot {
            bg: semantic::overlay::tooltip_bg(),
            fg: semantic::text::PRIMARY,
            border: semantic::border::DEFAULT,
        },
        alert: AlertSlots {
            info: alert_level_slots(
                semantic::accent::faint(),
                semantic::status::INFO,
                semantic::text::PRIMARY,
            ),
            success: alert_level_slots(
                semantic::status::success_faint(),
                semantic::status::SUCCESS,
                semantic::text::PRIMARY,
            ),
            warning: alert_level_slots(
                semantic::status::warning_subtle(),
                semantic::status::WARNING,
                semantic::text::PRIMARY,
            ),
            error: alert_level_slots(
                semantic::status::error_faint(),
                semantic::status::ERROR,
                semantic::text::PRIMARY,
            ),
        },
        toast: ToastSlots {
            info: toast_level_slots(
                semantic::surface::SURFACE,
                semantic::text::PRIMARY,
                semantic::border::DEFAULT,
                semantic::accent::PRIMARY,
            ),
            success: toast_level_slots(
                semantic::surface::SURFACE,
                semantic::text::PRIMARY,
                semantic::border::DEFAULT,
                semantic::status::SUCCESS,
            ),
            warning: toast_level_slots(
                semantic::surface::SURFACE,
                semantic::text::PRIMARY,
                semantic::border::DEFAULT,
                semantic::status::WARNING,
            ),
            error: toast_level_slots(
                semantic::surface::SURFACE,
                semantic::text::PRIMARY,
                semantic::border::DEFAULT,
                semantic::status::ERROR,
            ),
        },
        skeleton: SkeletonSlots {
            base: semantic::surface::WIDGET,
            shimmer: semantic::accent::PRIMARY,
        },
        color_picker: ColorPickerSlots {
            swatch_border: semantic::border::DEFAULT,
            swatch_hover: semantic::border::HOVER,
            // Checkerboard: light square is a fixed white, dark square uses the
            // disabled-neutral gray (matching the previous inline painting).
            checker_light: Color32::WHITE,
            checker_dark: semantic::text::DISABLED,
        },
        easing_curve: EasingCurveSlots {
            bg: semantic::surface::BASE,
            grid: semantic::lines::grid_line(),
            curve: semantic::accent::PRIMARY,
            handle: semantic::accent::PRIMARY,
            handle_active: semantic::status::WARNING,
        },
    }
}

fn light_component_slots(p: LightComponentPaint) -> ComponentSlots {
    let transparent = Color32::TRANSPARENT;
    ComponentSlots {
        toggle: ToggleSlots {
            checked: Slot {
                bg: semantic::accent::PRIMARY,
                fg: p.on_accent,
                border: semantic::accent::PRIMARY,
            },
            unchecked: Slot {
                bg: p.widget,
                fg: p.on_accent,
                border: p.border_default,
            },
            hover: Slot {
                bg: p.hover,
                fg: p.on_accent,
                border: semantic::accent::PRIMARY,
            },
            disabled: Slot {
                bg: p.widget,
                fg: p.disabled,
                border: p.border_default,
            },
            thumb: p.on_accent,
            mark: p.on_accent,
        },
        progress: ProgressSlots {
            track: p.widget,
            fill: semantic::accent::PRIMARY,
            label: p.primary,
            label_on_fill: p.on_accent,
        },
        badge: Slot {
            bg: p.badge_bg,
            fg: semantic::accent::PRIMARY,
            border: transparent,
        },
        tag: TagSlots {
            normal: Slot {
                bg: p.widget,
                fg: p.primary,
                border: p.border_default,
            },
            hover: Slot {
                bg: p.hover,
                fg: p.primary,
                border: semantic::accent::PRIMARY,
            },
        },
        kbd: Slot {
            bg: p.badge_bg,
            fg: p.secondary,
            border: p.border_default,
        },
        tooltip: Slot {
            bg: p.tooltip_bg,
            fg: p.primary,
            border: p.border_default,
        },
        alert: AlertSlots {
            info: alert_level_slots(semantic::accent::faint(), semantic::status::INFO, p.primary),
            success: alert_level_slots(
                semantic::status::success_faint(),
                semantic::status::SUCCESS,
                p.primary,
            ),
            warning: alert_level_slots(
                semantic::status::warning_subtle(),
                semantic::status::WARNING,
                p.primary,
            ),
            error: alert_level_slots(
                semantic::status::error_faint(),
                semantic::status::ERROR,
                p.primary,
            ),
        },
        toast: ToastSlots {
            info: toast_level_slots(
                p.surface,
                p.primary,
                p.border_default,
                semantic::accent::PRIMARY,
            ),
            success: toast_level_slots(
                p.surface,
                p.primary,
                p.border_default,
                semantic::status::SUCCESS,
            ),
            warning: toast_level_slots(
                p.surface,
                p.primary,
                p.border_default,
                semantic::status::WARNING,
            ),
            error: toast_level_slots(
                p.surface,
                p.primary,
                p.border_default,
                semantic::status::ERROR,
            ),
        },
        skeleton: SkeletonSlots {
            base: p.widget,
            shimmer: semantic::accent::PRIMARY,
        },
        color_picker: ColorPickerSlots {
            swatch_border: p.border_default,
            swatch_hover: p.border_strong,
            checker_light: Color32::WHITE,
            checker_dark: p.disabled,
        },
        easing_curve: EasingCurveSlots {
            bg: p.base,
            grid: semantic::lines::grid_line_light(),
            curve: semantic::accent::PRIMARY,
            handle: semantic::accent::PRIMARY,
            handle_active: semantic::status::WARNING,
        },
    }
}

// ── Immediate-mode Memory accessors ───────────────────────────────────

/// Read the current `Theme` from the `Ui`'s `egui::Context`.
pub fn theme(ui: &egui::Ui) -> Theme {
    theme_from_ctx(ui.ctx())
}

/// Read the current `Theme` from an `egui::Context`.
pub fn theme_from_ctx(ctx: &Context) -> Theme {
    ctx.data(|d| d.get_temp::<Theme>(egui::Id::new("eparts_theme")))
        .unwrap_or_default()
}

/// Store a new `Theme` in the `egui::Context`'s `Memory`.
pub fn set_theme(ctx: &Context, theme: Theme) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("eparts_theme"), theme));
}

// ── App theme choice / system auto-theme policy (B11) ─────────────────

/// A user-facing theme preference. `Auto` follows the OS light/dark setting.
///
/// This is the reusable policy half of cross-platform auto-theming: the host
/// app detects the OS appearance (egui/winit natively on Windows/macOS, or the
/// `dark-light` crate on Linux) and calls [`AppThemeChoice::resolve`] to pick a
/// concrete [`Theme`]. The app keeps the choice (egui's resolved system theme is
/// `pub(crate)`), then `eparts::set_theme` + `ctx.set_visuals(theme.to_visuals(..))`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AppThemeChoice {
    /// Follow the OS light/dark setting (falls back to dark when unknown).
    #[default]
    Auto,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

impl AppThemeChoice {
    /// Resolve to a concrete [`Theme`]. `system_is_dark` is the detected OS
    /// appearance (`None` = unknown → dark fallback), only consulted for `Auto`.
    pub fn resolve(self, system_is_dark: Option<bool>) -> Theme {
        if self.is_dark(system_is_dark) {
            Theme::dark()
        } else {
            Theme::light()
        }
    }

    /// Whether the effective theme is dark, given the detected OS appearance.
    pub fn is_dark(self, system_is_dark: Option<bool>) -> bool {
        match self {
            AppThemeChoice::Light => false,
            AppThemeChoice::Dark => true,
            // Unknown system appearance falls back to dark (matches egui's default).
            AppThemeChoice::Auto => system_is_dark.unwrap_or(true),
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::util::{WCAG_AA_TEXT, WCAG_AA_UI, contrast_ratio};

    fn assert_contrast(fg: Color32, bg: Color32, threshold: f64, label: &str) {
        let ratio = contrast_ratio(fg, bg);
        assert!(
            ratio >= threshold,
            "{label} contrast is {ratio:.2}:1, expected at least {threshold:.1}:1"
        );
    }

    #[test]
    fn light_core_text_surface_pairs_meet_wcag_aa() {
        let t = Theme::light();
        let surfaces = [
            ("base", t.surface.base),
            ("panel", t.surface.panel),
            ("surface", t.surface.surface),
            ("widget", t.surface.widget),
        ];

        for (surface_name, bg) in surfaces {
            assert_contrast(t.text.primary, bg, WCAG_AA_TEXT, &format!("primary/{surface_name}"));
            assert_contrast(
                t.text.secondary,
                bg,
                WCAG_AA_TEXT,
                &format!("secondary/{surface_name}"),
            );
            assert_contrast(t.text.muted, bg, WCAG_AA_TEXT, &format!("muted/{surface_name}"));
        }

        assert_contrast(t.text.primary, t.surface.hover, WCAG_AA_TEXT, "primary/hover");
        assert_contrast(t.text.primary, t.surface.active, WCAG_AA_TEXT, "primary/active");
    }

    #[test]
    fn light_accent_button_pairs_meet_wcag_aa() {
        let t = Theme::light();
        assert_contrast(
            t.button.primary.normal.fg,
            t.button.primary.normal.bg,
            WCAG_AA_TEXT,
            "primary button normal",
        );
        assert_contrast(
            t.button.primary.hover.fg,
            t.button.primary.hover.bg,
            WCAG_AA_UI,
            "primary button hover",
        );
        assert_contrast(
            t.button.primary.active.fg,
            t.button.primary.active.bg,
            WCAG_AA_UI,
            "primary button active",
        );
    }

    #[test]
    fn dark_matches_semantic_constants() {
        let t = Theme::dark();
        assert_eq!(t.surface.base, semantic::surface::BASE);
        assert_eq!(t.surface.panel, semantic::surface::PANEL);
        assert_eq!(t.surface.surface, semantic::surface::SURFACE);
        assert_eq!(t.surface.widget, semantic::surface::WIDGET);
        assert_eq!(t.surface.hover, semantic::surface::HOVER);
        assert_eq!(t.surface.active, semantic::surface::ACTIVE);
        assert_eq!(t.surface.floating_card_bg, semantic::surface::floating_card_bg());

        assert_eq!(t.text.primary, semantic::text::PRIMARY);
        assert_eq!(t.text.secondary, semantic::text::SECONDARY);
        assert_eq!(t.text.muted, semantic::text::MUTED);
        assert_eq!(t.text.disabled, semantic::text::DISABLED);
        assert_eq!(t.text.on_accent, semantic::text::ON_ACCENT);
        assert_eq!(t.text.faint, semantic::text::faint());
        assert_eq!(t.text.subtle, semantic::text::subtle());
        assert_eq!(t.text.hover, semantic::text::hover());
        assert_eq!(t.text.dim, semantic::text::dim());

        assert_eq!(t.accent.primary, semantic::accent::PRIMARY);
        assert_eq!(t.accent.cyan, semantic::accent::CYAN);
        assert_eq!(t.accent.primary_hover, semantic::accent::PRIMARY_HOVER);
        assert_eq!(t.accent.primary_active, semantic::accent::PRIMARY_ACTIVE);
        assert_eq!(t.accent.faint, semantic::accent::faint());
        assert_eq!(t.accent.ghost, semantic::accent::ghost());
        assert_eq!(t.accent.subtle, semantic::accent::subtle());
        assert_eq!(t.accent.hover, semantic::accent::hover());
        assert_eq!(t.accent.strong, semantic::accent::strong());
        assert_eq!(t.accent.selection, semantic::accent::selection());

        assert_eq!(t.status.success, semantic::status::SUCCESS);
        assert_eq!(t.status.warning, semantic::status::WARNING);
        assert_eq!(t.status.error, semantic::status::ERROR);
        assert_eq!(t.status.info, semantic::status::INFO);
        assert_eq!(t.status.playing_text, semantic::status::PLAYING_TEXT);
        assert_eq!(t.status.diagnostic_error, semantic::status::DIAGNOSTIC_ERROR);
        assert_eq!(t.status.diagnostic_warning, semantic::status::DIAGNOSTIC_WARNING);
        assert_eq!(t.status.success_faint, semantic::status::success_faint());
        assert_eq!(t.status.success_ultra_faint, semantic::status::success_ultra_faint());
        assert_eq!(t.status.warning_subtle, semantic::status::warning_subtle());
        assert_eq!(t.status.error_faint, semantic::status::error_faint());
        assert_eq!(t.status.error_ultra_faint, semantic::status::error_ultra_faint());

        assert_eq!(t.border.default, semantic::border::DEFAULT);
        assert_eq!(t.border.strong, semantic::border::HOVER);
        assert_eq!(t.border.focus, semantic::border::FOCUS);

        assert_eq!(t.overlay.backdrop, semantic::overlay::backdrop());
        assert_eq!(t.overlay.badge_bg, semantic::overlay::badge_bg());
        assert_eq!(t.overlay.tooltip_bg, semantic::overlay::tooltip_bg());
        assert_eq!(t.overlay.shadow_ambient, semantic::overlay::shadow_ambient());
        assert_eq!(t.overlay.shadow_direct, semantic::overlay::shadow_direct());

        // Elevation shadows are non-zero (not default).
        assert!(t.elevation.raised.blur > 0);
        assert!(t.elevation.overlay.blur > t.elevation.raised.blur);
        assert!(t.elevation.overlay.color.a() > t.elevation.raised.color.a());

        assert_eq!(t.lines.grid, semantic::lines::grid_line());
        assert_eq!(t.lines.guide, semantic::lines::guide_line());
    }

    #[test]
    fn theme_memory_round_trip() {
        let ctx = Context::default();
        let original = Theme::dark();
        set_theme(&ctx, original);
        let read = theme_from_ctx(&ctx);
        assert_eq!(read.surface.base, original.surface.base);
        assert_eq!(read.text.primary, original.text.primary);
        assert_eq!(read.accent.primary, original.accent.primary);
        assert_eq!(read.status.success, original.status.success);
        assert_eq!(read.border.default, original.border.default);
        assert_eq!(read.overlay.backdrop, original.overlay.backdrop);
        assert_eq!(read.elevation.raised, original.elevation.raised);
        assert_eq!(read.elevation.overlay, original.elevation.overlay);
    }

    #[test]
    fn light_distinct_from_dark() {
        let d = Theme::dark();
        let l = Theme::light();
        assert_ne!(l.surface.base, d.surface.base);
        assert_ne!(l.surface.panel, d.surface.panel);
        assert_ne!(l.text.primary, d.text.primary);
        assert_ne!(l.border.default, d.border.default);
        assert_ne!(l.overlay.backdrop, d.overlay.backdrop);
        // Light surface is bright, light text is dark (contrast sanity).
        assert!(l.surface.base.r() > 200 && l.surface.base.g() > 200 && l.surface.base.b() > 200);
        assert!(l.text.primary.r() < 80 && l.text.primary.g() < 80 && l.text.primary.b() < 80);
        // Raised is smaller than overlay in both themes.
        assert!(d.elevation.overlay.blur > d.elevation.raised.blur);
        assert!(l.elevation.overlay.blur > l.elevation.raised.blur);
        // Light raised is slightly stronger than dark (light surfaces need more contrast).
        assert!(l.elevation.raised.blur >= d.elevation.raised.blur);
    }

    #[test]
    fn focus_ring_matches_border_focus() {
        assert_eq!(Theme::dark().focus_ring(), Theme::dark().border.focus);
        assert_eq!(Theme::light().focus_ring(), Theme::light().border.focus);
    }

    #[test]
    fn accessor_methods_return_correct_values() {
        let d = Theme::dark();
        assert_eq!(d.elevation_raised(), d.elevation.raised);
        assert_eq!(d.elevation_overlay(), d.elevation.overlay);
        let l = Theme::light();
        assert_eq!(l.elevation_raised(), l.elevation.raised);
        assert_eq!(l.elevation_overlay(), l.elevation.overlay);
    }

    #[test]
    fn to_visuals_maps_theme_fields() {
        let t = Theme::dark();
        let v = t.to_visuals(true);
        assert_eq!(v.panel_fill, t.surface.panel);
        assert_eq!(v.selection.bg_fill, t.accent.selection);
        assert_eq!(v.extreme_bg_color, t.surface.base);

        let lt = Theme::light();
        let lv = lt.to_visuals(false);
        assert_eq!(lv.panel_fill, lt.surface.panel);
        assert_eq!(v.widgets.open.corner_radius, v.widgets.hovered.corner_radius);
        assert_eq!(v.widgets.open.bg_stroke.width, STROKE_WIDTH);
        assert_eq!(v.widgets.open.expansion, 0.0);
        assert_eq!(lv.widgets.open.corner_radius, lv.widgets.hovered.corner_radius);
        assert_eq!(lv.widgets.open.bg_stroke.width, STROKE_WIDTH);
        assert_eq!(lv.widgets.open.expansion, 0.0);
    }

    #[test]
    fn component_slots_seeded() {
        let t = Theme::dark();
        // Primary button normal bg is the accent; danger path exists.
        assert_eq!(t.button.primary.normal.bg, semantic::accent::PRIMARY);
        assert_eq!(t.button.danger.hover.bg, semantic::status::ERROR);
        assert_eq!(t.tab.active.indicator, semantic::accent::PRIMARY);
        assert_eq!(t.input.focus.border, semantic::border::FOCUS);
    }

    /// Every per-component slot group this change introduced must be defined in
    /// both themes, with non-transparent values where the widget paints them.
    #[test]
    fn per_component_slots_defined_in_both_themes() {
        for (label, t) in [("dark", Theme::dark()), ("light", Theme::light())] {
            // Toggle: all four states plus thumb/mark must be visible colours.
            for (state, slot) in [
                ("checked", t.toggle.checked),
                ("unchecked", t.toggle.unchecked),
                ("hover", t.toggle.hover),
                ("disabled", t.toggle.disabled),
            ] {
                assert_ne!(slot.bg, Color32::TRANSPARENT, "{label} toggle.{state}.bg");
                assert_ne!(slot.border, Color32::TRANSPARENT, "{label} toggle.{state}.border");
                assert_ne!(slot.fg, Color32::TRANSPARENT, "{label} toggle.{state}.fg");
            }
            assert_ne!(t.toggle.thumb, Color32::TRANSPARENT, "{label} toggle.thumb");
            assert_ne!(t.toggle.mark, Color32::TRANSPARENT, "{label} toggle.mark");

            // Progress.
            assert_ne!(t.progress.track, Color32::TRANSPARENT, "{label} progress.track");
            assert_ne!(t.progress.fill, Color32::TRANSPARENT, "{label} progress.fill");
            assert_ne!(t.progress.label, Color32::TRANSPARENT, "{label} progress.label");
            assert_ne!(
                t.progress.label_on_fill,
                Color32::TRANSPARENT,
                "{label} progress.label_on_fill"
            );

            // Badge / kbd / tooltip single Slots.
            assert_ne!(t.badge.bg, Color32::TRANSPARENT, "{label} badge.bg");
            assert_ne!(t.badge.fg, Color32::TRANSPARENT, "{label} badge.fg");
            assert_ne!(t.kbd.bg, Color32::TRANSPARENT, "{label} kbd.bg");
            assert_ne!(t.kbd.fg, Color32::TRANSPARENT, "{label} kbd.fg");
            assert_ne!(t.kbd.border, Color32::TRANSPARENT, "{label} kbd.border");
            assert_ne!(t.tooltip.bg, Color32::TRANSPARENT, "{label} tooltip.bg");
            assert_ne!(t.tooltip.fg, Color32::TRANSPARENT, "{label} tooltip.fg");
            assert_ne!(t.tooltip.border, Color32::TRANSPARENT, "{label} tooltip.border");

            // Tag normal + hover.
            for (state, slot) in [("normal", t.tag.normal), ("hover", t.tag.hover)] {
                assert_ne!(slot.bg, Color32::TRANSPARENT, "{label} tag.{state}.bg");
                assert_ne!(slot.fg, Color32::TRANSPARENT, "{label} tag.{state}.fg");
                assert_ne!(slot.border, Color32::TRANSPARENT, "{label} tag.{state}.border");
            }

            // Alert: all four levels need a visible bg/accent/icon and fg.
            for (level, slots) in [
                ("info", t.alert.info),
                ("success", t.alert.success),
                ("warning", t.alert.warning),
                ("error", t.alert.error),
            ] {
                assert_ne!(slots.bg, Color32::TRANSPARENT, "{label} alert.{level}.bg");
                assert_ne!(slots.fg, Color32::TRANSPARENT, "{label} alert.{level}.fg");
                assert_ne!(slots.accent, Color32::TRANSPARENT, "{label} alert.{level}.accent");
                assert_ne!(slots.icon, Color32::TRANSPARENT, "{label} alert.{level}.icon");
            }

            // Toast: all four levels need visible bg/fg/border/accent.
            for (level, slots) in [
                ("info", t.toast.info),
                ("success", t.toast.success),
                ("warning", t.toast.warning),
                ("error", t.toast.error),
            ] {
                assert_ne!(slots.bg, Color32::TRANSPARENT, "{label} toast.{level}.bg");
                assert_ne!(slots.fg, Color32::TRANSPARENT, "{label} toast.{level}.fg");
                assert_ne!(slots.border, Color32::TRANSPARENT, "{label} toast.{level}.border");
                assert_ne!(slots.accent, Color32::TRANSPARENT, "{label} toast.{level}.accent");
            }

            // Skeleton / colour picker / easing curve.
            assert_ne!(t.skeleton.base, Color32::TRANSPARENT, "{label} skeleton.base");
            assert_ne!(t.skeleton.shimmer, Color32::TRANSPARENT, "{label} skeleton.shimmer");
            assert_ne!(
                t.color_picker.swatch_border,
                Color32::TRANSPARENT,
                "{label} color_picker.swatch_border"
            );
            assert_ne!(
                t.color_picker.swatch_hover,
                Color32::TRANSPARENT,
                "{label} color_picker.swatch_hover"
            );
            assert_eq!(t.color_picker.checker_light, Color32::WHITE, "{label} checker_light");
            assert_ne!(t.color_picker.checker_dark, Color32::TRANSPARENT, "{label} checker_dark");
            for (name, value) in [
                ("bg", t.easing_curve.bg),
                ("curve", t.easing_curve.curve),
                ("handle", t.easing_curve.handle),
                ("handle_active", t.easing_curve.handle_active),
            ] {
                assert_ne!(value, Color32::TRANSPARENT, "{label} easing_curve.{name}");
            }
            // The grid line is a low-alpha neutral; assert it is a real colour
            // (alpha > 0) rather than fully transparent.
            assert!(t.easing_curve.grid.a() > 0, "{label} easing_curve.grid alpha");
        }
    }

    /// The alert levels must keep their distinct status accents in both themes.
    #[test]
    fn alert_slots_use_distinct_status_accents() {
        for t in [Theme::dark(), Theme::light()] {
            assert_eq!(t.alert.success.accent, semantic::status::SUCCESS);
            assert_eq!(t.alert.warning.accent, semantic::status::WARNING);
            assert_eq!(t.alert.error.accent, semantic::status::ERROR);
            assert_eq!(t.alert.info.accent, semantic::status::INFO);
            // Info borrows the accent faint tint (no `info_faint` status token).
            assert_eq!(t.alert.info.bg, semantic::accent::faint());
        }
    }

    #[test]
    fn app_theme_choice_resolves() {
        // Explicit choices ignore the system hint.
        assert!(!AppThemeChoice::Light.is_dark(Some(true)));
        assert!(AppThemeChoice::Dark.is_dark(Some(false)));
        // Auto follows the system, dark-fallback when unknown.
        assert!(AppThemeChoice::Auto.is_dark(Some(true)));
        assert!(!AppThemeChoice::Auto.is_dark(Some(false)));
        assert!(AppThemeChoice::Auto.is_dark(None));
        // resolve() picks the matching Theme.
        assert_eq!(AppThemeChoice::Light.resolve(None).surface.base, Theme::light().surface.base);
        assert_eq!(
            AppThemeChoice::Auto.resolve(Some(true)).surface.base,
            Theme::dark().surface.base
        );
        assert_eq!(AppThemeChoice::default(), AppThemeChoice::Auto);
    }
}
