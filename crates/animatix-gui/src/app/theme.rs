//! Unified design tokens for the entire Animatix GUI.
//!
//! All UI modules import from here. No local palette constants allowed.

#![allow(dead_code)]

use egui::Color32;

// ── Backgrounds ──
pub const BG_BASE: Color32 = Color32::from_rgb(12, 14, 18);
pub const BG_PANEL: Color32 = Color32::from_rgb(18, 20, 24);
pub const BG_SURFACE: Color32 = Color32::from_rgb(24, 27, 33);
pub const BG_WIDGET: Color32 = Color32::from_rgb(32, 36, 44);
pub const BG_HOVER: Color32 = Color32::from_rgb(28, 31, 38);
pub const BG_ACTIVE: Color32 = Color32::from_rgb(40, 45, 55);

// ── Text ──
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(228, 232, 243);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(150, 158, 175);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(90, 96, 110);
pub const TEXT_DISABLED: Color32 = Color32::from_rgb(60, 64, 72);

// ── Accents ──
pub const ACCENT_BLUE: Color32 = Color32::from_rgb(84, 110, 255);
pub const ACCENT_CYAN: Color32 = Color32::from_rgb(137, 200, 235);
pub const AMBER: Color32 = Color32::from_rgb(255, 196, 92);
pub const RED: Color32 = Color32::from_rgb(255, 100, 100);
pub const GREEN: Color32 = Color32::from_rgb(80, 200, 140);

// ── Borders ──
pub const BORDER: Color32 = Color32::from_rgb(40, 44, 52);
pub const BORDER_HOVER: Color32 = Color32::from_rgb(60, 66, 78);
pub const BORDER_FOCUS: Color32 = ACCENT_BLUE;

// ── Shadows (layered: ambient + direct) ──
// Using const fn because from_rgba_unmultiplied is not a const fn in egui 0.34
pub fn shadow_ambient() -> Color32 {
    Color32::from_rgba_unmultiplied(0, 0, 0, 40)
}
pub fn shadow_direct() -> Color32 {
    Color32::from_rgba_unmultiplied(0, 0, 0, 60)
}

// ── Spacing Scale ──
pub const SPACE_XS: f32 = 2.0;
pub const SPACE_S: f32 = 4.0;
pub const SPACE_M: f32 = 6.0;
pub const SPACE_L: f32 = 8.0;
pub const SPACE_XL: f32 = 12.0;

// ── Row Heights ──
pub const ROW_XS: f32 = 18.0;
pub const ROW_S: f32 = 20.0;
pub const ROW_M: f32 = 24.0;
pub const ROW_L: f32 = 28.0;

// ── Corner Radii ──
pub const RADIUS_S: f32 = 2.0;
pub const RADIUS_M: f32 = 4.0;
pub const RADIUS_L: f32 = 6.0;
pub const RADIUS_XL: f32 = 8.0;

// ── Typography ──
pub const FONT_SIZE_XS: f32 = 9.0;
pub const FONT_SIZE_S: f32 = 10.0;
pub const FONT_SIZE_M: f32 = 11.0;
pub const FONT_SIZE_L: f32 = 12.0;
pub const FONT_SIZE_XL: f32 = 14.0;

// ── Inspector Layout ──
/// Width of the keyframe indicator column (px).
pub const INSPECTOR_KF_COL_WIDTH: f32 = 18.0;
/// Minimum width of the label column (px).
pub const INSPECTOR_LABEL_MIN_WIDTH: f32 = 90.0;
/// Maximum width of the label column (px).
pub const INSPECTOR_LABEL_MAX_WIDTH: f32 = 160.0;
/// Gap between label column and input column (px).
pub const INSPECTOR_COL_GAP: f32 = 8.0;
/// Standard width for a single DragValue input (px).
pub const INSPECTOR_INPUT_WIDTH_FLOAT: f32 = 72.0;
/// Width of the entire right-hand input column (px).
pub const INSPECTOR_INPUT_COL_WIDTH: f32 = 120.0;
/// Standard width for a Vec2 input pair (px).
pub const INSPECTOR_INPUT_WIDTH_VEC2: f32 = 110.0;
/// Standard width for a slider + value input (px).
pub const INSPECTOR_INPUT_WIDTH_SLIDER: f32 = 110.0;
/// Standard width for a color swatch + hex input (px).
pub const INSPECTOR_INPUT_WIDTH_COLOR: f32 = 88.0;
/// Row height for inspector property rows (px).
pub const INSPECTOR_ROW_HEIGHT: f32 = ROW_M; // 24px
/// Width of the keyframe toggle button column (px).
pub const INSPECTOR_KF_BTN_WIDTH: f32 = 16.0;
/// Fraction of available width for the label column.
pub const INSPECTOR_LABEL_WIDTH_FRAC: f32 = 0.42;
