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
