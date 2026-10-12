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

// ── Core palette (Electric Indigo + Studio Status Accents) ──

pub const BLUE_600: Color32 = Color32::from_rgb(70, 94, 230);
pub const BLUE_500: Color32 = Color32::from_rgb(84, 110, 255);
pub const BLUE_400: Color32 = Color32::from_rgb(115, 138, 255);

pub const CYAN_500: Color32 = Color32::from_rgb(56, 189, 248);

pub const GREEN_500: Color32 = Color32::from_rgb(16, 185, 129);
pub const AMBER_500: Color32 = Color32::from_rgb(245, 158, 11);
pub const RED_500: Color32 = Color32::from_rgb(244, 63, 94);
pub const PURPLE_500: Color32 = Color32::from_rgb(168, 85, 247);

// ── Surface / text palette (Obsidian Studio) ──

pub const GRAY_950: Color32 = Color32::from_rgb(14, 16, 19);
pub const GRAY_900: Color32 = Color32::from_rgb(20, 23, 28);
pub const GRAY_850: Color32 = Color32::from_rgb(26, 30, 36);
pub const GRAY_800: Color32 = Color32::from_rgb(32, 36, 44);
pub const GRAY_700: Color32 = Color32::from_rgb(41, 47, 58);
pub const GRAY_600: Color32 = Color32::from_rgb(51, 59, 73);
pub const GRAY_500: Color32 = Color32::from_rgb(100, 116, 139);
pub const GRAY_400: Color32 = Color32::from_rgb(148, 163, 184);
#[allow(dead_code)] // Reserved for future light-theme / elevated-surface tokens
pub const GRAY_300: Color32 = Color32::from_rgb(180, 190, 205);
#[allow(dead_code)] // Reserved for future light-theme / elevated-surface tokens
pub const GRAY_200: Color32 = Color32::from_rgb(220, 226, 235);
pub const GRAY_100: Color32 = Color32::from_rgb(248, 250, 252);

// ── Domain raw colors ──

/// Canvas background (separate from surface palette).
pub const CANVAS_BG: Color32 = Color32::from_rgb(8, 9, 11);

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
