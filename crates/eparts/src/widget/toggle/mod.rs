//! Themed Checkbox and Switch widgets with animated transitions.
//!
//! Both are thin aliases over one generic [`Toggle`]: they share field layout,
//! label placement, interaction and accessibility, and differ only in the
//! control they paint (a check box vs. a track and sliding thumb).

mod checkbox;
mod core;
mod switch;
#[cfg(test)]
mod tests;

pub use core::{Side, Toggle};

/// A themed checkbox with an animated checkmark crossfade.
///
/// State is app-owned: the caller passes `&mut bool`. The checkmark fades in/out
/// smoothly (~200 ms, ease-in-out) rather than snapping.
///
/// ## Examples
/// ```ignore
/// let mut enabled = false;
/// ui.add(Checkbox::new(&mut enabled).label("Enable feature"));
/// ```
pub type Checkbox<'a> = Toggle<'a, false>;

/// A themed switch with an animated sliding thumb.
///
/// State is app-owned: the caller passes `&mut bool`. The thumb slides while the
/// track crossfades between the off/on slot fills.
///
/// ## Examples
/// ```ignore
/// let mut enabled = false;
/// ui.add(Switch::new(&mut enabled).label("Enable feature"));
/// ```
pub type Switch<'a> = Toggle<'a, true>;
