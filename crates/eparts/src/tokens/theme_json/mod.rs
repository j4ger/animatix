//! JSON theme loading and schema support (`theme-json` feature).
//!
//! The serialized format keeps the runtime [`Theme`](crate::tokens::theme::Theme)
//! representation as a DTO: colors are hex strings (`#rrggbb` or `#rrggbbaa`)
//! and shadows are small objects (`{ "offset": [x, y], "blur": n, "spread": n,
//! "color": "#..." }`).
//!
//! A theme file can carry both modes:
//! ```json
//! {
//!   "name": "My Theme",
//!   "dark": { "surface": { "base": "#202124" }, ... },
//!   "light": { "surface": { "base": "#f8f9fa" }, ... }
//! }
//! ```

mod file;
mod partial;
mod schema;

pub use file::{ThemeFile, ThemeJsonError};
pub use partial::{
    Color32Field, PartialAccent, PartialAlertLevelSlots, PartialAlertSlots, PartialBorder,
    PartialButtonSlots, PartialButtonStateSlots, PartialColorPickerSlots, PartialEasingCurveSlots,
    PartialElevation, PartialField, PartialFill, PartialInputSlots, PartialLines, PartialListSlots,
    PartialMenuItemSlots, PartialOverlay, PartialProgressSlots, PartialScrollbarSlots,
    PartialSkeletonSlots, PartialSlot, PartialStatus, PartialSurface, PartialTabSlot,
    PartialTabSlots, PartialTagSlots, PartialText, PartialTheme, PartialToastLevelSlots,
    PartialToastSlots, PartialToggleSlots, ShadowField,
};
pub use schema::theme_schema_json;

#[cfg(test)]
mod tests;
