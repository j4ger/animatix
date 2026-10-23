use egui::Color32;

use super::slots::{
    AlertLevelSlots, AlertSlots, ButtonSlots, ColorPickerSlots, EasingCurveSlots, InputSlots,
    ListSlots, MenuItemSlots, ProgressSlots, ScrollbarSlots, SkeletonSlots, Slot, TabSlots,
    TagSlots, ToastLevelSlots, ToastSlots, ToggleSlots,
};

/// Component-scoped color slots, one group per themed widget family.
///
/// Accessed as `theme.components.button.primary.normal.bg`,
/// `theme.components.tag.hover.fg`, …
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Components {
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
}

// ── Per-component slot seeding (B) ────────────────────────────────────

/// The 11 per-component slot groups, bundled so both `Theme::dark()` and
/// `Theme::light()` can share one set of `Self { .. }` assignments.
pub(super) struct ComponentSlots {
    pub(super) toggle: ToggleSlots,
    pub(super) progress: ProgressSlots,
    pub(super) badge: Slot,
    pub(super) tag: TagSlots,
    pub(super) kbd: Slot,
    pub(super) tooltip: Slot,
    pub(super) alert: AlertSlots,
    pub(super) toast: ToastSlots,
    pub(super) skeleton: SkeletonSlots,
    pub(super) color_picker: ColorPickerSlots,
    pub(super) easing_curve: EasingCurveSlots,
}

/// Seed the per-level alert colors from the shared status roles.
///
/// Info borrows the accent `faint` tint (there is no `info_faint` status token);
/// every level's icon/accent uses its own status role.
pub(super) fn alert_level_slots(bg: Color32, accent: Color32, fg: Color32) -> AlertLevelSlots {
    AlertLevelSlots {
        bg,
        fg,
        accent,
        icon: accent,
    }
}

pub(super) fn toast_level_slots(
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
