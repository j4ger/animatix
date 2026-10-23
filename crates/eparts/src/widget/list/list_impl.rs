use super::*;

impl<'a> List<'a> {
    pub fn new(items: &'a [&'a str]) -> Self {
        Self {
            items,
            row_height: ROW_M,
            type_ahead_timeout: Duration::from_secs_f64(0.8),
        }
    }

    /// Row height in pixels (default: [`ROW_M`]).
    pub fn row_height(mut self, h: f32) -> Self {
        self.row_height = h;
        self
    }

    /// Duration after which the type-ahead buffer resets (default: 800 ms).
    pub fn type_ahead_timeout(mut self, d: Duration) -> Self {
        self.type_ahead_timeout = d;
        self
    }

    /// Render the list.
    ///
    /// The body is wrapped in a vertical [`egui::ScrollArea`] and only rows
    /// intersecting the scroll viewport are painted / interacted with
    /// (virtualized). The full-height space is still allocated inside the
    /// scroll area so the scrollbar range reflects every item.
    pub fn show(self, ui: &mut egui::Ui, id_source: impl std::hash::Hash) -> ListResponse {
        let list_id = Id::new(id_source);
        let num_items = self.items.len();
        let row_h = density(ui).scale(self.row_height);
        let timeout_secs = self.type_ahead_timeout.as_secs_f64();

        // ── Read / initialise Memory ────────────────────────────────
        let mut sel_idx: Option<usize> = ui
            .ctx()
            .data(|d| d.get_temp::<Option<usize>>(list_id.with("sel_idx")))
            .unwrap_or(None);
        if let Some(i) = sel_idx {
            if num_items > 0 && i >= num_items {
                sel_idx = Some(num_items - 1);
            }
        }

        let mut ta_buffer: String = ui
            .ctx()
            .data(|d| d.get_temp::<String>(list_id.with("ta_buf")))
            .unwrap_or_default();
        let mut ta_time: f64 =
            ui.ctx().data(|d| d.get_temp::<f64>(list_id.with("ta_time"))).unwrap_or(0.0);
        let now = ui.input(|i| i.time);

        // Scroll region honours the caller's available height, shrinking to the
        // content when everything fits.
        let scroll_height = ui.available_height();

        let output = egui::ScrollArea::vertical()
            .id_salt(list_id.with("__scroll"))
            .auto_shrink([false, true])
            .max_height(scroll_height)
            .show_viewport(ui, |ui, viewport| {
                // ── Allocate the full content rect ──────────────────
                // Keep the full-height allocation *inside* the scroll area so the
                // scrollbar range covers every row, even though only the visible
                // window is painted below.
                let total_height = num_items as f32 * row_h;
                let (_, outer_rect) =
                    ui.allocate_space(egui::vec2(ui.available_width(), total_height));
                let outer_resp = ui.interact(outer_rect, list_id, Sense::click());

                let mut has_focus = outer_resp.has_focus();
                if outer_resp.clicked() {
                    ui.memory_mut(|m| m.request_focus(list_id));
                    has_focus = true;
                }
                if has_focus {
                    // While the list is focused, arrow keys navigate it — they must
                    // not also move focus to a sibling via egui's focus traversal.
                    ui.memory_mut(|m| {
                        m.set_focus_lock_filter(
                            list_id,
                            egui::EventFilter {
                                horizontal_arrows: true,
                                vertical_arrows: true,
                                tab: false,
                                escape: false,
                            },
                        );
                    });
                }

                // ── Keyboard events (focused only, consumed) ────────
                let mut action: Option<ListAction> = None;
                let mut nav_moved = false;

                if has_focus {
                    ui.input_mut(|i| {
                        i.events.retain(|ev| match ev {
                            egui::Event::Key {
                                key: egui::Key::ArrowDown,
                                pressed: true,
                                ..
                            } if num_items > 0 => {
                                sel_idx =
                                    Some(sel_idx.map(|i| (i + 1).min(num_items - 1)).unwrap_or(0));
                                nav_moved = true;
                                false
                            },
                            egui::Event::Key {
                                key: egui::Key::ArrowUp,
                                pressed: true,
                                ..
                            } if num_items > 0 => {
                                sel_idx = Some(sel_idx.map(|i| i.saturating_sub(1)).unwrap_or(0));
                                nav_moved = true;
                                false
                            },
                            egui::Event::Key {
                                key: egui::Key::Home,
                                pressed: true,
                                ..
                            } if num_items > 0 => {
                                sel_idx = Some(0);
                                nav_moved = true;
                                false
                            },
                            egui::Event::Key {
                                key: egui::Key::End,
                                pressed: true,
                                ..
                            } if num_items > 0 => {
                                sel_idx = Some(num_items - 1);
                                nav_moved = true;
                                false
                            },
                            egui::Event::Key {
                                key: egui::Key::Enter,
                                pressed: true,
                                ..
                            } => {
                                if let Some(idx) = sel_idx {
                                    action = Some(ListAction::Confirmed(idx));
                                }
                                false
                            },
                            egui::Event::Text(text) => {
                                if now - ta_time > timeout_secs {
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
                                        let found = self.items[start..num_items]
                                            .iter()
                                            .enumerate()
                                            .find(|(_, s)| s.to_lowercase().starts_with(&prefix))
                                            .map(|(i, _)| start + i);
                                        let found = found.or_else(|| {
                                            self.items[0..start.min(num_items)]
                                                .iter()
                                                .enumerate()
                                                .find(|(_, s)| {
                                                    s.to_lowercase().starts_with(&prefix)
                                                })
                                                .map(|(i, _)| i)
                                        });
                                        sel_idx = found;
                                        nav_moved = true;
                                    }
                                }
                                false
                            },
                            _ => true,
                        });
                    });
                }

                // ── Click selection ─────────────────────────────────
                if outer_resp.clicked() {
                    if let Some(clicked_idx) =
                        row_at(outer_resp.interact_pointer_pos(), outer_rect, row_h)
                    {
                        if clicked_idx < num_items {
                            sel_idx = Some(clicked_idx);
                            if action.is_none() {
                                action = Some(ListAction::Clicked(clicked_idx));
                            }
                        }
                    }
                }

                // ── Virtualized window ──────────────────────────────
                // `viewport` is content-relative (min == ZERO at the top), which
                // lines up 1:1 with row offsets from `outer_rect.min.y`.
                let window = visible_row_range(viewport, row_h, num_items);

                if ui.is_rect_visible(outer_rect) {
                    // Clip row painting to the intersection of the content rect and
                    // the visible scroll viewport (the parent clip already does this,
                    // but be explicit so painting can never escape the viewport).
                    let clip = outer_rect.intersect(ui.clip_rect());
                    let painter = ui.painter_at(clip);

                    for idx in window.clone() {
                        let row_rect = row_rect_for(outer_rect, row_h, idx);
                        let is_selected = sel_idx == Some(idx);
                        let row_resp = ui.interact(row_rect, list_id.with(idx), Sense::click());

                        // Row widgets sit above the outer frame, so they win the hit
                        // test; route their clicks back to the list and claim focus.
                        if row_resp.clicked() {
                            sel_idx = Some(idx);
                            ui.memory_mut(|m| m.request_focus(list_id));
                            nav_moved = true;
                            if action.is_none() {
                                action = Some(ListAction::Clicked(idx));
                            }
                        }

                        let _ = Row::new(self.items[idx])
                            .height(row_h)
                            .zebra(idx % 2 == 0)
                            .selected(is_selected)
                            .show_in_rect(ui, row_rect, row_resp, list_id.with(idx), &painter);
                    }

                    // Keyboard navigation moved the selection: bring it into view.
                    if nav_moved {
                        if let Some(idx) = sel_idx {
                            ui.scroll_to_rect(row_rect_for(outer_rect, row_h, idx), None);
                        }
                    }
                }

                persist_list_state(ui.ctx(), list_id, sel_idx, &ta_buffer, ta_time);

                ListResponse {
                    action,
                    selected_index: sel_idx,
                }
            });

        output.inner
    }
}

// ── SearchableList ───────────────────────────────────────────────────
