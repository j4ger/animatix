//! Typography roles and compatibility font-size constants.
//!
//! `TextRole` centralises the 8-level type scale and is the recommended
//! API for new code. Legacy `FONT_SIZE_*` constants are re-exported by the
//! compatibility facade for use during migration.

use egui::FontId;

/// 8-level type scale based on a 1.2 ratio.
///
/// Each variant maps to a size and a font family (proportional or monospace).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextRole {
    Display,
    Heading,
    Title,
    Body,
    BodyS,
    Caption,
    Mono,
    Micro,
}

impl TextRole {
    /// Returns the `egui::FontId` for this role.
    pub fn font_id(&self) -> FontId {
        match self {
            Self::Display => FontId::proportional(20.0),
            Self::Heading => FontId::proportional(18.0),
            Self::Title => FontId::proportional(15.0),
            Self::Body => FontId::proportional(13.0),
            Self::BodyS => FontId::proportional(12.0),
            Self::Caption => FontId::proportional(11.0),
            Self::Mono => FontId::monospace(12.0),
            Self::Micro => FontId::proportional(10.0),
        }
    }

    /// Returns the font size in pixels.
    pub const fn size(&self) -> f32 {
        match self {
            Self::Display => 20.0,
            Self::Heading => 18.0,
            Self::Title => 15.0,
            Self::Body => 13.0,
            Self::BodyS => 12.0,
            Self::Caption => 11.0,
            Self::Mono => 12.0,
            Self::Micro => 10.0,
        }
    }
}

// ── Legacy font-size constants (for compatibility facade) ──

pub const FONT_SIZE_XS: f32 = TextRole::Micro.size();
pub const FONT_SIZE_S: f32 = TextRole::BodyS.size();
pub const FONT_SIZE_M: f32 = TextRole::Body.size();
pub const FONT_SIZE_L: f32 = TextRole::Title.size();
pub const FONT_SIZE_XL: f32 = TextRole::Heading.size();
