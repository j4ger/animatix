use super::*;

impl CellEditorState {
    /// Swap all index-based state for two cell positions (e.g. after reordering).
    pub fn swap_cell_indices(&mut self, a: usize, b: usize) {
        let swap_opt = |opt: &mut Option<usize>| {
            if let Some(v) = *opt {
                if v == a {
                    *opt = Some(b);
                } else if v == b {
                    *opt = Some(a);
                }
            }
        };
        swap_opt(&mut self.focused_cell);
        swap_opt(&mut self.scroll_to_cell);
        swap_opt(&mut self.highlighted_cell);
        swap_opt(&mut self.prev_focused_cell);
        swap_opt(&mut self.pending_cursor_cell);
        swap_opt(&mut self.editing_timestamp_cell);
        if let Some(action) = self.pending_action.as_mut() {
            action.swap_indices(a, b);
        }

        let swap_set = |set: &mut std::collections::HashSet<usize>| {
            let has_a = set.contains(&a);
            let has_b = set.contains(&b);
            if has_a && !has_b {
                set.remove(&a);
                set.insert(b);
            } else if !has_a && has_b {
                set.remove(&b);
                set.insert(a);
            }
        };
        swap_set(&mut self.collapsed_cells);
        swap_set(&mut self.error_cells);
        swap_set(&mut self.warning_cells);
    }
}
