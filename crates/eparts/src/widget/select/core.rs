//! Constructors and builder setters for [`Select`].

use super::{FlatOption, Select};
use crate::widget::IntoWidgetId;

impl<'a> Select<'a> {
    /// Create a new flat `Select`.
    pub fn new<T: AsRef<str> + 'a>(
        id: impl IntoWidgetId,
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
            id: id.into_widget_id(),
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
        id: impl IntoWidgetId,
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
            id: id.into_widget_id(),
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
