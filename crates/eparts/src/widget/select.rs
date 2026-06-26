//! C6 — Themed `Select` / Combobox widget.
//!
//! A searchable, clearable, optionally grouped dropdown. Selection state is
//! app-owned (`&mut Option<usize>`), while transient UI state (open/closed,
//! search filter) lives in `egui::Memory` per the framework contract.
//!
//! The dropdown panel is rendered with [`crate::widget::popover::Popover`]
//! so it participates in the overlay coordination layer (Escape, click-outside).

use egui::{Response, TextEdit, Widget};

use crate::tokens::theme::theme;
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

impl Widget for Select<'_> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let t = theme(ui);
        let popover_id = self.id.with("__popover");
        let filter_key = self.id.with("__filter");

        let current_label = match self.selected {
            Some(idx) => self
                .flat_options
                .iter()
                .find_map(|opt| opt.label().filter(|_| opt.index() == Some(*idx)))
                .unwrap_or(self.placeholder),
            None => self.placeholder,
        };

        let button_response = ui.button(current_label).on_hover_cursor(egui::CursorIcon::Default);

        let popover = Popover::new(popover_id)
            .below()
            .max_width(260.0);

        let _popover_resp = popover.show(ui, &button_response, |ui| {
            let mut filter: String =
                ui.ctx().data(|d| d.get_temp::<String>(filter_key).unwrap_or_default());

            ui.horizontal(|ui| {
                if self.searchable {
                    let edit = ui.add(
                        TextEdit::singleline(&mut filter).hint_text("Search…"),
                    );
                    if edit.changed() {
                        ui.ctx()
                            .data_mut(|d| d.insert_temp(filter_key, filter.clone()));
                    }
                }
                self.clearable.then(|| {
                    if ui.button("✕").clicked() {
                        *self.selected = None;
                        ui.ctx()
                            .data_mut(|d| d.remove::<String>(filter_key));
                        Popover::close_by_id(ui.ctx(), popover_id);
                    }
                });
            });
            if self.searchable || self.clearable {
                ui.separator();
            }

            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                let mut pending_header: Option<usize> = None;
                let mut visible_indices: Vec<usize> = Vec::new();
                let filter_lower = filter.to_lowercase();

                for (i, opt) in self.flat_options.iter().enumerate() {
                    match opt {
                        FlatOption::Header(_) => {
                            pending_header = Some(i);
                        }
                        FlatOption::Item { label, .. } => {
                            let matches = filter.is_empty()
                                || label.to_lowercase().contains(&filter_lower);
                            if matches {
                                if let Some(h) = pending_header.take() {
                                    visible_indices.push(h);
                                }
                                visible_indices.push(i);
                            }
                        }
                    }
                }

                for i in visible_indices {
                    match &self.flat_options[i] {
                        FlatOption::Header(text) => {
                            ui.label(egui::RichText::new(text).strong().color(t.text.secondary));
                        }
                        FlatOption::Item { label, index } => {
                            if ui
                                .selectable_label(*self.selected == Some(*index), label)
                                .clicked()
                            {
                                *self.selected = Some(*index);
                                ui.ctx()
                                    .data_mut(|d| d.remove::<String>(filter_key));
                                Popover::close_by_id(ui.ctx(), popover_id);
                            }
                        }
                    }
                }
            });
        });

        button_response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_defaults() {
        let mut sel = None;
        let s = Select::new("test", &mut sel, &["A", "B"]);
        assert_eq!(s.placeholder, "Select…");
        assert!(!s.searchable);
        assert!(!s.clearable);
        assert_eq!(s.flat_options.len(), 2);
    }

    #[test]
    fn builder_placeholder() {
        let mut sel = None;
        let s = Select::new("test", &mut sel, &["A"]).placeholder("Pick one");
        assert_eq!(s.placeholder, "Pick one");
    }

    #[test]
    fn builder_searchable() {
        let mut sel = None;
        let s = Select::new("test", &mut sel, &["A"]).searchable(true);
        assert!(s.searchable);
    }

    #[test]
    fn builder_clearable() {
        let mut sel = None;
        let s = Select::new("test", &mut sel, &["A"]).clearable(true);
        assert!(s.clearable);
    }

    #[test]
    fn grouped_constructor() {
        let groups: &[(&str, &[&str])] =
            &[("Fruits", &["Apple", "Banana"]), ("Veg", &["Carrot"])];
        let mut sel = None;
        let s = Select::grouped("test", &mut sel, groups);
        assert_eq!(s.flat_options.len(), 5);
        assert!(matches!(
            s.flat_options[0],
            FlatOption::Header(ref h) if h == "Fruits"
        ));
        assert!(matches!(
            s.flat_options[1],
            FlatOption::Item { ref label, .. } if label == "Apple"
        ));
        assert!(matches!(
            s.flat_options[2],
            FlatOption::Item { ref label, .. } if label == "Banana"
        ));
        assert!(matches!(
            s.flat_options[3],
            FlatOption::Header(ref h) if h == "Veg"
        ));
        assert!(matches!(
            s.flat_options[4],
            FlatOption::Item { ref label, .. } if label == "Carrot"
        ));
        assert_eq!(s.flat_options[4].index(), Some(2));
    }
}
