//! Spatial tokens — spacing, row heights, stroke widths, radii,
//! and reusable component dimensions.
//!
//! Only the generic (domain-free) tokens live here.

// ── Unified spacing scale ──
pub const SPACE_0: f32 = 0.0;
pub const SPACE_1: f32 = 2.0;
pub const SPACE_2: f32 = 4.0;
pub const SPACE_3: f32 = 6.0;
pub const SPACE_4: f32 = 8.0;
pub const SPACE_5: f32 = 12.0;
pub const SPACE_6: f32 = 16.0;
pub const SPACE_7: f32 = 24.0;
pub const SPACE_8: f32 = 32.0;


// ── Row heights ──
pub const ROW_XS: f32 = 18.0;
pub const ROW_S: f32 = 20.0;
pub const ROW_M: f32 = 24.0;
pub const ROW_L: f32 = 28.0;

// ── Stroke widths ──
pub const STROKE_WIDTH: f32 = 1.0;
pub const STROKE_WIDTH_THICK: f32 = 1.5;
pub const STROKE_WIDTH_THIN: f32 = 0.5;

// ── Corner radii ──
pub const RADIUS_S: f32 = 2.0;
pub const RADIUS_M: f32 = 4.0;
pub const RADIUS_L: f32 = 6.0;
pub const RADIUS_XL: f32 = 8.0;

// ── Reusable component dimensions ──
pub mod toggle {
    /// Square size of the checkbox hit/display box (18 px).
    pub const CHECKBOX_SIZE: f32 = 18.0;
    /// Diameter of the radio button outer circle (16 px).
    pub const RADIO_SIZE: f32 = 16.0;
    /// Height of the switch track pill (20 px).
    pub const SWITCH_TRACK_HEIGHT: f32 = 20.0;
    /// Width of the switch track pill (36 px).
    pub const SWITCH_TRACK_WIDTH: f32 = 36.0;
    /// Radius of the switch thumb/knob circle (10 px).
    pub const SWITCH_THUMB_RADIUS: f32 = 10.0;
}

pub mod component {
    pub const PILL_TAB_HEIGHT: f32 = 26.0;
    pub const PILL_TAB_GAP: f32 = 2.0;
    pub const TOAST_WIDTH: f32 = 280.0;
    pub const TOAST_HEIGHT: f32 = 40.0;
    pub const TOAST_SPACING: f32 = 8.0;
    pub const TOAST_MARGIN: f32 = 16.0;
    pub const ICON_SLOT_WIDTH: f32 = 14.0;
    /// Height of the determinate progress bar (16 px).
    pub const PROGRESS_BAR_HEIGHT: f32 = 16.0;
}

// ── Menu layout constants (generic, used by context_menu widget) ──
pub mod menu {
    pub const MIN_WIDTH: f32 = 140.0;
    pub const ICON_WIDTH: f32 = 16.0;
    pub const CHECK_WIDTH: f32 = 14.0;
    pub const SHADOW_OFFSET_Y: i8 = 4;
    pub const SHADOW_BLUR: i8 = 12;
}

// ── Dialog layout constants (generic, used by dialog widget) ──
pub mod dialog {
    use super::SPACE_5;
    use super::SPACE_7;

    /// Inner margin applied around all dialog content (12px).
    pub const INNER_MARGIN: f32 = SPACE_5;
    /// Minimum gap between dialog edge and screen edge (24px).
    pub const SCREEN_MARGIN: f32 = SPACE_7;
    /// Maximum fraction of viewport the dialog may occupy.
    pub const MAX_VIEWPORT_FRAC: [f32; 2] = [0.85, 0.8];
    /// Vertical slide distance (px) for the open/close animation.
    pub const SLIDE_PX: f32 = 12.0;
}

// ═══════════════════════════════════════════════════════════════════════
// Density mode — runtime token-level scaling
// ═══════════════════════════════════════════════════════════════════════

use egui::Context;

// ── Preference enum ─────────────────────────────────────────────────

/// A user's display-density preference.
///
/// - `Default` — 1.0×; all tokens resolve to their base const values
///   (byte-identical to the pre-refactor behaviour).
/// - `Compact` — 0.875×; spacing, row-heights, toggle and component dims
///   are multiplied by 0.875 and rounded to whole logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Density {
    /// Standard density (1.0×).
    #[default]
    Default,
    /// Compact density (0.875×).
    Compact,
}

/// The egui Memory key used to store the density preference.
fn density_key() -> egui::Id {
    egui::Id::new("eparts_density")
}

/// Read the current [`Density`] from an `egui::Context`.
pub fn density_from_ctx(ctx: &Context) -> Density {
    ctx.data(|d| d.get_temp::<Density>(density_key())).unwrap_or_default()
}

/// Read the current [`Density`] from a [`egui::Ui`].
pub fn density(ui: &egui::Ui) -> Density {
    density_from_ctx(ui.ctx())
}

/// Store a new [`Density`] in the `egui::Context` Memory.
pub fn set_density(ctx: &Context, d: Density) {
    ctx.data_mut(|d2| d2.insert_temp(density_key(), d));
}

impl Density {
    /// The multiplicative factor for this density level.
    pub fn factor(self) -> f32 {
        match self {
            Density::Default => 1.0,
            Density::Compact => 0.875,
        }
    }

    /// Scale a spatial pixel value.
    ///
    /// `Default` returns the input unchanged (no multiply, no round) so
    /// Default is byte-identical to the base const. `Compact` multiplies by
    /// 0.875 then rounds to a whole logical pixel.
    pub fn scale(self, px: f32) -> f32 {
        match self {
            Density::Default => px,
            Density::Compact => (px * 0.875).round(),
        }
    }
}

// ── Scaled dims structs ─────────────────────────────────────────────

/// Toggle-control dimensions scaled by [`Density::scale`].
///
/// Mirrors the five size constants in the `toggle` submodule.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ToggleDims {
    pub checkbox_size: f32,
    pub radio_size: f32,
    pub switch_track_height: f32,
    pub switch_track_width: f32,
    pub switch_thumb_radius: f32,
}

impl ToggleDims {
    /// Build [`ToggleDims`] by scaling each base const for the given density.
    pub fn for_density(d: Density) -> Self {
        Self {
            checkbox_size: d.scale(toggle::CHECKBOX_SIZE),
            radio_size: d.scale(toggle::RADIO_SIZE),
            switch_track_height: d.scale(toggle::SWITCH_TRACK_HEIGHT),
            switch_track_width: d.scale(toggle::SWITCH_TRACK_WIDTH),
            switch_thumb_radius: d.scale(toggle::SWITCH_THUMB_RADIUS),
        }
    }
}

/// Component chrome dimensions scaled by [`Density::scale`].
///
/// Contains every field in the `component` submodule that should scale.
/// `TOAST_WIDTH` is intentionally excluded (content width; shrinking clips
/// text).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ComponentDims {
    pub pill_tab_height: f32,
    pub pill_tab_gap: f32,
    pub toast_height: f32,
    pub toast_spacing: f32,
    pub toast_margin: f32,
    pub icon_slot_width: f32,
    pub progress_bar_height: f32,
}

impl ComponentDims {
    /// Build [`ComponentDims`] by scaling each base const for the given density.
    pub fn for_density(d: Density) -> Self {
        Self {
            pill_tab_height: d.scale(component::PILL_TAB_HEIGHT),
            pill_tab_gap: d.scale(component::PILL_TAB_GAP),
            toast_height: d.scale(component::TOAST_HEIGHT),
            toast_spacing: d.scale(component::TOAST_SPACING),
            toast_margin: d.scale(component::TOAST_MARGIN),
            icon_slot_width: d.scale(component::ICON_SLOT_WIDTH),
            progress_bar_height: d.scale(component::PROGRESS_BAR_HEIGHT),
        }
    }
}

// ── Spatial resolved-struct ─────────────────────────────────────────

/// A fully-resolved, density-aware set of spatial tokens.
///
/// Read once per widget via `let s = spatial(ui);` (or
/// `let s = spatial_from_ctx(ctx);`) then access `s.space_3`, `s.row_m`,
/// `s.toggle.checkbox_size`, etc.
///
/// Non-scaling tokens (`STROKE_*`, `RADIUS_*`) remain as plain consts and
/// are **not** included here.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spatial {
    pub space_0: f32,
    pub space_1: f32,
    pub space_2: f32,
    pub space_3: f32,
    pub space_4: f32,
    pub space_5: f32,
    pub space_6: f32,
    pub space_7: f32,
    pub space_8: f32,
    pub row_xs: f32,
    pub row_s: f32,
    pub row_m: f32,
    pub row_l: f32,
    pub toggle: ToggleDims,
    pub component: ComponentDims,
}

impl Spatial {
    /// Resolve [`Spatial`] from a [`Density`], scaling every field from
    /// its base const.
    pub fn for_density(d: Density) -> Self {
        Self {
            space_0: d.scale(SPACE_0),
            space_1: d.scale(SPACE_1),
            space_2: d.scale(SPACE_2),
            space_3: d.scale(SPACE_3),
            space_4: d.scale(SPACE_4),
            space_5: d.scale(SPACE_5),
            space_6: d.scale(SPACE_6),
            space_7: d.scale(SPACE_7),
            space_8: d.scale(SPACE_8),
            row_xs: d.scale(ROW_XS),
            row_s: d.scale(ROW_S),
            row_m: d.scale(ROW_M),
            row_l: d.scale(ROW_L),
            toggle: ToggleDims::for_density(d),
            component: ComponentDims::for_density(d),
        }
    }
}

/// Resolve [`Spatial`] from a [`egui::Ui`].
pub fn spatial(ui: &egui::Ui) -> Spatial {
    Spatial::for_density(density_from_ctx(ui.ctx()))
}

/// Resolve [`Spatial`] from an [`egui::Context`].
pub fn spatial_from_ctx(ctx: &Context) -> Spatial {
    Spatial::for_density(density_from_ctx(ctx))
}

// ═══════════════════════════════════════════════════════════════════════
// Unit tests (§6 of the density-refactor plan)
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_density_matches_base_consts() {
        let s = Spatial::for_density(Density::Default);
        assert_eq!(s.space_0, SPACE_0);
        assert_eq!(s.space_1, SPACE_1);
        assert_eq!(s.space_2, SPACE_2);
        assert_eq!(s.space_3, SPACE_3);
        assert_eq!(s.space_4, SPACE_4);
        assert_eq!(s.space_5, SPACE_5);
        assert_eq!(s.space_6, SPACE_6);
        assert_eq!(s.space_7, SPACE_7);
        assert_eq!(s.space_8, SPACE_8);
        assert_eq!(s.row_xs, ROW_XS);
        assert_eq!(s.row_s, ROW_S);
        assert_eq!(s.row_m, ROW_M);
        assert_eq!(s.row_l, ROW_L);
        // scale() identity on Default for arbitrary values (incl. non-integers)
        for px in [0.0, 0.5, 1.5, 6.0, 13.0, 27.5] {
            assert_eq!(Density::Default.scale(px), px);
        }
    }

    #[test]
    fn compact_density_shrinks_and_rounds_to_whole_px() {
        let s = Spatial::for_density(Density::Compact);
        // space_8 must shrink from the base 32.0
        assert!(s.space_8 < SPACE_8);
        // Must equal the mathematically expected value
        assert_eq!(s.space_8, (SPACE_8 * 0.875).round()); // 28.0
        // Must be a whole pixel (no fractional remainder)
        assert_eq!(s.space_8.fract(), 0.0);
    }

    #[test]
    fn density_memory_round_trip() {
        let ctx = egui::Context::default();
        set_density(&ctx, Density::Compact);
        assert_eq!(density_from_ctx(&ctx), Density::Compact);
        // Writing Default and re-reading
        set_density(&ctx, Density::Default);
        assert_eq!(density_from_ctx(&ctx), Density::Default);
    }
}

