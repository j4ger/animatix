//! C6 — Themed `Select` / Combobox widget.
//!
//! A searchable, clearable, optionally grouped dropdown. Selection state is
//! app-owned (`&mut Option<usize>`), while transient UI state (open/closed,
//! search filter, keyboard highlight) lives in `egui::Memory` per the framework
//! contract.
//!
//! The trigger is painted from `theme.components.input.*` slots so it matches
//! [`crate::widget::input::TextField`], and supports keyboard navigation
//! (arrows / Home / End / Enter / Escape) while focused.
//!
//! The dropdown panel is rendered with [`crate::widget::popover::Popover`]
//! so it participates in the overlay coordination layer (Escape, click-outside).

use egui::{Align, Key, Rect, Response, Role, Sense, Vec2, Widget, WidgetInfo};

use crate::tokens::spatial::{RADIUS_M, ROW_M, STROKE_WIDTH, density};
use crate::tokens::theme::theme;
use crate::tokens::typography::TextRole;
use crate::widget::button::Button;
use crate::widget::input::TextField;
use crate::widget::popover::Popover;

/// A single option entry in the flat list.
#[derive(Debug)]
enum FlatOption {
    Item { label: String, index: usize },
    Header(String),
}

/// A themed searchable/clearable select dropdown.
///
/// ## Flat usage
/// ```ignore
/// # let selected = &mut None;
/// Select::new("my_select", selected, &["Red", "Green", "Blue"])
///     .placeholder("Pick a color")
///     .searchable(true)
///     .clearable(true);
/// ```
///
/// ## Grouped usage
/// ```ignore
/// # let selected = &mut None;
/// Select::grouped("my_grouped", selected, &[
///     ("Fruits", &["Apple", "Banana"]),
///     ("Veg", &["Carrot"]),
/// ])
///     .placeholder("Pick food")
///     .searchable(true);
/// ```
#[derive(Debug)]
pub struct Select<'a> {
    id: egui::Id,
    selected: &'a mut Option<usize>,
    flat_options: Vec<FlatOption>,
    placeholder: &'a str,
    searchable: bool,
    clearable: bool,
}

mod core;
mod flat_option;
mod options;
#[cfg(test)]
mod tests;
mod widget_impl;
