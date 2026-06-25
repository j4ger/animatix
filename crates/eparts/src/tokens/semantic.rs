//! Semantic token modules — the public color API for all UI code.
//!
//! Only the generic (domain-free) roles live here: `surface`, `text`, `accent`,
//! `status`, `border`, `overlay`.  App-specific roles (`canvas`, `timeline`,
//! `diagnostic`, `curve`, `editor`, `category`) are defined in
//! `animatix-gui/src/app/design_tokens/semantic.rs` and re-exported alongside
//! these generic modules so that existing paths like
//! `design_tokens::semantic::surface::BASE` continue to resolve.

use egui::Color32;

use super::primitive as p;

// ── Surface (5 depth layers) ──

pub mod surface {
    use super::*;

    /// Depth 0: window bottom layer (replaces BG_BASE).
    pub const BASE: Color32 = p::GRAY_950;
    /// Depth 1: panel background (replaces BG_PANEL).
    pub const PANEL: Color32 = p::GRAY_900;
    /// Depth 2: cards / floating surfaces (replaces BG_SURFACE).
    pub const SURFACE: Color32 = p::GRAY_850;
    /// Depth 3: widgets, inputs, buttons (replaces BG_WIDGET).
    pub const WIDGET: Color32 = p::GRAY_800;
    /// Depth 4: hover overlay (replaces BG_HOVER).
    pub const HOVER: Color32 = p::GRAY_700;
    /// Depth 4+: active / pressed (replaces BG_ACTIVE).
    pub const ACTIVE: Color32 = p::GRAY_600;

    /// Floating card / popup background (alpha).
    pub fn floating_card_bg() -> Color32 {
        Color32::from_rgba_unmultiplied(30, 30, 35, 220)
    }
}

// ── Text ──

pub mod text {
    use super::*;

    pub const PRIMARY: Color32 = p::GRAY_100;
    pub const SECONDARY: Color32 = p::GRAY_400;
    pub const MUTED: Color32 = p::GRAY_500;
    pub const DISABLED: Color32 = p::GRAY_600;

    /// Text on accent fills (uses surface BASE for contrast).
    pub const ON_ACCENT: Color32 = surface::BASE;

    pub fn faint() -> Color32 {
        Color32::from_rgba_unmultiplied(p::GRAY_100.r(), p::GRAY_100.g(), p::GRAY_100.b(), 80)
    }
    pub fn subtle() -> Color32 {
        Color32::from_rgba_unmultiplied(p::GRAY_100.r(), p::GRAY_100.g(), p::GRAY_100.b(), 160)
    }
    pub fn hover() -> Color32 {
        Color32::from_rgba_unmultiplied(p::GRAY_100.r(), p::GRAY_100.g(), p::GRAY_100.b(), 220)
    }
    pub fn dim() -> Color32 {
        Color32::from_rgba_unmultiplied(p::GRAY_100.r(), p::GRAY_100.g(), p::GRAY_100.b(), 180)
    }
}

// ── Accent ──

pub mod accent {
    use super::*;

    pub const PRIMARY: Color32 = p::BLUE_500;
    pub const CYAN: Color32 = p::CYAN_500;
    pub const PRIMARY_HOVER: Color32 = p::BLUE_400;
    pub const PRIMARY_ACTIVE: Color32 = p::BLUE_600;

    /// Pre-computed alpha variants.
    pub fn faint() -> Color32 {
        Color32::from_rgba_unmultiplied(p::BLUE_500.r(), p::BLUE_500.g(), p::BLUE_500.b(), 30)
    }
    pub fn ghost() -> Color32 {
        Color32::from_rgba_unmultiplied(p::BLUE_500.r(), p::BLUE_500.g(), p::BLUE_500.b(), 80)
    }
    pub fn subtle() -> Color32 {
        Color32::from_rgba_unmultiplied(p::BLUE_500.r(), p::BLUE_500.g(), p::BLUE_500.b(), 120)
    }
    pub fn hover() -> Color32 {
        Color32::from_rgba_unmultiplied(p::BLUE_500.r(), p::BLUE_500.g(), p::BLUE_500.b(), 140)
    }
    pub fn strong() -> Color32 {
        Color32::from_rgba_unmultiplied(p::BLUE_500.r(), p::BLUE_500.g(), p::BLUE_500.b(), 200)
    }
    pub fn selection() -> Color32 {
        Color32::from_rgba_unmultiplied(p::BLUE_500.r(), p::BLUE_500.g(), p::BLUE_500.b(), 60)
    }
}

// ── Status ──

pub mod status {
    use super::*;

    pub const SUCCESS: Color32 = p::GREEN_500;
    pub const WARNING: Color32 = p::AMBER_500;
    pub const ERROR: Color32 = p::RED_500;
    pub const INFO: Color32 = p::BLUE_500;

    pub const PLAYING_TEXT: Color32 = p::PLAYING_TEXT_RAW;
    pub const DIAGNOSTIC_ERROR: Color32 = p::DIAGNOSTIC_RED_RAW;
    pub const DIAGNOSTIC_WARNING: Color32 = p::DIAGNOSTIC_AMBER_RAW;

    pub fn success_faint() -> Color32 {
        Color32::from_rgba_unmultiplied(p::GREEN_500.r(), p::GREEN_500.g(), p::GREEN_500.b(), 60)
    }
    pub fn success_ultra_faint() -> Color32 {
        Color32::from_rgba_unmultiplied(p::GREEN_500.r(), p::GREEN_500.g(), p::GREEN_500.b(), 20)
    }
    pub fn warning_subtle() -> Color32 {
        Color32::from_rgba_unmultiplied(p::AMBER_500.r(), p::AMBER_500.g(), p::AMBER_500.b(), 120)
    }
    pub fn error_faint() -> Color32 {
        Color32::from_rgba_unmultiplied(p::RED_500.r(), p::RED_500.g(), p::RED_500.b(), 60)
    }
    pub fn error_ultra_faint() -> Color32 {
        Color32::from_rgba_unmultiplied(p::RED_500.r(), p::RED_500.g(), p::RED_500.b(), 20)
    }
}

// ── Borders ──

pub mod border {
    use super::*;

    pub const DEFAULT: Color32 = p::GRAY_700;
    pub const HOVER: Color32 = p::GRAY_600;
    pub const FOCUS: Color32 = p::BLUE_500;
}

// ── Lines (neutral grid/guide) ─────────────────────────────────────────

pub mod lines {
    use super::*;

    /// Light grid line (white, alpha 12).
    pub fn grid_line() -> Color32 {
        Color32::from_rgba_unmultiplied(255, 255, 255, 12)
    }
    /// Guide / reference line (white, alpha 30).
    pub fn guide_line() -> Color32 {
        Color32::from_rgba_unmultiplied(255, 255, 255, 30)
    }
}

// ── Overlay ──

pub mod overlay {
    use super::*;

    pub fn backdrop() -> Color32 {
        Color32::from_rgba_unmultiplied(0, 0, 0, 120)
    }
    pub fn badge_bg() -> Color32 {
        Color32::from_rgba_unmultiplied(p::GRAY_950.r(), p::GRAY_950.g(), p::GRAY_950.b(), 220)
    }
    pub fn tooltip_bg() -> Color32 {
        Color32::from_rgba_unmultiplied(p::GRAY_950.r(), p::GRAY_950.g(), p::GRAY_950.b(), 235)
    }
    pub fn shadow_ambient() -> Color32 {
        Color32::from_rgba_unmultiplied(0, 0, 0, 40)
    }
    pub fn shadow_direct() -> Color32 {
        Color32::from_rgba_unmultiplied(0, 0, 0, 60)
    }
}
