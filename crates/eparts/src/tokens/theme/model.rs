use egui::{Color32, CornerRadius, Shadow, Stroke, Visuals};

use crate::tokens::spatial::{RADIUS_M, STROKE_WIDTH};

use super::components::Components;
use super::elevation::Elevation;
use super::palette::Palette;

// ── Theme ─────────────────────────────────────────────────────────────

/// The runtime theme, grouped into palette / component / elevation scopes.
/// Each leaf is a `Color32` (or shadow); the struct is `Copy` so cloning is cheap.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Theme {
    /// Palette-scoped tokens (surfaces, text, accents, status, borders, overlays, lines).
    pub palette: Palette,
    /// Component-scoped color slots, grouped per widget family.
    pub components: Components,
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
    /// The single focus-ring color for this theme.
    ///
    /// All focusable widgets must use this one slot so the focus indicator is
    /// consistent across the UI. Do not add additional focus colors elsewhere.
    pub fn focus_ring(&self) -> Color32 {
        self.palette.border.focus
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

        v.panel_fill = self.palette.surface.panel;
        v.window_fill = self.palette.surface.panel;
        v.extreme_bg_color = self.palette.surface.base;
        v.faint_bg_color = self.palette.surface.surface;

        v.selection.bg_fill = self.palette.accent.selection;
        v.selection.stroke = Stroke::new(STROKE_WIDTH, self.palette.accent.primary);
        v.override_text_color = Some(self.palette.text.primary);

        let radius = CornerRadius::same(RADIUS_M as u8);

        let ni = &mut v.widgets.noninteractive;
        ni.bg_fill = self.palette.surface.surface;
        ni.weak_bg_fill = self.palette.surface.surface;
        ni.bg_stroke = Stroke::new(STROKE_WIDTH, self.palette.border.default);
        ni.fg_stroke = Stroke::new(STROKE_WIDTH, self.palette.text.secondary);
        ni.corner_radius = radius;

        let ina = &mut v.widgets.inactive;
        ina.bg_fill = self.palette.surface.widget;
        ina.weak_bg_fill = self.palette.surface.widget;
        ina.bg_stroke = Stroke::new(STROKE_WIDTH, self.palette.border.default);
        ina.fg_stroke = Stroke::new(STROKE_WIDTH, self.palette.text.primary);
        ina.corner_radius = radius;

        let hv = &mut v.widgets.hovered;
        hv.bg_fill = self.palette.surface.hover;
        hv.weak_bg_fill = self.palette.surface.hover;
        hv.bg_stroke = Stroke::new(STROKE_WIDTH, self.palette.accent.primary);
        hv.fg_stroke = Stroke::new(STROKE_WIDTH, self.palette.text.primary);
        hv.corner_radius = radius;

        let ac = &mut v.widgets.active;
        ac.bg_fill = self.palette.surface.active;
        ac.weak_bg_fill = self.palette.surface.active;
        ac.bg_stroke = Stroke::new(STROKE_WIDTH, self.palette.accent.primary);
        ac.fg_stroke = Stroke::new(STROKE_WIDTH, self.palette.text.primary);
        ac.corner_radius = radius;

        // Menus and combo boxes use the `open` state for their trigger frame.
        // Keep its geometry aligned with the other interactive states so opening
        // a popup cannot introduce a state-dependent layout shift.
        let op = &mut v.widgets.open;
        op.bg_fill = self.palette.surface.active;
        op.weak_bg_fill = self.palette.surface.active;
        op.bg_stroke = Stroke::new(STROKE_WIDTH, self.palette.accent.primary);
        op.fg_stroke = Stroke::new(STROKE_WIDTH, self.palette.text.primary);
        op.corner_radius = radius;
        op.expansion = 0.0;

        v
    }
}
