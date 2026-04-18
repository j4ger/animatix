use super::{Easing, PlacementMode, Timeline};

impl Timeline {
    pub(super) fn apply_container_layout(
        &mut self,
        container_label: &str,
        container_ty: &str,
        time_ms: f64,
        gap: f32,
        align: Option<&str>,
        cols: Option<usize>,
    ) {
        let children = if let Some(node) = self.nodes.get(container_label) {
            node.children.clone()
        } else {
            return;
        };

        let is_row = container_ty == "Row";
        let is_col = container_ty == "Col";
        let is_stack = container_ty == "Stack";
        let is_grid = container_ty == "Grid";

        if !is_row && !is_col && !is_stack && !is_grid {
            return;
        }

        let child_extents: Vec<(f32, f32)> = children
            .iter()
            .filter_map(|cl| {
                self.tracks.get(cl).map(|t| {
                    let s = t.size.last_value();
                    (s[0] * 2.0, s[1] * 2.0)
                })
            })
            .collect();

        let t_ms = time_ms as u64;

        if is_stack {
            for child_label in &children {
                if let Some(track) = self.tracks.get_mut(child_label) {
                    if track.placement_mode.last_value() == PlacementMode::LayoutManaged {
                        track
                            .position
                            .add_keyframe(t_ms, [0.0, 0.0], Easing::Linear);
                    }
                }
            }
            return;
        }

        if is_grid {
            let cols = cols.unwrap_or(1).max(1);
            let rows = children.len().div_ceil(cols);
            let mut col_widths = vec![0.0f32; cols];
            let mut row_heights = vec![0.0f32; rows.max(1)];

            for (index, (child_w, child_h)) in child_extents.iter().copied().enumerate() {
                let row = index / cols;
                let col = index % cols;
                col_widths[col] = col_widths[col].max(child_w);
                row_heights[row] = row_heights[row].max(child_h);
            }

            let total_width =
                col_widths.iter().sum::<f32>() + gap * (col_widths.len().saturating_sub(1) as f32);
            let total_height = row_heights.iter().sum::<f32>()
                + gap * (row_heights.len().saturating_sub(1) as f32);

            let mut row_starts = Vec::with_capacity(row_heights.len());
            let mut current_y = -total_height / 2.0;
            for row_height in &row_heights {
                row_starts.push(current_y);
                current_y += *row_height + gap;
            }

            let mut col_starts = Vec::with_capacity(col_widths.len());
            let mut current_x = -total_width / 2.0;
            for col_width in &col_widths {
                col_starts.push(current_x);
                current_x += *col_width + gap;
            }

            for (index, child_label) in children.iter().enumerate() {
                if let Some(track) = self.tracks.get_mut(child_label) {
                    if track.placement_mode.last_value() != PlacementMode::LayoutManaged {
                        continue;
                    }

                    let row = index / cols;
                    let col = index % cols;
                    if row >= row_heights.len() || col >= col_widths.len() {
                        continue;
                    }

                    let x = col_starts[col] + col_widths[col] / 2.0;
                    let y = row_starts[row] + row_heights[row] / 2.0;
                    track.position.add_keyframe(t_ms, [x, y], Easing::Linear);
                }
            }
            return;
        }

        let mut total_extent = 0.0f32;
        let mut max_cross_extent = 0.0f32;
        for (w, h) in child_extents.iter().copied() {
            if is_row {
                total_extent += w;
                if max_cross_extent < h {
                    max_cross_extent = h;
                }
            } else {
                total_extent += h;
                if max_cross_extent < w {
                    max_cross_extent = w;
                }
            }
        }

        if !children.is_empty() && children.len() > 1 {
            total_extent += gap * (children.len() as f32 - 1.0);
        }

        let cross_offset = match align.unwrap_or("center") {
            "start" => -max_cross_extent / 2.0,
            "end" => max_cross_extent / 2.0,
            _ => 0.0,
        };

        let main_start = -total_extent / 2.0;
        let mut offset = 0.0f32;

        for (i, child_label) in children.iter().enumerate() {
            if let Some(track) = self.tracks.get_mut(child_label) {
                let (child_w, child_h) = child_extents[i];

                let (x, y) = if is_row {
                    let cx = main_start + offset + child_w / 2.0;
                    offset += child_w;
                    if i < children.len() - 1 {
                        offset += gap;
                    }
                    let cy = match align.unwrap_or("center") {
                        "start" => cross_offset + child_h / 2.0,
                        "end" => cross_offset - child_h / 2.0,
                        _ => cross_offset,
                    };
                    (cx, cy)
                } else {
                    let cy = main_start + offset + child_h / 2.0;
                    offset += child_h;
                    if i < children.len() - 1 {
                        offset += gap;
                    }
                    let cx = match align.unwrap_or("center") {
                        "start" => cross_offset + child_w / 2.0,
                        "end" => cross_offset - child_w / 2.0,
                        _ => cross_offset,
                    };
                    (cx, cy)
                };

                if track.placement_mode.last_value() == PlacementMode::LayoutManaged {
                    track.position.add_keyframe(t_ms, [x, y], Easing::Linear);
                }
            }
        }
    }
}
