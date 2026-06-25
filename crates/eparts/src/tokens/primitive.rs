//! Raw palette — public so that app-specific token submodules in `animatix-gui`
//! can reference primitive entries through the `eparts::tokens::primitive` path.
//!
//! # Visibility
//! All entries are `pub` because the animatix-gui app-specific semantic
//! submodules (category, diagnostic, curve, editor, timeline, canvas) access
//! raw palette values directly via `super::primitive as p`.
//!
//! # Values
//! Matches the spec (docs/gui_design_language.md). Values are unchanged.

use egui::Color32;

// ── Core palette ──

pub const BLUE_600: Color32 = Color32::from_rgb(60, 84, 220);
pub const BLUE_500: Color32 = Color32::from_rgb(84, 110, 255);
pub const BLUE_400: Color32 = Color32::from_rgb(120, 145, 255);

pub const CYAN_500: Color32 = Color32::from_rgb(137, 200, 235);

pub const GREEN_500: Color32 = Color32::from_rgb(80, 200, 140);
pub const AMBER_500: Color32 = Color32::from_rgb(255, 196, 92);
pub const RED_500: Color32 = Color32::from_rgb(255, 100, 100);
pub const PURPLE_500: Color32 = Color32::from_rgb(156, 39, 176);

// ── Surface / text palette ──

pub const GRAY_950: Color32 = Color32::from_rgb(10, 12, 16);
pub const GRAY_900: Color32 = Color32::from_rgb(16, 18, 23);
pub const GRAY_850: Color32 = Color32::from_rgb(22, 25, 31);
pub const GRAY_800: Color32 = Color32::from_rgb(30, 34, 42);
pub const GRAY_700: Color32 = Color32::from_rgb(42, 47, 57);
pub const GRAY_600: Color32 = Color32::from_rgb(60, 66, 78);
pub const GRAY_500: Color32 = Color32::from_rgb(90, 97, 112);
pub const GRAY_400: Color32 = Color32::from_rgb(130, 138, 153);
#[allow(dead_code)] // Reserved for future light-theme / elevated-surface tokens
pub const GRAY_300: Color32 = Color32::from_rgb(170, 178, 192);
#[allow(dead_code)] // Reserved for future light-theme / elevated-surface tokens
pub const GRAY_200: Color32 = Color32::from_rgb(210, 216, 228);
pub const GRAY_100: Color32 = Color32::from_rgb(232, 236, 245);

// ── Domain raw colors ──

/// Canvas background (separate from surface palette).
pub const CANVAS_BG: Color32 = Color32::from_rgb(8, 8, 12);

/// Diagnostic phase colors.
pub const DIAG_PARSE: Color32 = Color32::from_rgb(137, 180, 250);
pub const DIAG_RESOLVE: Color32 = Color32::from_rgb(180, 190, 254);
pub const DIAG_COMPILE: Color32 = Color32::from_rgb(203, 166, 126);

/// Playing indicator text.
pub const PLAYING_TEXT_RAW: Color32 = Color32::from_rgb(216, 249, 235);

/// Diagnostic status (non-phase) colors.
pub const DIAGNOSTIC_RED_RAW: Color32 = Color32::from_rgb(255, 136, 136);
pub const DIAGNOSTIC_AMBER_RAW: Color32 = Color32::from_rgb(255, 214, 102);

/// Curve editor.
pub const CURVE_GREEN_RAW: Color32 = Color32::from_rgb(100, 255, 100);
pub const CURVE_BLUE_RAW: Color32 = Color32::from_rgb(80, 140, 255);
pub const CURVE_GRAY_RAW: Color32 = Color32::from_rgb(200, 200, 200);

/// Keyframe flash.
pub const KF_FLASH_RAW: Color32 = Color32::from_rgb(255, 200, 50);

/// Insertion palette snippet highlight.
pub const SNIPPET_BLUE_RAW: Color32 = Color32::from_rgb(108, 153, 187);
