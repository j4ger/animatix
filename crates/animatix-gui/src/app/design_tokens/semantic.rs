//! Semantic token modules — the public color API for all UI code.
//!
//! Every module maps a role (surface, text, accent, status, etc.) to
//! primitive palette values. No UI code should import `primitive` directly.
//!
//! # Import convention
//! ```ignore
//! use crate::app::design_tokens::semantic::surface;
//! use crate::app::design_tokens::semantic::text;
//! ```

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
        Color32::from_rgba_unmultiplied(
            p::GRAY_100.r(),
            p::GRAY_100.g(),
            p::GRAY_100.b(),
            80,
        )
    }
    pub fn subtle() -> Color32 {
        Color32::from_rgba_unmultiplied(
            p::GRAY_100.r(),
            p::GRAY_100.g(),
            p::GRAY_100.b(),
            160,
        )
    }
    pub fn hover() -> Color32 {
        Color32::from_rgba_unmultiplied(
            p::GRAY_100.r(),
            p::GRAY_100.g(),
            p::GRAY_100.b(),
            220,
        )
    }
    pub fn dim() -> Color32 {
        Color32::from_rgba_unmultiplied(
            p::GRAY_100.r(),
            p::GRAY_100.g(),
            p::GRAY_100.b(),
            180,
        )
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
        Color32::from_rgba_unmultiplied(
            p::GREEN_500.r(),
            p::GREEN_500.g(),
            p::GREEN_500.b(),
            60,
        )
    }
    pub fn success_ultra_faint() -> Color32 {
        Color32::from_rgba_unmultiplied(
            p::GREEN_500.r(),
            p::GREEN_500.g(),
            p::GREEN_500.b(),
            20,
        )
    }
    pub fn warning_subtle() -> Color32 {
        Color32::from_rgba_unmultiplied(
            p::AMBER_500.r(),
            p::AMBER_500.g(),
            p::AMBER_500.b(),
            120,
        )
    }
    pub fn error_faint() -> Color32 {
        Color32::from_rgba_unmultiplied(
            p::RED_500.r(),
            p::RED_500.g(),
            p::RED_500.b(),
            60,
        )
    }
    pub fn error_ultra_faint() -> Color32 {
        Color32::from_rgba_unmultiplied(
            p::RED_500.r(),
            p::RED_500.g(),
            p::RED_500.b(),
            20,
        )
    }
}

// ── Category (property groups, scene tracks, insertion palette) ──

pub mod category {
    use super::*;

    pub const TRANSFORM: Color32 = p::BLUE_500;
    pub const STYLE: Color32 = p::GREEN_500;
    pub const SHAPE: Color32 = p::AMBER_500;
    pub const TEXT: Color32 = p::CYAN_500;
    pub const ACTION: Color32 = p::PURPLE_500;
    pub const FILTER: Color32 = p::PURPLE_500;
    pub const MEDIA: Color32 = p::PURPLE_500;
}

// ── Borders ──

pub mod border {
    use super::*;

    pub const DEFAULT: Color32 = p::GRAY_700;
    pub const HOVER: Color32 = p::GRAY_600;
    pub const FOCUS: Color32 = p::BLUE_500;
}

// ── Canvas-specific (preview overlay only) ──

pub mod canvas {
    use super::*;

    pub const BG: Color32 = p::CANVAS_BG;

    pub fn grid_line() -> Color32 {
        Color32::from_rgba_unmultiplied(255, 255, 255, 12)
    }
    pub fn guide_line() -> Color32 {
        Color32::from_rgba_unmultiplied(255, 255, 255, 30)
    }
    pub fn hatch_line() -> Color32 {
        Color32::from_rgba_unmultiplied(255, 255, 255, 30)
    }
    pub fn ghost_prev() -> Color32 {
        Color32::from_rgba_unmultiplied(80, 220, 120, 77)
    }
    pub fn ghost_next() -> Color32 {
        Color32::from_rgba_unmultiplied(80, 160, 255, 77)
    }
    pub fn snap_guide_line() -> Color32 {
        Color32::from_rgba_unmultiplied(84, 191, 123, 160)
    }
    pub fn snap_guide_label_bg() -> Color32 {
        Color32::from_rgba_unmultiplied(30, 30, 35, 200)
    }
}

// ── Timeline ──

pub mod timeline {
    use super::*;

    pub fn track_block_1() -> Color32 {
        Color32::from_rgba_unmultiplied(92, 140, 255, 60)
    }
    pub fn track_block_2() -> Color32 {
        Color32::from_rgba_unmultiplied(145, 104, 255, 60)
    }
    pub fn track_block_3() -> Color32 {
        Color32::from_rgba_unmultiplied(84, 191, 123, 60)
    }
    pub fn track_block_4() -> Color32 {
        Color32::from_rgba_unmultiplied(245, 179, 78, 60)
    }
    pub fn track_block_5() -> Color32 {
        Color32::from_rgba_unmultiplied(233, 108, 122, 60)
    }
    pub fn loop_region() -> Color32 {
        Color32::from_rgba_unmultiplied(100, 200, 255, 40)
    }
    pub fn transition_stripe_1() -> Color32 {
        Color32::from_rgba_unmultiplied(255, 200, 100, 50)
    }
    pub fn transition_stripe_2() -> Color32 {
        Color32::from_rgba_unmultiplied(100, 200, 255, 50)
    }
    pub fn transition_stripe_3() -> Color32 {
        Color32::from_rgba_unmultiplied(255, 120, 120, 50)
    }
    pub fn transition_stripe_4() -> Color32 {
        Color32::from_rgba_unmultiplied(120, 255, 160, 50)
    }
    pub fn transition_stripe_5() -> Color32 {
        Color32::from_rgba_unmultiplied(200, 140, 255, 50)
    }
    pub fn transition_stripe_6() -> Color32 {
        Color32::from_rgba_unmultiplied(255, 180, 50, 50)
    }

    pub const KF_FLASH: Color32 = p::KF_FLASH_RAW;

    pub fn row_alt() -> Color32 {
        Color32::from_rgba_unmultiplied(255, 255, 255, 2)
    }
}

// ── Diagnostic phase ──

pub mod diagnostic {
    use super::*;

    pub const PHASE_PARSE: Color32 = p::DIAG_PARSE;
    pub const PHASE_RESOLVE: Color32 = p::DIAG_RESOLVE;
    pub const PHASE_COMPILE: Color32 = p::DIAG_COMPILE;
}

// ── Overlay ──

pub mod overlay {
    use super::*;

    pub fn backdrop() -> Color32 {
        Color32::from_rgba_unmultiplied(0, 0, 0, 120)
    }
    pub fn badge_bg() -> Color32 {
        Color32::from_rgba_unmultiplied(
            p::GRAY_950.r(),
            p::GRAY_950.g(),
            p::GRAY_950.b(),
            220,
        )
    }
    pub fn tooltip_bg() -> Color32 {
        Color32::from_rgba_unmultiplied(
            p::GRAY_950.r(),
            p::GRAY_950.g(),
            p::GRAY_950.b(),
            235,
        )
    }
    pub fn shadow_ambient() -> Color32 {
        Color32::from_rgba_unmultiplied(0, 0, 0, 40)
    }
    pub fn shadow_direct() -> Color32 {
        Color32::from_rgba_unmultiplied(0, 0, 0, 60)
    }
}

// ── Curve editor colors ──

pub mod curve {
    use super::*;

    pub const GREEN: Color32 = p::CURVE_GREEN_RAW;
    pub const BLUE: Color32 = p::CURVE_BLUE_RAW;
    pub const GRAY: Color32 = p::CURVE_GRAY_RAW;
}

// ── Editor-specific ──

pub mod editor {
    use super::*;

    /// Snippet highlight in insertion palette / code editor.
    pub const SNIPPET_BLUE: Color32 = p::SNIPPET_BLUE_RAW;
}
