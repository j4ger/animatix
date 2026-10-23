use super::*;

impl<'a> SearchableList<'a> {
    pub fn new(items: &'a [&'a str]) -> Self {
        Self {
            items,
            placeholder: None,
            row_height: ROW_M,
            type_ahead_timeout: Duration::from_secs_f64(0.8),
        }
    }

    /// Placeholder text for the filter field.
    pub fn placeholder(mut self, text: &'a str) -> Self {
        self.placeholder = Some(text);
        self
    }

    /// Row height in pixels.
    pub fn row_height(mut self, h: f32) -> Self {
        self.row_height = h;
        self
    }

    /// Type-ahead timeout duration.
    pub fn type_ahead_timeout(mut self, d: Duration) -> Self {
        self.type_ahead_timeout = d;
        self
    }

    /// Render the searchable list.
    pub fn show(
        self,
        ui: &mut egui::Ui,
        id_source: impl std::hash::Hash,
    ) -> SearchableListResponse {
        let root_id = Id::new(id_source);

        // Filter + selection state in Memory.
        let mut filter: String = ui
            .ctx()
            .data(|d| d.get_temp::<String>(root_id.with("filter")))
            .unwrap_or_default();

        let sel_filtered: Option<usize> = ui
            .ctx()
            .data(|d| d.get_temp::<Option<usize>>(root_id.with("sel_filt")))
            .unwrap_or(None);

        // ── Filter TextField ────────────────────────────────────────
        ui.horizontal(|ui| {
            // Edit the persisted buffer in place so typing actually filters.
            let tf = TextField::new(&mut filter)
                .placeholder(self.placeholder.unwrap_or("Filter…"))
                .cleanable(true)
                .desired_width(ui.available_width());
            let _resp = tf.show(ui);
        });

        // ── Compute filtered items ──────────────────────────────────
        let lower_filter = filter.to_lowercase();
        let filtered: Vec<(usize, &str)> = self
            .items
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, label)| label.to_lowercase().contains(&lower_filter))
            .collect();

        let filtered_labels: Vec<&str> = filtered.iter().map(|(_, l)| *l).collect();

        // Selection defaults: first filtered item, or None.
        let default_sel: Option<usize> = if filtered_labels.is_empty() {
            None
        } else {
            Some(0)
        };

        // Clamp a persisted selection that may be stale after filter change.
        let sel_filtered: Option<usize> = sel_filtered
            .filter(|i| filtered_labels.is_empty() || *i < filtered_labels.len())
            .or(default_sel);

        // Persist the (possibly clamped) selection so the inner List picks it up.
        ui.ctx().data_mut(|d| {
            d.insert_temp(root_id.with("sel_filt"), sel_filtered);
        });

        // ── Inner List ──────────────────────────────────────────────
        let list_resp = List::new(&filtered_labels)
            .row_height(self.row_height)
            .type_ahead_timeout(self.type_ahead_timeout)
            .show(ui, root_id.with("inner"));

        // Map filtered index → original index.
        let confirmed_original = list_resp.action.and_then(|a| match a {
            ListAction::Confirmed(fi) | ListAction::Clicked(fi) => {
                filtered.get(fi).map(|(oi, _)| *oi)
            },
        });
        let selected_original =
            list_resp.selected_index.and_then(|fi| filtered.get(fi).map(|(oi, _)| *oi));

        // ── Persist filter ──────────────────────────────────────────
        ui.ctx().data_mut(|d| {
            d.insert_temp(root_id.with("filter"), filter);
        });

        SearchableListResponse {
            confirmed: confirmed_original,
            selected_index: selected_original,
            filtered_count: filtered_labels.len(),
        }
    }
}

/// Response from [`SearchableList::show`].
pub struct SearchableListResponse {
    /// Original (unfiltered) index of the confirmed item, or `None`.
    pub confirmed: Option<usize>,
    /// Original (unfiltered) index of the currently selected item, or `None`.
    pub selected_index: Option<usize>,
    /// Number of items matching the current filter.
    pub filtered_count: usize,
}
