//! Runtime `Theme` struct + immediate-mode accessors.
//!
//! `Theme` mirrors `tokens::semantic` so that `Theme::dark()` is *by construction*
//! equal to the current compile-time consts.  The const modules stay intact.
//!
//! `Theme` is grouped into three scopes: [`Palette`] (surfaces, text, accents,
//! status, borders, overlays, lines), [`Components`] (per-widget color slots),
//! and [`Elevation`] (shadow tokens).
//!
//! Access pattern (per §3a Step 2 of the roadmap):
//! ```ignore
//! let t = eparts::theme(ui);
//! let bg = t.palette.surface.base;
//! let primary = t.components.button.primary.normal.bg;
//! ```
//! Set once per frame (or on theme switch):
//! ```ignore
//! eparts::set_theme(ctx, Theme::light());
//! ```

mod accessors;
mod components;
mod dark;
mod elevation;
mod light;
mod model;
mod palette;
#[cfg(feature = "theme-json")]
mod serde_helpers;
mod slots;

pub use accessors::{AppThemeChoice, set_theme, theme, theme_from_ctx};
pub use components::Components;
pub use elevation::Elevation;
pub use model::Theme;
pub use palette::{Accent, Border, Lines, Overlay, Palette, Status, Surface, Text};
#[cfg(feature = "theme-json")]
pub(crate) use serde_helpers::{serde_color32, serde_shadow};
pub use slots::{
    AlertLevelSlots, AlertSlots, ButtonSlots, ButtonStateSlots, ColorPickerSlots, EasingCurveSlots,
    Fill, InputSlots, ListSlots, MenuItemSlots, ProgressSlots, ScrollbarSlots, SkeletonSlots, Slot,
    TabSlot, TabSlots, TagSlots, ToastLevelSlots, ToastSlots, ToggleSlots,
};

#[cfg(test)]
mod tests;
