//! Shared widget traits — the composable building blocks for eparts widgets.
//!
//! Four marker/builder traits (`Sizable`, `Selectable`, `Disableable`, `Collapsible`)
//! plus the `Size` enum they share.  These replace per-widget size/state enums and
//! keep widget APIs chainable and consistent.

use crate::tokens::spatial::{RADIUS_L, RADIUS_M, RADIUS_S, ROW_L, ROW_M, ROW_S, ROW_XS};
use crate::tokens::typography::TextRole;

// ── Size ──────────────────────────────────────────────────────────────

/// 4-tier size vocabulary with a custom escape hatch.
///
/// Mirrors gpui's `Size` concept: all widgets that implement `Sizable` use this
/// enum so every component shares one consistent spatial scale.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Size {
    Xs,
    Sm,
    #[default]
    Md,
    Lg,
    Custom(f32),
}

impl Size {
    /// Returns the row height (in px) for this size.
    pub fn row_height(self) -> f32 {
        match self {
            Self::Xs => ROW_XS,
            Self::Sm => ROW_S,
            Self::Md => ROW_M,
            Self::Lg => ROW_L,
            Self::Custom(v) => v,
        }
    }

    /// Returns the corner radius (in px) for this size.
    pub fn radius(self) -> f32 {
        match self {
            Self::Xs | Self::Sm => RADIUS_S,
            Self::Md => RADIUS_M,
            Self::Lg => RADIUS_L,
            Self::Custom(_) => RADIUS_M,
        }
    }

    /// Returns the horizontal padding (in px) for this size.
    pub fn pad_x(self) -> f32 {
        match self {
            Self::Xs => crate::tokens::spatial::SPACE_1,
            Self::Sm => crate::tokens::spatial::SPACE_2,
            Self::Md => crate::tokens::spatial::SPACE_3,
            Self::Lg => crate::tokens::spatial::SPACE_4,
            Self::Custom(_) => crate::tokens::spatial::SPACE_3,
        }
    }

    /// Returns the vertical padding (in px) for this size.
    pub fn pad_y(self) -> f32 {
        match self {
            Self::Xs => crate::tokens::spatial::SPACE_1,
            Self::Sm => crate::tokens::spatial::SPACE_2,
            Self::Md => crate::tokens::spatial::SPACE_3,
            Self::Lg => crate::tokens::spatial::SPACE_4,
            Self::Custom(_) => crate::tokens::spatial::SPACE_3,
        }
    }

    /// Returns the `TextRole` that corresponds to this size.
    pub fn font(self) -> TextRole {
        match self {
            Self::Xs => TextRole::Caption,
            Self::Sm => TextRole::BodyS,
            Self::Md => TextRole::Body,
            Self::Lg => TextRole::Title,
            Self::Custom(_) => TextRole::Body,
        }
    }
}

// ── Builder traits ─────────────────────────────────────────────────────

/// Builder trait for size-aware widgets.
///
/// Allows `.xs()`, `.sm()`, `.lg()` shorthand chaining on any implementing widget.
pub trait Sizable: Sized {
    fn with_size(self, size: Size) -> Self;

    fn xs(self) -> Self {
        self.with_size(Size::Xs)
    }

    fn sm(self) -> Self {
        self.with_size(Size::Sm)
    }

    fn lg(self) -> Self {
        self.with_size(Size::Lg)
    }
}

/// Builder trait for selectable widgets (e.g. rows, tabs, list items).
///
/// The widget carries its selected state across frames via `ctx.data`.
pub trait Selectable: Sized {
    fn selected(self, yes: bool) -> Self;
    fn is_selected(&self) -> bool;
}

/// Builder trait for widgets that can be disabled.
///
/// Disabled widgets render with reduced emphasis and do not interact.
pub trait Disableable: Sized {
    fn disabled(self, yes: bool) -> Self;
    fn is_disabled(&self) -> bool;
}

/// Builder trait for collapsible widgets (e.g. accordion, tree rows).
pub trait Collapsible: Sized {
    fn collapsed(self, yes: bool) -> Self;
    fn is_collapsed(&self) -> bool;
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_default_is_md() {
        assert_eq!(Size::default(), Size::Md);
    }

    #[test]
    fn size_mapping_matches_spatial_constants() {
        assert_eq!(Size::Xs.row_height(), ROW_XS);
        assert_eq!(Size::Sm.row_height(), ROW_S);
        assert_eq!(Size::Md.row_height(), ROW_M);
        assert_eq!(Size::Lg.row_height(), ROW_L);
        assert_eq!(Size::Custom(42.0).row_height(), 42.0);

        assert_eq!(Size::Xs.radius(), RADIUS_S);
        assert_eq!(Size::Sm.radius(), RADIUS_S);
        assert_eq!(Size::Md.radius(), RADIUS_M);
        assert_eq!(Size::Lg.radius(), RADIUS_L);
        assert_eq!(Size::Custom(42.0).radius(), RADIUS_M);
    }

    #[test]
    fn size_font_roles_are_valid() {
        assert_eq!(Size::Xs.font(), TextRole::Caption);
        assert_eq!(Size::Sm.font(), TextRole::BodyS);
        assert_eq!(Size::Md.font(), TextRole::Body);
        assert_eq!(Size::Lg.font(), TextRole::Title);
        assert_eq!(Size::Custom(42.0).font(), TextRole::Body);
    }

    #[test]
    fn sizable_trait_works() {
        #[derive(Clone, Copy, Debug, PartialEq)]
        struct Dummy {
            size: Size,
        }

        impl Default for Dummy {
            fn default() -> Self {
                Self { size: Size::Md }
            }
        }

        impl Sizable for Dummy {
            fn with_size(mut self, size: Size) -> Self {
                self.size = size;
                self
            }
        }

        let d = Dummy::default().xs().lg();
        assert_eq!(d.size, Size::Lg);
    }
}
