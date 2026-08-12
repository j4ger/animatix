//! JSON theme loading and schema support (`theme-json` feature).
//!
//! The serialized format keeps the runtime [`Theme`] representation as a DTO:
//! colors are hex strings (`#rrggbb` or `#rrggbbaa`) and shadows are small
//! objects (`{ "offset": [x, y], "blur": n, "spread": n, "color": "#..." }`).
//!
//! A theme file can carry both modes:
//! ```json
//! {
//!   "name": "My Theme",
//!   "dark": { "surface": { "base": "#202124" }, ... },
//!   "light": { "surface": { "base": "#f8f9fa" }, ... }
//! }
//! ```

use std::path::Path;

use egui::Shadow;

use super::theme::{Theme, serde_color32, serde_shadow};
use serde::{Deserialize, Serialize};

/// A single dark/light theme file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeFile {
    /// Optional display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Dark-mode theme. Omitted fields fall back to [`Theme::dark`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dark: Option<PartialTheme>,
    /// Light-mode theme. Omitted fields fall back to [`Theme::light`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<PartialTheme>,
}

/// A partially-specified theme. Every field is optional; `apply_to` fills the
/// remainder from a base theme, which makes a JSON theme a small override file
/// instead of a full re-export of every token.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialTheme {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<PartialSurface>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<PartialText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<PartialAccent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<PartialStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<PartialBorder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay: Option<PartialOverlay>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<PartialLines>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button: Option<PartialButtonSlots>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list: Option<PartialListSlots>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab: Option<PartialTabSlots>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_item: Option<PartialMenuItemSlots>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<PartialInputSlots>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scrollbar: Option<PartialScrollbarSlots>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elevation: Option<PartialElevation>,
}

// The `Color32Field` one-field wrapper keeps `serde_color32` compatible with
// optional fields while the helper remains available for non-optional colors.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color32Field(pub egui::Color32);

impl Serialize for Color32Field {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serde_color32::serialize(&self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for Color32Field {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self(serde_color32::deserialize(deserializer)?))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialSlot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<Color32Field>,
}

impl PartialSlot {
    pub fn apply_to(self, target: &mut super::theme::Slot) {
        if let Some(bg) = self.bg {
            target.bg = bg.0;
        }
        if let Some(fg) = self.fg {
            target.fg = fg.0;
        }
        if let Some(border) = self.border {
            target.border = border.0;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialFill {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<Color32Field>,
}

impl PartialFill {
    pub fn apply_to(self, target: &mut super::theme::Fill) {
        if let Some(bg) = self.bg {
            target.bg = bg.0;
        }
        if let Some(fg) = self.fg {
            target.fg = fg.0;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialButtonStateSlots {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normal: Option<PartialSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hover: Option<PartialSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<PartialSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<PartialSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<PartialSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<PartialSlot>,
}

impl PartialButtonStateSlots {
    pub fn apply_to(self, target: &mut super::theme::ButtonStateSlots) {
        if let Some(v) = self.normal {
            v.apply_to(&mut target.normal);
        }
        if let Some(v) = self.hover {
            v.apply_to(&mut target.hover);
        }
        if let Some(v) = self.active {
            v.apply_to(&mut target.active);
        }
        if let Some(v) = self.selected {
            v.apply_to(&mut target.selected);
        }
        if let Some(v) = self.disabled {
            v.apply_to(&mut target.disabled);
        }
        if let Some(v) = self.focus {
            v.apply_to(&mut target.focus);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialButtonSlots {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<PartialButtonStateSlots>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<PartialButtonStateSlots>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ghost: Option<PartialButtonStateSlots>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<PartialButtonStateSlots>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub danger: Option<PartialButtonStateSlots>,
}

impl PartialButtonSlots {
    pub fn apply_to(self, target: &mut super::theme::ButtonSlots) {
        if let Some(v) = self.primary {
            v.apply_to(&mut target.primary);
        }
        if let Some(v) = self.secondary {
            v.apply_to(&mut target.secondary);
        }
        if let Some(v) = self.ghost {
            v.apply_to(&mut target.ghost);
        }
        if let Some(v) = self.icon {
            v.apply_to(&mut target.icon);
        }
        if let Some(v) = self.danger {
            v.apply_to(&mut target.danger);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialListSlots {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub even: Option<PartialFill>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub odd: Option<PartialFill>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<PartialFill>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hover: Option<PartialFill>,
}

impl PartialListSlots {
    pub fn apply_to(self, target: &mut super::theme::ListSlots) {
        if let Some(v) = self.even {
            v.apply_to(&mut target.even);
        }
        if let Some(v) = self.odd {
            v.apply_to(&mut target.odd);
        }
        if let Some(v) = self.selected {
            v.apply_to(&mut target.selected);
        }
        if let Some(v) = self.hover {
            v.apply_to(&mut target.hover);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialTabSlot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indicator: Option<Color32Field>,
}

impl PartialTabSlot {
    pub fn apply_to(self, target: &mut super::theme::TabSlot) {
        if let Some(bg) = self.bg {
            target.bg = bg.0;
        }
        if let Some(fg) = self.fg {
            target.fg = fg.0;
        }
        if let Some(indicator) = self.indicator {
            target.indicator = indicator.0;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialTabSlots {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<PartialTabSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inactive: Option<PartialTabSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hover: Option<PartialTabSlot>,
}

impl PartialTabSlots {
    pub fn apply_to(self, target: &mut super::theme::TabSlots) {
        if let Some(v) = self.active {
            v.apply_to(&mut target.active);
        }
        if let Some(v) = self.inactive {
            v.apply_to(&mut target.inactive);
        }
        if let Some(v) = self.hover {
            v.apply_to(&mut target.hover);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialMenuItemSlots {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normal: Option<PartialSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hover: Option<PartialSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<PartialSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<PartialSlot>,
}

impl PartialMenuItemSlots {
    pub fn apply_to(self, target: &mut super::theme::MenuItemSlots) {
        if let Some(v) = self.normal {
            v.apply_to(&mut target.normal);
        }
        if let Some(v) = self.hover {
            v.apply_to(&mut target.hover);
        }
        if let Some(v) = self.active {
            v.apply_to(&mut target.active);
        }
        if let Some(v) = self.disabled {
            v.apply_to(&mut target.disabled);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialInputSlots {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normal: Option<PartialSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hover: Option<PartialSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<PartialSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invalid: Option<PartialSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<PartialSlot>,
}

impl PartialInputSlots {
    pub fn apply_to(self, target: &mut super::theme::InputSlots) {
        if let Some(v) = self.normal {
            v.apply_to(&mut target.normal);
        }
        if let Some(v) = self.hover {
            v.apply_to(&mut target.hover);
        }
        if let Some(v) = self.focus {
            v.apply_to(&mut target.focus);
        }
        if let Some(v) = self.invalid {
            v.apply_to(&mut target.invalid);
        }
        if let Some(v) = self.disabled {
            v.apply_to(&mut target.disabled);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialScrollbarSlots {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb_hover: Option<Color32Field>,
}

impl PartialScrollbarSlots {
    pub fn apply_to(self, target: &mut super::theme::ScrollbarSlots) {
        if let Some(v) = self.thumb {
            target.thumb = v.0;
        }
        if let Some(v) = self.thumb_hover {
            target.thumb_hover = v.0;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShadowField(pub Shadow);

impl Serialize for ShadowField {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serde_shadow::serialize(&self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for ShadowField {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self(serde_shadow::deserialize(deserializer)?))
    }
}

impl From<Shadow> for ShadowField {
    fn from(value: Shadow) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialElevation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raised: Option<ShadowField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay: Option<ShadowField>,
}

impl PartialElevation {
    pub fn apply_to(self, target: &mut super::theme::Elevation) {
        if let Some(v) = self.raised {
            target.raised = v.0;
        }
        if let Some(v) = self.overlay {
            target.overlay = v.0;
        }
    }
}

// These partial structs mirror the full Theme groups but every color is
// optional. They are intentionally hand-written rather than macro-generated
// so serde field names and docs stay reviewable.

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialSurface {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hover: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floating_card_bg: Option<Color32Field>,
}

impl PartialSurface {
    pub fn apply_to(self, target: &mut super::theme::Surface) {
        if let Some(v) = self.base {
            target.base = v.0;
        }
        if let Some(v) = self.panel {
            target.panel = v.0;
        }
        if let Some(v) = self.surface {
            target.surface = v.0;
        }
        if let Some(v) = self.widget {
            target.widget = v.0;
        }
        if let Some(v) = self.hover {
            target.hover = v.0;
        }
        if let Some(v) = self.active {
            target.active = v.0;
        }
        if let Some(v) = self.floating_card_bg {
            target.floating_card_bg = v.0;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialText {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_accent: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faint: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtle: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hover: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dim: Option<Color32Field>,
}

impl PartialText {
    pub fn apply_to(self, target: &mut super::theme::Text) {
        if let Some(v) = self.primary {
            target.primary = v.0;
        }
        if let Some(v) = self.secondary {
            target.secondary = v.0;
        }
        if let Some(v) = self.muted {
            target.muted = v.0;
        }
        if let Some(v) = self.disabled {
            target.disabled = v.0;
        }
        if let Some(v) = self.on_accent {
            target.on_accent = v.0;
        }
        if let Some(v) = self.faint {
            target.faint = v.0;
        }
        if let Some(v) = self.subtle {
            target.subtle = v.0;
        }
        if let Some(v) = self.hover {
            target.hover = v.0;
        }
        if let Some(v) = self.dim {
            target.dim = v.0;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialAccent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cyan: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_hover: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_active: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faint: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ghost: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtle: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hover: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strong: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<Color32Field>,
}

impl PartialAccent {
    pub fn apply_to(self, target: &mut super::theme::Accent) {
        if let Some(v) = self.primary {
            target.primary = v.0;
        }
        if let Some(v) = self.cyan {
            target.cyan = v.0;
        }
        if let Some(v) = self.primary_hover {
            target.primary_hover = v.0;
        }
        if let Some(v) = self.primary_active {
            target.primary_active = v.0;
        }
        if let Some(v) = self.faint {
            target.faint = v.0;
        }
        if let Some(v) = self.ghost {
            target.ghost = v.0;
        }
        if let Some(v) = self.subtle {
            target.subtle = v.0;
        }
        if let Some(v) = self.hover {
            target.hover = v.0;
        }
        if let Some(v) = self.strong {
            target.strong = v.0;
        }
        if let Some(v) = self.selection {
            target.selection = v.0;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playing_text: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic_error: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic_warning: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success_faint: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success_ultra_faint: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning_subtle: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_faint: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_ultra_faint: Option<Color32Field>,
}

impl PartialStatus {
    pub fn apply_to(self, target: &mut super::theme::Status) {
        if let Some(v) = self.success {
            target.success = v.0;
        }
        if let Some(v) = self.warning {
            target.warning = v.0;
        }
        if let Some(v) = self.error {
            target.error = v.0;
        }
        if let Some(v) = self.info {
            target.info = v.0;
        }
        if let Some(v) = self.playing_text {
            target.playing_text = v.0;
        }
        if let Some(v) = self.diagnostic_error {
            target.diagnostic_error = v.0;
        }
        if let Some(v) = self.diagnostic_warning {
            target.diagnostic_warning = v.0;
        }
        if let Some(v) = self.success_faint {
            target.success_faint = v.0;
        }
        if let Some(v) = self.success_ultra_faint {
            target.success_ultra_faint = v.0;
        }
        if let Some(v) = self.warning_subtle {
            target.warning_subtle = v.0;
        }
        if let Some(v) = self.error_faint {
            target.error_faint = v.0;
        }
        if let Some(v) = self.error_ultra_faint {
            target.error_ultra_faint = v.0;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialBorder {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strong: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<Color32Field>,
}

impl PartialBorder {
    pub fn apply_to(self, target: &mut super::theme::Border) {
        if let Some(v) = self.default {
            target.default = v.0;
        }
        if let Some(v) = self.strong {
            target.strong = v.0;
        }
        if let Some(v) = self.focus {
            target.focus = v.0;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialOverlay {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backdrop: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge_bg: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip_bg: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_ambient: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_direct: Option<Color32Field>,
}

impl PartialOverlay {
    pub fn apply_to(self, target: &mut super::theme::Overlay) {
        if let Some(v) = self.backdrop {
            target.backdrop = v.0;
        }
        if let Some(v) = self.badge_bg {
            target.badge_bg = v.0;
        }
        if let Some(v) = self.tooltip_bg {
            target.tooltip_bg = v.0;
        }
        if let Some(v) = self.shadow_ambient {
            target.shadow_ambient = v.0;
        }
        if let Some(v) = self.shadow_direct {
            target.shadow_direct = v.0;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialLines {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<Color32Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide: Option<Color32Field>,
}

impl PartialLines {
    pub fn apply_to(self, target: &mut super::theme::Lines) {
        if let Some(v) = self.grid {
            target.grid = v.0;
        }
        if let Some(v) = self.guide {
            target.guide = v.0;
        }
    }
}

impl PartialTheme {
    /// Apply this partial theme over `base`, returning the merged theme.
    pub fn apply_to(self, base: Theme) -> Theme {
        let mut theme = base;
        if let Some(v) = self.surface {
            v.apply_to(&mut theme.surface);
        }
        if let Some(v) = self.text {
            v.apply_to(&mut theme.text);
        }
        if let Some(v) = self.accent {
            v.apply_to(&mut theme.accent);
        }
        if let Some(v) = self.status {
            v.apply_to(&mut theme.status);
        }
        if let Some(v) = self.border {
            v.apply_to(&mut theme.border);
        }
        if let Some(v) = self.overlay {
            v.apply_to(&mut theme.overlay);
        }
        if let Some(v) = self.lines {
            v.apply_to(&mut theme.lines);
        }
        if let Some(v) = self.button {
            v.apply_to(&mut theme.button);
        }
        if let Some(v) = self.list {
            v.apply_to(&mut theme.list);
        }
        if let Some(v) = self.tab {
            v.apply_to(&mut theme.tab);
        }
        if let Some(v) = self.menu_item {
            v.apply_to(&mut theme.menu_item);
        }
        if let Some(v) = self.input {
            v.apply_to(&mut theme.input);
        }
        if let Some(v) = self.scrollbar {
            v.apply_to(&mut theme.scrollbar);
        }
        if let Some(v) = self.elevation {
            v.apply_to(&mut theme.elevation);
        }
        theme
    }

    /// Convert a full runtime theme into a partial override with every field
    /// populated. This is useful for canonical export files.
    pub fn full_from(theme: Theme) -> Self {
        Self {
            surface: Some(PartialSurface {
                base: Some(theme.surface.base.into()),
                panel: Some(theme.surface.panel.into()),
                surface: Some(theme.surface.surface.into()),
                widget: Some(theme.surface.widget.into()),
                hover: Some(theme.surface.hover.into()),
                active: Some(theme.surface.active.into()),
                floating_card_bg: Some(theme.surface.floating_card_bg.into()),
            }),
            text: Some(PartialText {
                primary: Some(theme.text.primary.into()),
                secondary: Some(theme.text.secondary.into()),
                muted: Some(theme.text.muted.into()),
                disabled: Some(theme.text.disabled.into()),
                on_accent: Some(theme.text.on_accent.into()),
                faint: Some(theme.text.faint.into()),
                subtle: Some(theme.text.subtle.into()),
                hover: Some(theme.text.hover.into()),
                dim: Some(theme.text.dim.into()),
            }),
            accent: Some(PartialAccent {
                primary: Some(theme.accent.primary.into()),
                cyan: Some(theme.accent.cyan.into()),
                primary_hover: Some(theme.accent.primary_hover.into()),
                primary_active: Some(theme.accent.primary_active.into()),
                faint: Some(theme.accent.faint.into()),
                ghost: Some(theme.accent.ghost.into()),
                subtle: Some(theme.accent.subtle.into()),
                hover: Some(theme.accent.hover.into()),
                strong: Some(theme.accent.strong.into()),
                selection: Some(theme.accent.selection.into()),
            }),
            status: Some(PartialStatus {
                success: Some(theme.status.success.into()),
                warning: Some(theme.status.warning.into()),
                error: Some(theme.status.error.into()),
                info: Some(theme.status.info.into()),
                playing_text: Some(theme.status.playing_text.into()),
                diagnostic_error: Some(theme.status.diagnostic_error.into()),
                diagnostic_warning: Some(theme.status.diagnostic_warning.into()),
                success_faint: Some(theme.status.success_faint.into()),
                success_ultra_faint: Some(theme.status.success_ultra_faint.into()),
                warning_subtle: Some(theme.status.warning_subtle.into()),
                error_faint: Some(theme.status.error_faint.into()),
                error_ultra_faint: Some(theme.status.error_ultra_faint.into()),
            }),
            border: Some(PartialBorder {
                default: Some(theme.border.default.into()),
                strong: Some(theme.border.strong.into()),
                focus: Some(theme.border.focus.into()),
            }),
            overlay: Some(PartialOverlay {
                backdrop: Some(theme.overlay.backdrop.into()),
                badge_bg: Some(theme.overlay.badge_bg.into()),
                tooltip_bg: Some(theme.overlay.tooltip_bg.into()),
                shadow_ambient: Some(theme.overlay.shadow_ambient.into()),
                shadow_direct: Some(theme.overlay.shadow_direct.into()),
            }),
            lines: Some(PartialLines {
                grid: Some(theme.lines.grid.into()),
                guide: Some(theme.lines.guide.into()),
            }),
            button: Some(partial_button_slots(&theme.button)),
            list: Some(PartialListSlots {
                even: Some(partial_fill(theme.list.even)),
                odd: Some(partial_fill(theme.list.odd)),
                selected: Some(partial_fill(theme.list.selected)),
                hover: Some(partial_fill(theme.list.hover)),
            }),
            tab: Some(PartialTabSlots {
                active: Some(partial_tab_slot(theme.tab.active)),
                inactive: Some(partial_tab_slot(theme.tab.inactive)),
                hover: Some(partial_tab_slot(theme.tab.hover)),
            }),
            menu_item: Some(PartialMenuItemSlots {
                normal: Some(partial_slot(theme.menu_item.normal)),
                hover: Some(partial_slot(theme.menu_item.hover)),
                active: Some(partial_slot(theme.menu_item.active)),
                disabled: Some(partial_slot(theme.menu_item.disabled)),
            }),
            input: Some(PartialInputSlots {
                normal: Some(partial_slot(theme.input.normal)),
                hover: Some(partial_slot(theme.input.hover)),
                focus: Some(partial_slot(theme.input.focus)),
                invalid: Some(partial_slot(theme.input.invalid)),
                disabled: Some(partial_slot(theme.input.disabled)),
            }),
            scrollbar: Some(PartialScrollbarSlots {
                thumb: Some(theme.scrollbar.thumb.into()),
                thumb_hover: Some(theme.scrollbar.thumb_hover.into()),
            }),
            elevation: Some(PartialElevation {
                raised: Some(theme.elevation.raised.into()),
                overlay: Some(theme.elevation.overlay.into()),
            }),
        }
    }
}

impl ThemeFile {
    /// Build a `ThemeFile` from the built-in dark and light themes.
    pub fn builtin() -> Self {
        Self {
            name: Some("Animatix".to_string()),
            dark: Some(PartialTheme::full_from(Theme::dark())),
            light: Some(PartialTheme::full_from(Theme::light())),
        }
    }

    /// Resolve the dark theme, falling back to built-in dark.
    pub fn dark_theme(&self) -> Theme {
        self.dark.clone().unwrap_or_default().apply_to(Theme::dark())
    }

    /// Resolve the light theme, falling back to built-in light.
    pub fn light_theme(&self) -> Theme {
        self.light.clone().unwrap_or_default().apply_to(Theme::light())
    }

    /// Parse a theme file from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize this theme file as pretty JSON.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Load a theme file from disk.
    pub fn load(path: &Path) -> Result<Self, ThemeJsonError> {
        let source = std::fs::read_to_string(path).map_err(ThemeJsonError::Io)?;
        Self::from_json(&source).map_err(ThemeJsonError::Json)
    }

    /// Save this theme file to disk.
    pub fn save(&self, path: &Path) -> Result<(), ThemeJsonError> {
        let source = self.to_json_pretty().map_err(ThemeJsonError::Json)?;
        std::fs::write(path, source).map_err(ThemeJsonError::Io)
    }
}

impl From<egui::Color32> for Color32Field {
    fn from(value: egui::Color32) -> Self {
        Self(value)
    }
}

fn partial_slot(slot: super::theme::Slot) -> PartialSlot {
    PartialSlot {
        bg: Some(slot.bg.into()),
        fg: Some(slot.fg.into()),
        border: Some(slot.border.into()),
    }
}

fn partial_fill(fill: super::theme::Fill) -> PartialFill {
    PartialFill {
        bg: Some(fill.bg.into()),
        fg: Some(fill.fg.into()),
    }
}

fn partial_tab_slot(slot: super::theme::TabSlot) -> PartialTabSlot {
    PartialTabSlot {
        bg: Some(slot.bg.into()),
        fg: Some(slot.fg.into()),
        indicator: Some(slot.indicator.into()),
    }
}

fn partial_button_state(state: super::theme::ButtonStateSlots) -> PartialButtonStateSlots {
    PartialButtonStateSlots {
        normal: Some(partial_slot(state.normal)),
        hover: Some(partial_slot(state.hover)),
        active: Some(partial_slot(state.active)),
        selected: Some(partial_slot(state.selected)),
        disabled: Some(partial_slot(state.disabled)),
        focus: Some(partial_slot(state.focus)),
    }
}

fn partial_button_slots(slots: &super::theme::ButtonSlots) -> PartialButtonSlots {
    PartialButtonSlots {
        primary: Some(partial_button_state(slots.primary)),
        secondary: Some(partial_button_state(slots.secondary)),
        ghost: Some(partial_button_state(slots.ghost)),
        icon: Some(partial_button_state(slots.icon)),
        danger: Some(partial_button_state(slots.danger)),
    }
}

/// A small error type for theme file I/O and JSON parsing.
#[derive(Debug, thiserror::Error)]
pub enum ThemeJsonError {
    #[error("failed to read theme file: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid theme JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// JSON Schema for the theme file format.
///
/// The schema is intentionally permissive at the root (string values for every
/// color field) and does not enumerate every nested token individually; editors
/// can use it for basic validation and auto-completion while the Rust DTO
/// remains the authoritative parser.
pub fn theme_schema_json() -> String {
    let schema = serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Animatix Theme",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "name": { "type": "string" },
            "dark": { "$ref": "#/$defs/theme" },
            "light": { "$ref": "#/$defs/theme" }
        },
        "$defs": {
            "theme": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "surface": { "$ref": "#/$defs/colorGroup" },
                    "text": { "$ref": "#/$defs/colorGroup" },
                    "accent": { "$ref": "#/$defs/colorGroup" },
                    "status": { "$ref": "#/$defs/colorGroup" },
                    "border": { "$ref": "#/$defs/colorGroup" },
                    "overlay": { "$ref": "#/$defs/colorGroup" },
                    "lines": { "$ref": "#/$defs/colorGroup" },
                    "button": { "$ref": "#/$defs/buttonSlots" },
                    "list": { "$ref": "#/$defs/fillSlots" },
                    "tab": { "$ref": "#/$defs/tabSlots" },
                    "menu_item": { "$ref": "#/$defs/slotSlots" },
                    "input": { "$ref": "#/$defs/slotSlots" },
                    "scrollbar": { "$ref": "#/$defs/scrollbar" },
                    "elevation": { "$ref": "#/$defs/elevation" }
                }
            },
            "colorGroup": {
                "type": "object",
                "additionalProperties": false,
                "patternProperties": {
                    "^[a-z_]+$": { "type": "string" }
                }
            },
            "slot": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "bg": { "type": "string" },
                    "fg": { "type": "string" },
                    "border": { "type": "string" }
                }
            },
            "fill": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "bg": { "type": "string" },
                    "fg": { "type": "string" }
                }
            },
            "tab": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "bg": { "type": "string" },
                    "fg": { "type": "string" },
                    "indicator": { "type": "string" }
                }
            },
            "buttonState": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "normal": { "$ref": "#/$defs/slot" },
                    "hover": { "$ref": "#/$defs/slot" },
                    "active": { "$ref": "#/$defs/slot" },
                    "selected": { "$ref": "#/$defs/slot" },
                    "disabled": { "$ref": "#/$defs/slot" },
                    "focus": { "$ref": "#/$defs/slot" }
                }
            },
            "buttonSlots": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "primary": { "$ref": "#/$defs/buttonState" },
                    "secondary": { "$ref": "#/$defs/buttonState" },
                    "ghost": { "$ref": "#/$defs/buttonState" },
                    "icon": { "$ref": "#/$defs/buttonState" },
                    "danger": { "$ref": "#/$defs/buttonState" }
                }
            },
            "fillSlots": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "even": { "$ref": "#/$defs/fill" },
                    "odd": { "$ref": "#/$defs/fill" },
                    "selected": { "$ref": "#/$defs/fill" },
                    "hover": { "$ref": "#/$defs/fill" }
                }
            },
            "tabSlots": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "active": { "$ref": "#/$defs/tab" },
                    "inactive": { "$ref": "#/$defs/tab" },
                    "hover": { "$ref": "#/$defs/tab" }
                }
            },
            "slotSlots": {
                "type": "object",
                "additionalProperties": false,
                "patternProperties": {
                    "^[a-z_]+$": { "$ref": "#/$defs/slot" }
                }
            },
            "scrollbar": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "thumb": { "type": "string" },
                    "thumb_hover": { "type": "string" }
                }
            },
            "elevation": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "raised": { "$ref": "#/$defs/shadow" },
                    "overlay": { "$ref": "#/$defs/shadow" }
                }
            },
            "shadow": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "offset": {
                        "type": "array",
                        "items": { "type": "integer" },
                        "minItems": 2,
                        "maxItems": 2
                    },
                    "blur": { "type": "integer" },
                    "spread": { "type": "integer" },
                    "color": { "type": "string" }
                }
            }
        }
    });
    serde_json::to_string_pretty(&schema).expect("theme schema is valid JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_theme_file_roundtrips_both_modes() {
        let file = ThemeFile::builtin();
        let json = file.to_json_pretty().expect("serialize");
        let parsed = ThemeFile::from_json(&json).expect("parse");
        assert_eq!(parsed.dark_theme(), Theme::dark());
        assert_eq!(parsed.light_theme(), Theme::light());
        assert_eq!(parsed.name.as_deref(), Some("Animatix"));
    }

    #[test]
    fn partial_override_keeps_unset_dark_tokens() {
        let json = r##"{
            "name": "Custom",
            "dark": { "surface": { "base": "#112233" } }
        }"##;
        let file = ThemeFile::from_json(json).expect("parse");
        let theme = file.dark_theme();
        assert_eq!(theme.surface.base, egui::Color32::from_rgb(0x11, 0x22, 0x33));
        assert_eq!(theme.surface.panel, Theme::dark().surface.panel);
        assert_eq!(theme.text.primary, Theme::dark().text.primary);
        assert_eq!(theme.button.primary.normal.bg, Theme::dark().button.primary.normal.bg);
        assert_eq!(theme.elevation.raised, Theme::dark().elevation.raised);
    }

    #[test]
    fn nested_slots_and_shadows_roundtrip() {
        let json = r##"{
            "dark": {
                "button": { "primary": { "hover": { "bg": "#ff0000" } } },
                "elevation": { "raised": { "offset": [0, 4], "blur": 8, "spread": 1, "color": "#000000aa" } }
            }
        }"##;
        let file = ThemeFile::from_json(json).expect("parse");
        let theme = file.dark_theme();
        assert_eq!(theme.button.primary.hover.bg, egui::Color32::from_rgb(0xff, 0, 0));
        assert_eq!(theme.elevation.raised.offset, [0, 4]);
        assert_eq!(theme.elevation.raised.blur, 8);
        assert_eq!(theme.elevation.raised.spread, 1);
        assert_eq!(
            theme.elevation.raised.color,
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 0xaa)
        );
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let json = r##"{ "dark": { "unknown_token": "#ff0000" } }"##;
        assert!(ThemeFile::from_json(json).is_err());
    }

    #[test]
    fn invalid_color_is_rejected() {
        let json = r##"{ "dark": { "surface": { "base": "not-a-color" } } }"##;
        assert!(ThemeFile::from_json(json).is_err());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("eparts-theme-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("theme.json");
        let file = ThemeFile::builtin();
        file.save(&path).expect("save");
        let loaded = ThemeFile::load(&path).expect("load");
        assert_eq!(loaded.dark_theme(), file.dark_theme());
        assert_eq!(loaded.light_theme(), file.light_theme());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn schema_is_valid_json_and_describes_theme_file() {
        let schema = theme_schema_json();
        let value: serde_json::Value = serde_json::from_str(&schema).expect("valid JSON");
        assert_eq!(value["$schema"], "https://json-schema.org/draft/2020-12/schema");
        assert!(value["$defs"]["theme"].is_object());
        assert!(value["$defs"]["elevation"].is_object());
    }
}
