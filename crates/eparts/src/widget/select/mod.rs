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

use egui::{Align, Key, Rect, Response, Sense, Vec2, Widget, WidgetInfo, WidgetType};

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

impl FlatOption {
    fn index(&self) -> Option<usize> {
        match self {
            Self::Item { index, .. } => Some(*index),
            _ => None,
        }
    }

    fn label(&self) -> Option<&str> {
        match self {
            Self::Item { label, .. } => Some(label.as_str()),
            _ => None,
        }
    }
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

impl<'a> Select<'a> {
    /// Create a new flat `Select`.
    pub fn new<T: AsRef<str> + 'a>(
        id: impl Into<egui::Id>,
        selected: &'a mut Option<usize>,
        options: &'a [T],
    ) -> Self {
        let flat_options = options
            .iter()
            .enumerate()
            .map(|(i, s)| FlatOption::Item {
                label: s.as_ref().to_string(),
                index: i,
            })
            .collect();

        Self {
            id: id.into(),
            selected,
            flat_options,
            placeholder: "Select…",
            searchable: false,
            clearable: false,
        }
    }

    /// Create a new grouped `Select`.
    ///
    /// `groups` is a list of `(group_label, items)` slices. The selected
    /// index refers to the position of the item within the flattened
    /// sequence of all items (headers do not count).
    pub fn grouped(
        id: impl Into<egui::Id>,
        selected: &'a mut Option<usize>,
        groups: &'a [(&'a str, &'a [&'a str])],
    ) -> Self {
        let mut flat_options = Vec::new();
        let mut global_index = 0usize;

        for (group_label, items) in groups {
            flat_options.push(FlatOption::Header(group_label.to_string()));
            for item in *items {
                flat_options.push(FlatOption::Item {
                    label: item.to_string(),
                    index: global_index,
                });
                global_index += 1;
            }
        }

        Self {
            id: id.into(),
            selected,
            flat_options,
            placeholder: "Select…",
            searchable: false,
            clearable: false,
        }
    }

    /// Set the placeholder text shown when nothing is selected.
    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }

    /// Enable the search box inside the dropdown.
    pub fn searchable(mut self, searchable: bool) -> Self {
        self.searchable = searchable;
        self
    }

    /// Enable the clear ('x') button inside the dropdown.
    pub fn clearable(mut self, clearable: bool) -> Self {
        self.clearable = clearable;
        self
    }
}

mod options;
#[cfg(test)]
mod tests;
mod widget_impl;
