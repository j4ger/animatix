use super::options::*;
use super::*;
use crate::widget::traits::finish_response;

/// Per-frame immutable state threaded through the Select render phases.
///
/// The widget is immediate-mode: this struct is constructed once per frame in
/// [`Widget::ui`] and borrowed by the phase helpers. `selected` stays outside
/// it because the dropdown mutably writes the app-owned selection.
struct SelectFrame<'a> {
    flat_options: &'a [FlatOption],
    searchable: bool,
    clearable: bool,
    t: crate::tokens::theme::Theme,
    s: crate::tokens::spatial::Spatial,
    d: crate::tokens::spatial::Density,
    popover_id: egui::Id,
    filter_key: egui::Id,
    highlight_key: egui::Id,
    scroll_key: egui::Id,
    was_open_key: egui::Id,
    trigger_id: egui::Id,
    current_label: String,
}

impl SelectFrame<'_> {
    /// Size the trigger from the available space and the selected label.
    ///
    /// `ui.add_sized(size, Select)` builds a justified child Ui; honour that
    /// exact rect. Otherwise size to the row height and fill available width.
    fn trigger_size(&self, ui: &egui::Ui) -> Vec2 {
        let avail = ui.available_size();
        let row_h = self.d.scale(ROW_M).max(ui.spacing().interact_size.y);
        let layout = *ui.layout();
        let constrained = layout.main_justify || layout.cross_justify();
        let label_galley = ui.painter().layout_no_wrap(
            self.current_label.clone(),
            TextRole::BodyS.font_id(),
            self.t.palette.text.secondary,
        );
        let caret_w = ui
            .painter()
            .layout_no_wrap(
                egui_phosphor::regular::CARET_DOWN.to_owned(),
                TextRole::Body.font_id(),
                self.t.palette.text.secondary,
            )
            .size()
            .x;
        let content_w = label_galley.size().x + caret_w + self.s.space_3 * 3.0;
        let width = if avail.x.is_finite() && avail.x > 0.0 {
            avail.x
        } else {
            content_w
        };
        let height = if constrained && avail.y.is_finite() && avail.y > 0.0 {
            avail.y
        } else {
            row_h
        };
        Vec2::new(width, height)
    }

    /// Consume Enter/Space before the trigger is interacted with.
    ///
    /// egui synthesizes a primary click for a focused click-sense widget on
    /// Enter/Space. While the popover is open those keys mean "commit", so
    /// consume them *before* interacting — otherwise the synthesized click
    /// would re-toggle the popover open right after we close it.
    fn consume_commit_keys(&self, ui: &mut egui::Ui, keyboard_active: bool) -> bool {
        let mut key_commit = false;
        if keyboard_active {
            ui.input_mut(|i| {
                i.events.retain(|ev| match ev {
                    egui::Event::Key {
                        key: Key::Enter,
                        pressed: true,
                        ..
                    }
                    | egui::Event::Key {
                        key: Key::Space,
                        pressed: true,
                        ..
                    } => {
                        key_commit = true;
                        false
                    },
                    _ => true,
                });
            });
        }
        key_commit
    }

    /// Allocate the trigger rect and run its click/focus interaction.
    fn interact_trigger(&self, ui: &mut egui::Ui, size: Vec2) -> (Rect, Response, bool) {
        let (_, rect) = ui.allocate_space(size);
        let response = ui.interact(rect, self.trigger_id, Sense::click());
        let mut has_focus = response.has_focus();
        if response.clicked() {
            ui.memory_mut(|m| m.request_focus(self.trigger_id));
            has_focus = true;
        }
        if has_focus {
            // While the trigger holds focus, arrows/escape act on the widget and
            // must not move focus to a sibling via egui's focus traversal.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    self.trigger_id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        tab: false,
                        escape: true,
                    },
                );
            });
        }
        (rect, response, has_focus)
    }

    /// Read and clamp the keyboard highlight for this frame.
    ///
    /// The highlight only exists while the dropdown is open; a closed Select
    /// keeps no keyboard highlight (and clears any stale one).
    fn resolve_highlight(
        &self,
        ui: &egui::Ui,
        popover_open: bool,
        selected: Option<usize>,
        visible: &[usize],
    ) -> Option<usize> {
        let mut highlighted: Option<usize> = if popover_open {
            ui.ctx()
                .data(|dd| dd.get_temp::<Option<usize>>(self.highlight_key))
                .unwrap_or(None)
        } else {
            None
        };
        if popover_open {
            // Clamp a stale / filtered-out highlight back to a visible row.
            if highlighted.is_some_and(|h| !visible.contains(&h)) {
                highlighted = None;
            }
            if !visible.is_empty() {
                if highlighted.is_none() {
                    highlighted = selected
                        .and_then(|sel| {
                            visible
                                .iter()
                                .copied()
                                .find(|&p| self.flat_options[p].index() == Some(sel))
                        })
                        .or_else(|| visible.first().copied());
                }
            } else {
                highlighted = None;
            }
        }
        highlighted
    }

    /// Resolve the one-shot "scroll selection into view" request.
    ///
    /// Opening (by click or otherwise) should scroll the selection into view.
    fn scroll_requested(
        &self,
        ui: &egui::Ui,
        was_open: bool,
        popover_open: bool,
        clicked: bool,
    ) -> bool {
        let newly_open = (!was_open && popover_open) || (clicked && !popover_open);
        let mut pending_scroll = newly_open;
        ui.ctx().data_mut(|dd| {
            if newly_open {
                dd.insert_temp(self.scroll_key, true);
            }
        });
        if ui.ctx().data(|dd| dd.get_temp::<bool>(self.scroll_key).unwrap_or(false)) {
            pending_scroll = true;
        }
        pending_scroll
    }

    /// Apply the trigger's visual slots and paint the label/caret.
    fn paint_trigger(
        &self,
        ui: &egui::Ui,
        rect: Rect,
        response: &Response,
        has_focus: bool,
        selected: Option<usize>,
    ) {
        let radius = egui::CornerRadius::same(RADIUS_M as u8);
        let trigger_hovered = response.hovered();
        let slot = if !ui.is_enabled() {
            self.t.components.input.disabled
        } else if has_focus {
            self.t.components.input.focus
        } else if trigger_hovered {
            self.t.components.input.hover
        } else {
            self.t.components.input.normal
        };
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, radius, slot.bg);
        painter.rect_stroke(
            rect,
            radius,
            egui::Stroke::new(STROKE_WIDTH, slot.border),
            egui::StrokeKind::Inside,
        );
        if has_focus && ui.is_enabled() {
            // Mirror button.rs: focus ring inset by 1px, painted inside.
            painter.rect_stroke(
                rect.shrink(1.0),
                radius,
                egui::Stroke::new(STROKE_WIDTH, self.t.focus_ring()),
                egui::StrokeKind::Inside,
            );
        }

        let fg = if !ui.is_enabled() {
            self.t.components.input.disabled.fg
        } else if selected.is_some() {
            slot.fg
        } else {
            self.t.palette.text.muted
        };
        let caret_galley = ui.painter().layout_no_wrap(
            egui_phosphor::regular::CARET_DOWN.to_owned(),
            TextRole::Body.font_id(),
            fg,
        );
        let text_x = rect.min.x + self.s.space_3;
        let avail_text =
            (rect.width() - self.s.space_3 * 2.0 - caret_galley.size().x - self.s.space_2).max(0.0);
        let label_galley = ui.painter().layout(
            self.current_label.clone(),
            TextRole::BodyS.font_id(),
            fg,
            avail_text,
        );
        painter.galley(
            egui::pos2(text_x, rect.center().y - label_galley.size().y / 2.0),
            label_galley,
            fg,
        );
        painter.galley(
            egui::pos2(
                rect.max.x - self.s.space_3 - caret_galley.size().x,
                rect.center().y - caret_galley.size().y / 2.0,
            ),
            caret_galley,
            fg,
        );
    }

    /// Render the dropdown panel (search/clear row, grouped rows, scroll).
    fn show_dropdown(
        &self,
        ui: &mut egui::Ui,
        response: &Response,
        selected: &mut Option<usize>,
        highlighted: Option<usize>,
    ) {
        let popover = Popover::new(self.popover_id).below().max_width(260.0);
        let _popover_resp = popover.show(ui, response, |ui| {
            let mut filter: String =
                ui.ctx().data(|dd| dd.get_temp::<String>(self.filter_key).unwrap_or_default());
            let should_scroll =
                ui.ctx().data(|dd| dd.get_temp::<bool>(self.scroll_key).unwrap_or(false));

            ui.horizontal(|ui| {
                if self.searchable {
                    let tf = TextField::new(&mut filter)
                        .placeholder("Search…")
                        .desired_width(ui.available_width());
                    let _ = tf.show(ui);
                }
                if self.clearable
                    && ui
                        .add(Button::icon(egui_phosphor::regular::X).with_tooltip("Clear"))
                        .clicked()
                {
                    *selected = None;
                    ui.ctx().data_mut(|dd| dd.remove::<String>(self.filter_key));
                    Popover::close_by_id(ui.ctx(), self.popover_id);
                }
            });
            if self.searchable || self.clearable {
                ui.separator();
            }

            let visible_now = visible_positions(self.flat_options, &filter);
            let mut scroll_target: Option<Rect> = None;

            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                for pos in visible_now {
                    match &self.flat_options[pos] {
                        FlatOption::Header(text) => {
                            ui.label(
                                egui::RichText::new(text)
                                    .strong()
                                    .color(self.t.palette.text.secondary),
                            );
                        },
                        FlatOption::Item { label, index } => {
                            let row_h = ui.spacing().interact_size.y.max(ROW_M * 0.8);
                            let (row_rect, row_response) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), row_h),
                                Sense::click(),
                            );
                            let is_selected = *selected == Some(*index);
                            let is_highlighted = highlighted == Some(pos);
                            paint_option_row(
                                ui,
                                row_rect,
                                row_response.hovered(),
                                is_selected,
                                is_highlighted,
                                label,
                                &self.t,
                            );
                            if row_response.clicked() {
                                *selected = Some(*index);
                                ui.ctx().data_mut(|dd| {
                                    dd.remove::<String>(self.filter_key);
                                    dd.insert_temp(self.highlight_key, Some(pos));
                                });
                                Popover::close_by_id(ui.ctx(), self.popover_id);
                            }
                            if is_highlighted && should_scroll {
                                scroll_target = Some(row_rect);
                            }
                        },
                    }
                }
            });

            if let Some(target) = scroll_target {
                ui.scroll_to_rect(target, Some(Align::Center));
            }
            // Clear the one-shot scroll request whether or not the highlighted
            // row was visible this frame.
            ui.ctx().data_mut(|dd| dd.insert_temp(self.scroll_key, false));

            ui.ctx().data_mut(|dd| {
                dd.insert_temp(self.filter_key, filter);
            });
        });
    }

    /// Persist open state for the next frame's open-transition, then finish
    /// the response and attach the accessibility info.
    fn finish(&self, ui: &egui::Ui, response: Response, selected: Option<usize>) -> Response {
        let still_open = Popover::is_open(ui.ctx(), self.popover_id);
        ui.ctx().data_mut(|dd| dd.insert_temp(self.was_open_key, still_open));

        let response = finish_response(ui, response, "");
        let accessible = self.current_label.clone();
        response.widget_info(|| {
            WidgetInfo::selected(
                Role::ComboBox,
                ui.is_enabled(),
                selected.is_some(),
                accessible.clone(),
            )
        });
        response
    }
}

/// Handle arrow / Home / End / Escape while the trigger is focused.
///
/// Returns `true` when Escape requested closing the popover.
fn handle_nav_keys(
    ui: &mut egui::Ui,
    has_focus: bool,
    popover_open: bool,
    visible: &[usize],
    highlighted: &mut Option<usize>,
    pending_scroll: &mut bool,
) -> bool {
    let mut close_requested = false;
    if has_focus {
        ui.input_mut(|i| {
            i.events.retain(|ev| match ev {
                egui::Event::Key {
                    key: Key::ArrowDown,
                    pressed: true,
                    ..
                } if !visible.is_empty() => {
                    *highlighted = step_highlight(visible, *highlighted, 1);
                    *pending_scroll = true;
                    false
                },
                egui::Event::Key {
                    key: Key::ArrowUp,
                    pressed: true,
                    ..
                } if !visible.is_empty() => {
                    *highlighted = step_highlight(visible, *highlighted, -1);
                    *pending_scroll = true;
                    false
                },
                egui::Event::Key {
                    key: Key::Home,
                    pressed: true,
                    ..
                } if !visible.is_empty() => {
                    *highlighted = visible.first().copied();
                    *pending_scroll = true;
                    false
                },
                egui::Event::Key {
                    key: Key::End,
                    pressed: true,
                    ..
                } if !visible.is_empty() => {
                    *highlighted = visible.last().copied();
                    *pending_scroll = true;
                    false
                },
                // Escape closes even when the filter matches nothing.
                egui::Event::Key {
                    key: Key::Escape,
                    pressed: true,
                    ..
                } if popover_open => {
                    close_requested = true;
                    false
                },
                _ => true,
            });
        });
    }
    close_requested
}

impl Widget for Select<'_> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let Select {
            id,
            selected,
            flat_options,
            placeholder,
            searchable,
            clearable,
        } = self;

        let t = theme(ui);
        let s = crate::spatial(ui);
        let d = density(ui);

        let frame = SelectFrame {
            flat_options: &flat_options,
            searchable,
            clearable,
            t,
            s,
            d,
            popover_id: id.with("__popover"),
            filter_key: id.with("__filter"),
            highlight_key: id.with("__highlight"),
            scroll_key: id.with("__scroll"),
            was_open_key: id.with("__was_open"),
            trigger_id: id.with("__trigger"),
            current_label: match *selected {
                Some(idx) => flat_options
                    .iter()
                    .find_map(|opt| opt.label().filter(|_| opt.index() == Some(idx)))
                    .unwrap_or(placeholder),
                None => placeholder,
            }
            .to_owned(),
        };

        // ── Layout / trigger geometry ────────────────────────────────────
        let size = frame.trigger_size(ui);

        // ── Keyboard pre-pass ────────────────────────────────────────────
        let was_open = ui.ctx().data(|dd| dd.get_temp::<bool>(frame.was_open_key).unwrap_or(false));
        let popover_open = Popover::is_open(ui.ctx(), frame.popover_id);
        let trigger_focused = ui.memory(|m| m.has_focus(frame.trigger_id));
        let keyboard_active = trigger_focused && popover_open;

        let key_commit = frame.consume_commit_keys(ui, keyboard_active);

        // ── Trigger allocation & interaction ─────────────────────────────
        let (rect, response, has_focus) = frame.interact_trigger(ui, size);

        // ── Highlight state + keyboard navigation ────────────────────────
        let filter: String =
            ui.ctx().data(|dd| dd.get_temp::<String>(frame.filter_key).unwrap_or_default());
        let visible = visible_positions(frame.flat_options, &filter);

        let mut highlighted = frame.resolve_highlight(ui, popover_open, *selected, &visible);
        let mut pending_scroll =
            frame.scroll_requested(ui, was_open, popover_open, response.clicked());

        let mut close_requested = handle_nav_keys(
            ui,
            has_focus,
            popover_open,
            &visible,
            &mut highlighted,
            &mut pending_scroll,
        );
        let mut commit_index: Option<usize> = None;

        if keyboard_active && key_commit {
            commit_index = highlighted.and_then(|h| flat_options.get(h)).and_then(|o| o.index());
            close_requested = true;
        }

        if let Some(index) = commit_index {
            *selected = Some(index);
            ui.ctx().data_mut(|dd| dd.remove::<String>(frame.filter_key));
        }
        if close_requested {
            Popover::close_by_id(ui.ctx(), frame.popover_id);
            ui.ctx().data_mut(|dd| dd.remove::<String>(frame.filter_key));
        }

        ui.ctx().data_mut(|dd| {
            dd.insert_temp(frame.highlight_key, highlighted);
            if pending_scroll {
                dd.insert_temp(frame.scroll_key, true);
            }
        });

        // ── Dropdown panel ───────────────────────────────────────────────
        frame.show_dropdown(ui, &response, &mut *selected, highlighted);

        // ── Trigger visuals ──────────────────────────────────────────────
        frame.paint_trigger(ui, rect, &response, has_focus, *selected);

        // Persist open state and attach a11y info.
        frame.finish(ui, response, *selected)
    }
}
