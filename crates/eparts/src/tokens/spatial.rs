//! Spatial tokens — spacing, row heights, stroke widths, radii,
//! and reusable component dimensions.
//!
//! Only the generic (domain-free) tokens live here. App-specific submodules
//! (`preview`, `timeline`, `inspector`, `menu`, `toolbar`, `welcome`, `dialog`)
//! are defined in `animatix-gui/src/app/design_tokens/spatial.rs` and
//! re-exported alongside these generic constants so that existing paths like
//! `design_tokens::spatial::SPACE_4` continue to resolve.

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

// ── Legacy naming aliases (Phase 1 migration compatibility) ──
// These will be removed once all call sites use the new SPACE_N scale.
pub const SPACE_XS: f32 = SPACE_1;
pub const SPACE_S: f32 = SPACE_2;
pub const SPACE_M: f32 = SPACE_3;
pub const SPACE_L: f32 = SPACE_4;
pub const SPACE_XL: f32 = SPACE_5;
pub const PAD_XS: f32 = SPACE_1;
pub const PAD_S: f32 = SPACE_2;
pub const PAD_M: f32 = SPACE_3;
pub const PAD_L: f32 = SPACE_4;
pub const PAD_XL: f32 = SPACE_5;
pub const PAD_XXL: f32 = SPACE_6;

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
pub mod component {
    pub const PILL_TAB_HEIGHT: f32 = 26.0;
    pub const PILL_TAB_GAP: f32 = 2.0;
    pub const TOAST_WIDTH: f32 = 280.0;
    pub const TOAST_HEIGHT: f32 = 40.0;
    pub const TOAST_SPACING: f32 = 8.0;
    pub const TOAST_MARGIN: f32 = 16.0;
    pub const ICON_SLOT_WIDTH: f32 = 14.0;
}
