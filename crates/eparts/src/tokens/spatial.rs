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
pub mod component {
    pub const PILL_TAB_HEIGHT: f32 = 26.0;
    pub const PILL_TAB_GAP: f32 = 2.0;
    pub const TOAST_WIDTH: f32 = 280.0;
    pub const TOAST_HEIGHT: f32 = 40.0;
    pub const TOAST_SPACING: f32 = 8.0;
    pub const TOAST_MARGIN: f32 = 16.0;
    pub const ICON_SLOT_WIDTH: f32 = 14.0;
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
