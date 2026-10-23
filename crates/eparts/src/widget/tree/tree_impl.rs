use super::*;

impl<'a> Tree<'a> {
    pub fn new(items: &'a [TreeItem]) -> Self {
        Self {
            items,
            row_height: ROW_M,
            indent_step: SPACE_4,
        }
    }

    /// Row height in pixels (default: [`ROW_M`]).
    pub fn row_height(mut self, h: f32) -> Self {
        self.row_height = h;
        self
    }

    /// Indent increment per depth level in pixels (default: [`SPACE_4`]).
    pub fn indent_step(mut self, px: f32) -> Self {
        self.indent_step = px;
        self
    }

    /// Render the tree and return [`TreeResponse`].
    ///
    /// `id_source` seeds the `egui::Id` that guards the widget's Memory entries.
    ///
    /// The body is wrapped in a vertical [`egui::ScrollArea`] and only rows
    /// intersecting the scroll viewport are painted / interacted with
    /// (virtualized). The full-height space is still allocated inside the
    /// scroll area so the scrollbar range reflects every row.
    pub fn show(self, ui: &mut egui::Ui, id_source: impl std::hash::Hash) -> TreeResponse {
        let tree_id = Id::new(id_source);
        let row_h = density(ui).scale(self.row_height);
        let indent = density(ui).scale(self.indent_step);
        let scroll_height = ui.available_height();

        let output = egui::ScrollArea::vertical()
            .id_salt(tree_id.with("__scroll"))
            .auto_shrink([false, true])
            .max_height(scroll_height)
            .show_viewport(ui, |ui, viewport| {
                self.show_inner(ui, tree_id, row_h, indent, viewport)
            });

        let (action, response, viewport_response, selected_index) = output.inner;
        TreeResponse {
            action,
            response,
            viewport_response,
            selected_index,
        }
    }

    /// Inner render: allocates the full content rect and paints the visible
    /// window. Split out so the scroll area can wrap it.
    ///
    /// Returns `(action, content_response, viewport_response, selected_index)`.
    fn show_inner(
        self,
        ui: &mut egui::Ui,
        tree_id: Id,
        row_h: f32,
        indent: f32,
        viewport: egui::Rect,
    ) -> (Option<TreeAction>, Response, Response, Option<usize>) {
        let num_items = self.items.len();

        // ── Read / initialise Memory state ──────────────────────────
        let mut sel_idx: Option<usize> = ui
            .ctx()
            .data(|d| d.get_temp::<Option<usize>>(tree_id.with("sel_idx")))
            .unwrap_or(None);

        // Clamp stale selection to the current list length.
        if let Some(idx) = sel_idx {
            if num_items == 0 {
                sel_idx = None;
            } else if idx >= num_items {
                sel_idx = Some(num_items - 1);
            }
        }

        // Type-ahead buffer + last-keystroke time (lives in Memory).
        let mut ta_buffer: String = ui
            .ctx()
            .data(|d| d.get_temp::<String>(tree_id.with("ta_buf")))
            .unwrap_or_default();
        let mut ta_time: f64 =
            ui.ctx().data(|d| d.get_temp::<f64>(tree_id.with("ta_time"))).unwrap_or(0.0);
        let now = ui.input(|i| i.time);
        const TYPE_AHEAD_TIMEOUT: f64 = 0.8;

        // ── Accumulate keyboard events ──────────────────────────────
        let mut toggled_id: Option<TreeId> = None;
        let mut selected_id: Option<TreeId> = None;
        // Set whenever navigation (keyboard or click) changes the selection, so
        // the selected row can be scrolled into view after painting.
        let mut nav_moved: bool;

        // ── Allocate the outer frame & resolve focus ────────────────
        // The outer response owns keyboard focus, so allocate before reading
        // events. Interact under `tree_id` (not an auto id) so focus and the
        // focus lock filter refer to the same stable id.
        let total_height = num_items as f32 * row_h;
        let (_, outer_rect) = ui.allocate_space(egui::vec2(ui.available_width(), total_height));
        let outer_resp = ui.interact(outer_rect, tree_id, Sense::click());
        // Response for the on-screen scroll viewport only. Registered *before*
        // the rows so it never steals their hover/click (later widgets sit on
        // top), but still covers the empty area below the last row.
        let viewport_resp = ui.interact(
            outer_rect.intersect(ui.clip_rect()),
            tree_id.with("__viewport"),
            Sense::hover(),
        );
        let mut has_focus = outer_resp.has_focus();
        if outer_resp.clicked() {
            ui.memory_mut(|m| m.request_focus(tree_id));
            has_focus = true;
        }
        if has_focus {
            // Arrow keys navigate the tree; keep egui's focus traversal from
            // also moving focus to a sibling widget on the same key press.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    tree_id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        tab: false,
                        escape: false,
                    },
                );
            });
        }

        // ── Keyboard events (focused only, consumed) ────────────────
        let sel_before_kb = sel_idx;
        if has_focus {
            ui.input_mut(|i| {
                i.events.retain(|ev| match ev {
                    egui::Event::Key {
                        key: egui::Key::ArrowDown,
                        pressed: true,
                        ..
                    } if num_items > 0 => {
                        let next = sel_idx.map(|i| (i + 1).min(num_items - 1)).unwrap_or(0);
                        sel_idx = Some(next);
                        selected_id = Some(self.items[next].id.clone());
                        false
                    },
                    egui::Event::Key {
                        key: egui::Key::ArrowUp,
                        pressed: true,
                        ..
                    } if num_items > 0 => {
                        let prev = sel_idx.map(|i| i.saturating_sub(1)).unwrap_or(0);
                        sel_idx = Some(prev);
                        selected_id = Some(self.items[prev].id.clone());
                        false
                    },
                    egui::Event::Key {
                        key: egui::Key::Home,
                        pressed: true,
                        ..
                    } if num_items > 0 => {
                        sel_idx = Some(0);
                        selected_id = Some(self.items[0].id.clone());
                        false
                    },
                    egui::Event::Key {
                        key: egui::Key::End,
                        pressed: true,
                        ..
                    } if num_items > 0 => {
                        sel_idx = Some(num_items - 1);
                        selected_id = Some(self.items[num_items - 1].id.clone());
                        false
                    },
                    egui::Event::Key {
                        key: egui::Key::ArrowLeft,
                        pressed: true,
                        ..
                    } => {
                        if let Some(idx) = sel_idx {
                            if let Some(item) = self.items.get(idx) {
                                if item.expanded && item.has_children {
                                    toggled_id = Some(item.id.clone());
                                } else if item.depth > 0 {
                                    // Walk backward to nearest ancestor.
                                    let target_depth = item.depth.saturating_sub(1);
                                    let mut parent_idx = idx;
                                    while parent_idx > 0 {
                                        parent_idx -= 1;
                                        if let Some(cand) = self.items.get(parent_idx) {
                                            if cand.depth == target_depth {
                                                sel_idx = Some(parent_idx);
                                                selected_id = Some(cand.id.clone());
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        false
                    },
                    egui::Event::Key {
                        key: egui::Key::ArrowRight,
                        pressed: true,
                        ..
                    } => {
                        if let Some(idx) = sel_idx {
                            if let Some(item) = self.items.get(idx) {
                                if !item.expanded && item.has_children {
                                    toggled_id = Some(item.id.clone());
                                } else if item.expanded && item.has_children {
                                    let child_idx = idx + 1;
                                    if child_idx < num_items
                                        && self.items[child_idx].depth == item.depth + 1
                                    {
                                        sel_idx = Some(child_idx);
                                        selected_id = Some(self.items[child_idx].id.clone());
                                    }
                                }
                            }
                        }
                        false
                    },
                    egui::Event::Key {
                        key: egui::Key::Enter,
                        pressed: true,
                        ..
                    } => {
                        if let Some(idx) = sel_idx {
                            if let Some(item) = self.items.get(idx) {
                                if item.has_children {
                                    toggled_id = Some(item.id.clone());
                                }
                            }
                        }
                        false
                    },
                    egui::Event::Text(text) => {
                        // Type-ahead: reset buffer on timeout.
                        if now - ta_time > TYPE_AHEAD_TIMEOUT {
                            ta_buffer.clear();
                        }
                        let ch = text.trim();
                        if !ch.is_empty() && !text.contains(|c: char| c.is_control()) {
                            ta_buffer.push_str(ch);
                            ta_buffer.truncate(64);
                            ta_time = now;

                            if !ta_buffer.is_empty() && num_items > 0 {
                                let prefix = ta_buffer.to_lowercase();
                                let start = sel_idx.map(|i| i + 1).unwrap_or(0);
                                let mut found = None;
                                for i in start..num_items {
                                    if self.items[i].label.to_lowercase().starts_with(&prefix) {
                                        found = Some(i);
                                        break;
                                    }
                                }
                                if found.is_none() {
                                    for i in 0..start.min(num_items) {
                                        if self.items[i].label.to_lowercase().starts_with(&prefix) {
                                            found = Some(i);
                                            break;
                                        }
                                    }
                                }
                                if let Some(idx) = found {
                                    sel_idx = Some(idx);
                                    selected_id = Some(self.items[idx].id.clone());
                                }
                            }
                        }
                        false
                    },
                    _ => true,
                });
            });
        }
        nav_moved = sel_idx != sel_before_kb;

        // ── Paint rows (virtualized to the visible viewport) ────────
        if ui.is_rect_visible(outer_rect) {
            // `viewport` is content-relative (min == ZERO at the top), matching
            // row offsets from `outer_rect.min.y`.
            let window = crate::widget::list::visible_row_range(viewport, row_h, num_items);

            // Clip row painting to the intersection of the content rect and the
            // scroll viewport; `painter_at(outer_rect)` alone would clip only to
            // the parent's clip rect and paint outside the scroll view.
            let clip = outer_rect.intersect(ui.clip_rect());
            let painter = ui.painter_at(clip);

            for idx in window.clone() {
                let item = &self.items[idx];
                let row_rect = crate::widget::list::row_rect_for(outer_rect, row_h, idx);
                let is_selected = sel_idx == Some(idx);
                let row_id = tree_id.with(idx);

                let row_resp = ui.interact(row_rect, row_id, Sense::click());
                if row_resp.clicked() {
                    sel_idx = Some(idx);
                    selected_id = Some(item.id.clone());
                    nav_moved = true;
                    // Rows sit above the outer frame in the hit-test order, so a
                    // row click is what claims keyboard focus for the tree.
                    ui.memory_mut(|m| m.request_focus(tree_id));
                }

                let _ = Row::new(&item.label)
                    .height(row_h)
                    .indent(item.depth as f32 * indent)
                    .zebra(idx % 2 == 0)
                    .has_children(item.has_children)
                    .expanded(item.expanded)
                    .selected(is_selected)
                    .show_in_rect(ui, row_rect, row_resp, row_id, &painter);
            }

            // Keyboard navigation moved the selection: bring it into view.
            if nav_moved {
                if let Some(idx) = sel_idx {
                    ui.scroll_to_rect(
                        crate::widget::list::row_rect_for(outer_rect, row_h, idx),
                        None,
                    );
                }
            }
        }

        // ── Persist Memory state ────────────────────────────────────
        ui.ctx().data_mut(|d| {
            d.insert_temp(tree_id.with("sel_idx"), sel_idx);
            d.insert_temp(tree_id.with("has_focus"), has_focus);
            d.insert_temp(tree_id.with("ta_buf"), ta_buffer);
            d.insert_temp(tree_id.with("ta_time"), ta_time);
        });

        (resolve_tree_action(toggled_id, selected_id), outer_resp, viewport_resp, sel_idx)
    }
}

// ── Internal helpers ─────────────────────────────────────────────────
