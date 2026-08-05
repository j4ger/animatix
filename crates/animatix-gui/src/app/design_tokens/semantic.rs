//! Semantic token modules — the public color API for all UI code.
//!
//! Generic color roles (`surface`, `text`, `accent`, `status`, `border`,
//! `overlay`) are re-exported from `eparts::tokens::semantic`. App-specific
//! roles (`canvas`, `timeline`, `diagnostic`, `curve`, `editor`, `category`)
//! are defined inline below.
//!
//! All modules — generic and app-specific — appear at the same module depth
//! so that existing paths like `design_tokens::semantic::surface::BASE` and
//! `design_tokens::semantic::category::TRANSFORM` continue to resolve without
//! any call-site changes.
//!
//! # Import convention
//! ```ignore
//! use crate::app::design_tokens::semantic::{accent, surface, text};
//! ```

use egui::Color32;
// ── Generic roles — re-exported from eparts ──────────────────────────────────
pub use eparts::tokens::semantic;
pub use eparts::tokens::semantic::{accent, border, overlay, status, surface, text};

use super::primitive as p;

// ── App-specific submodules — defined locally ────────────────────────────────

// ── Canvas-specific (preview overlay only) ──

pub mod canvas {
    use super::*;

    pub const BG: Color32 = p::CANVAS_BG;

    // Neutral line colors promoted to eparts::tokens::semantic::lines —
    // re-exported here so existing `canvas::grid_line()` / `canvas::guide_line()`
    // call sites keep resolving without changes.
    pub use eparts::tokens::semantic::lines::{grid_line, guide_line};
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
