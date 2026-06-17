//! Spatial tokens — spacing, row heights, stroke widths, radii,
//! and domain-specific layout dimensions.

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

// ── Preview canvas ──
pub mod preview {
    pub const ROTATION_OFFSET: f32 = 20.0;
    pub const ROTATION_RADIUS: f32 = 4.0;
    pub const HANDLE_SIZE: f32 = 6.0;
    pub const HANDLE_HIT_RADIUS: f32 = 10.0;
    pub const MIN_ACTOR_SIZE: f32 = 10.0;
    pub const MIN_SCALE: f32 = 0.01;
    pub const MIN_ZOOM: f32 = 0.01;
    pub const DASH_LEN: f32 = 6.0;
    pub const GAP_LEN: f32 = 4.0;
    pub const CROSS_SIZE: f32 = 6.0;
    pub const VERTEX_HIT_BUFFER: f32 = 2.0;
    pub const ROTATION_HIT_BUFFER: f32 = 4.0;
}

// ── Toolbar ──
pub mod toolbar {
    pub const HEIGHT: f32 = 28.0;
}

// ── Timeline ──
pub mod timeline {
    pub const LABEL_COL_WIDTH: f32 = 120.0;
    pub const TRACK_ROW_HEIGHT: f32 = 24.0;
    pub const RULER_HEIGHT: f32 = 22.0;
    pub const RANGE_HEIGHT: f32 = 20.0;
    pub const KF_HALF: f32 = 4.0;
    pub const PLAYBACK_STRIP_HEIGHT: f32 = 28.0;
}

// ── Context menu ──
pub mod menu {
    pub const MIN_WIDTH: f32 = 140.0;
    pub const ICON_WIDTH: f32 = 16.0;
    pub const CHECK_WIDTH: f32 = 14.0;
    pub const SHADOW_OFFSET_Y: i8 = 4;
    pub const SHADOW_BLUR: i8 = 12;
}

// ── Welcome screen ──
pub mod welcome {
    pub const BTN_HEIGHT: f32 = 36.0;
    pub const TOP_OFFSET_FRAC: f32 = 0.22;
}

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

// ── Inspector layout ──
pub mod inspector {
    use super::ROW_M;

    pub const KF_COL_WIDTH: f32 = 18.0;
    pub const LABEL_MIN_WIDTH: f32 = 90.0;
    pub const LABEL_MAX_WIDTH: f32 = 160.0;
    pub const COL_GAP: f32 = 8.0;
    pub const INPUT_WIDTH_FLOAT: f32 = 72.0;
    pub const INPUT_COL_WIDTH: f32 = 120.0;
    pub const INPUT_WIDTH_VEC2: f32 = 110.0;
    pub const INPUT_WIDTH_SLIDER: f32 = 110.0;
    pub const INPUT_WIDTH_COLOR: f32 = 88.0;
    pub const ROW_HEIGHT: f32 = ROW_M;
    pub const KF_BTN_WIDTH: f32 = 18.0;
    pub const LABEL_WIDTH_FRAC: f32 = 0.42;
}
