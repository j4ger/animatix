//! Unified design tokens for the entire Animatix GUI.
//!
//! All UI modules import from here. No local palette constants allowed.
//!
//! Token groups:
//! - `color`: SURFACE_BASE, ELEVATED, WIDGET, TEXT_PRIMARY, etc.
//! - `spacing`: XS, S, M, L, XL, XXL
//! - `radius`: NONE, SM, MD, LG, FULL
//! - `typography`: H1, H2, BODY, CAPTION, MONO


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

// ── Ghost / Onion Skin ──
pub fn ghost_prev() -> Color32 { Color32::from_rgba_unmultiplied(80, 220, 120, 77) }
pub fn ghost_next() -> Color32 { Color32::from_rgba_unmultiplied(80, 160, 255, 77) }

// ── Grid & Guides ──
pub fn grid_line() -> Color32 { Color32::from_rgba_unmultiplied(255, 255, 255, 12) }
pub fn guide_line() -> Color32 { Color32::from_rgba_unmultiplied(255, 255, 255, 30) }
pub fn hatch_line() -> Color32 { Color32::from_rgba_unmultiplied(255, 255, 255, 30) }

// ── Smart Snap ──
pub fn snap_guide_line() -> Color32 { Color32::from_rgba_unmultiplied(84, 191, 123, 160) }
pub fn snap_guide_label_bg() -> Color32 { Color32::from_rgba_unmultiplied(30, 30, 35, 200) }

// ── Transport Bar ──
pub fn track_block_1() -> Color32 { Color32::from_rgba_unmultiplied(92, 140, 255, 60) }
pub fn track_block_2() -> Color32 { Color32::from_rgba_unmultiplied(145, 104, 255, 60) }
pub fn track_block_3() -> Color32 { Color32::from_rgba_unmultiplied(84, 191, 123, 60) }
pub fn track_block_4() -> Color32 { Color32::from_rgba_unmultiplied(245, 179, 78, 60) }
pub fn track_block_5() -> Color32 { Color32::from_rgba_unmultiplied(233, 108, 122, 60) }
pub fn loop_region() -> Color32 { Color32::from_rgba_unmultiplied(100, 200, 255, 40) }
pub fn transition_stripe_1() -> Color32 { Color32::from_rgba_unmultiplied(255, 200, 100, 50) }
pub fn transition_stripe_2() -> Color32 { Color32::from_rgba_unmultiplied(100, 200, 255, 50) }
pub fn transition_stripe_3() -> Color32 { Color32::from_rgba_unmultiplied(255, 120, 120, 50) }
pub fn transition_stripe_4() -> Color32 { Color32::from_rgba_unmultiplied(120, 255, 160, 50) }
pub fn transition_stripe_5() -> Color32 { Color32::from_rgba_unmultiplied(200, 140, 255, 50) }
pub fn transition_stripe_6() -> Color32 { Color32::from_rgba_unmultiplied(255, 180, 50, 50) }

// ── Diagnostic Phases ──
pub const DIAG_PHASE_PARSE: Color32 = Color32::from_rgb(137, 180, 250);
pub const DIAG_PHASE_RESOLVE: Color32 = Color32::from_rgb(180, 190, 254);
pub const DIAG_PHASE_COMPILE: Color32 = Color32::from_rgb(203, 166, 126);

// ── Overlay ──
pub fn overlay_backdrop() -> Color32 { Color32::from_rgba_unmultiplied(0, 0, 0, 120) }

// ── Floating Card ──
pub fn floating_card_bg() -> Color32 { Color32::from_rgba_unmultiplied(30, 30, 35, 220) }

// ── Alpha-tinted accents ──
pub fn accent_faint() -> Color32 { Color32::from_rgba_unmultiplied(ACCENT_BLUE.r(), ACCENT_BLUE.g(), ACCENT_BLUE.b(), 30) }
pub fn accent_ghost() -> Color32 { Color32::from_rgba_unmultiplied(ACCENT_BLUE.r(), ACCENT_BLUE.g(), ACCENT_BLUE.b(), 80) }
pub fn accent_subtle() -> Color32 { Color32::from_rgba_unmultiplied(ACCENT_BLUE.r(), ACCENT_BLUE.g(), ACCENT_BLUE.b(), 120) }
pub fn accent_hover() -> Color32 { Color32::from_rgba_unmultiplied(ACCENT_BLUE.r(), ACCENT_BLUE.g(), ACCENT_BLUE.b(), 140) }
pub fn accent_strong() -> Color32 { Color32::from_rgba_unmultiplied(ACCENT_BLUE.r(), ACCENT_BLUE.g(), ACCENT_BLUE.b(), 200) }
pub fn accent_selection() -> Color32 { Color32::from_rgba_unmultiplied(ACCENT_BLUE.r(), ACCENT_BLUE.g(), ACCENT_BLUE.b(), 60) }

// ── Alpha-tinted text ──
pub fn text_faint() -> Color32 { Color32::from_rgba_unmultiplied(TEXT_PRIMARY.r(), TEXT_PRIMARY.g(), TEXT_PRIMARY.b(), 80) }
pub fn text_subtle() -> Color32 { Color32::from_rgba_unmultiplied(TEXT_PRIMARY.r(), TEXT_PRIMARY.g(), TEXT_PRIMARY.b(), 160) }
pub fn text_hover() -> Color32 { Color32::from_rgba_unmultiplied(TEXT_PRIMARY.r(), TEXT_PRIMARY.g(), TEXT_PRIMARY.b(), 220) }
pub fn text_dim() -> Color32 { Color32::from_rgba_unmultiplied(TEXT_PRIMARY.r(), TEXT_PRIMARY.g(), TEXT_PRIMARY.b(), 180) }

// ── Alpha-tinted amber ──
pub fn amber_subtle() -> Color32 { Color32::from_rgba_unmultiplied(AMBER.r(), AMBER.g(), AMBER.b(), 120) }

// ── Alpha-tinted green / red ──
pub fn green_faint() -> Color32 { Color32::from_rgba_unmultiplied(GREEN.r(), GREEN.g(), GREEN.b(), 60) }
pub fn green_ultra_faint() -> Color32 { Color32::from_rgba_unmultiplied(GREEN.r(), GREEN.g(), GREEN.b(), 20) }
pub fn red_faint() -> Color32 { Color32::from_rgba_unmultiplied(RED.r(), RED.g(), RED.b(), 60) }
pub fn red_ultra_faint() -> Color32 { Color32::from_rgba_unmultiplied(RED.r(), RED.g(), RED.b(), 20) }

// ── Status colors ──
pub const PLAYING_TEXT: Color32 = Color32::from_rgb(216, 249, 235);
pub const DIAGNOSTIC_RED: Color32 = Color32::from_rgb(255, 136, 136);
pub const DIAGNOSTIC_AMBER: Color32 = Color32::from_rgb(255, 214, 102);

// ── Badge / Tooltip backgrounds ──
pub fn badge_bg() -> Color32 { Color32::from_rgba_unmultiplied(BG_BASE.r(), BG_BASE.g(), BG_BASE.b(), 220) }
pub fn tooltip_bg() -> Color32 { Color32::from_rgba_unmultiplied(BG_BASE.r(), BG_BASE.g(), BG_BASE.b(), 235) }

// ── Alternating row backgrounds ──
pub fn row_alt() -> Color32 { Color32::from_rgba_unmultiplied(255, 255, 255, 2) }

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
pub const FONT_SIZE_XS: f32 = 10.0;
pub const FONT_SIZE_S: f32 = 12.0;
pub const FONT_SIZE_M: f32 = 13.0;
pub const FONT_SIZE_L: f32 = 15.0;
pub const FONT_SIZE_XL: f32 = 18.0;

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

// ── Color utilities ──

/// Linearly interpolate between two colors.
pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
        (a.a() as f32 + (b.a() as f32 - a.a() as f32) * t) as u8,
    )
}

/// Multiply a color's alpha by a factor.
pub fn multiply_alpha(c: Color32, factor: f32) -> Color32 {
    let factor = factor.clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied(
        (c.r() as f32 * factor) as u8,
        (c.g() as f32 * factor) as u8,
        (c.b() as f32 * factor) as u8,
        (c.a() as f32 * factor) as u8,
    )
}
