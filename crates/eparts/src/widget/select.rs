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

        let popover_id = id.with("__popover");
        let filter_key = id.with("__filter");
        let highlight_key = id.with("__highlight");
        let scroll_key = id.with("__scroll");
        let was_open_key = id.with("__was_open");
        let trigger_id = id.with("__trigger");

        let current_label = match *selected {
            Some(idx) => flat_options
                .iter()
                .find_map(|opt| opt.label().filter(|_| opt.index() == Some(idx)))
                .unwrap_or(placeholder),
            None => placeholder,
        }
        .to_owned();

        // ── Layout / trigger geometry ────────────────────────────────────
        // `ui.add_sized(size, Select)` builds a justified child Ui; honour that
        // exact rect. Otherwise size to the row height and fill available width.
        let avail = ui.available_size();
        let row_h = d.scale(ROW_M).max(ui.spacing().interact_size.y);
        let layout = *ui.layout();
        let constrained = layout.main_justify || layout.cross_justify();
        let label_galley = ui.painter().layout_no_wrap(
            current_label.clone(),
            TextRole::BodyS.font_id(),
            t.palette.text.secondary,
        );
        let caret_w = ui
            .painter()
            .layout_no_wrap(
                egui_phosphor::regular::CARET_DOWN.to_owned(),
                TextRole::Body.font_id(),
                t.palette.text.secondary,
            )
            .size()
            .x;
        let content_w = label_galley.size().x + caret_w + s.space_3 * 3.0;
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

        // ── Keyboard pre-pass ────────────────────────────────────────────
        // egui synthesizes a primary click for a focused click-sense widget on
        // Enter/Space. While the popover is open those keys mean "commit", so
        // consume them *before* interacting — otherwise the synthesized click
        // would re-toggle the popover open right after we close it.
        let was_open = ui.ctx().data(|dd| dd.get_temp::<bool>(was_open_key).unwrap_or(false));
        let popover_open = Popover::is_open(ui.ctx(), popover_id);
        let trigger_focused = ui.memory(|m| m.has_focus(trigger_id));
        let keyboard_active = trigger_focused && popover_open;

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

        // ── Trigger allocation & interaction ─────────────────────────────
        let (_, rect) = ui.allocate_space(Vec2::new(width, height));
        let response = ui.interact(rect, trigger_id, Sense::click());
        let mut has_focus = response.has_focus();
        if response.clicked() {
            ui.memory_mut(|m| m.request_focus(trigger_id));
            has_focus = true;
        }
        if has_focus {
            // While the trigger holds focus, arrows/escape act on the widget and
            // must not move focus to a sibling via egui's focus traversal.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    trigger_id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        tab: false,
                        escape: true,
                    },
                );
            });
        }

        // ── Highlight state + keyboard navigation ────────────────────────
        let filter: String =
            ui.ctx().data(|dd| dd.get_temp::<String>(filter_key).unwrap_or_default());
        let visible = visible_positions(&flat_options, &filter);

        // The highlight only exists while the dropdown is open; a closed Select
        // keeps no keyboard highlight (and clears any stale one).
        let mut highlighted: Option<usize> = if popover_open {
            ui.ctx().data(|dd| dd.get_temp::<Option<usize>>(highlight_key)).unwrap_or(None)
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
                            visible.iter().copied().find(|&p| flat_options[p].index() == Some(sel))
                        })
                        .or_else(|| visible.first().copied());
                }
            } else {
                highlighted = None;
            }
        }

        // Opening (by click or otherwise) should scroll the selection into view.
        let newly_open = (!was_open && popover_open) || (response.clicked() && !popover_open);
        let mut pending_scroll = newly_open;
        ui.ctx().data_mut(|dd| {
            if newly_open {
                dd.insert_temp(scroll_key, true);
            }
        });
        if ui.ctx().data(|dd| dd.get_temp::<bool>(scroll_key).unwrap_or(false)) {
            pending_scroll = true;
        }

        let mut close_requested = false;
        let mut commit_index: Option<usize> = None;

        if has_focus {
            ui.input_mut(|i| {
                i.events.retain(|ev| match ev {
                    egui::Event::Key {
                        key: Key::ArrowDown,
                        pressed: true,
                        ..
                    } if !visible.is_empty() => {
                        highlighted = step_highlight(&visible, highlighted, 1);
                        pending_scroll = true;
                        false
                    },
                    egui::Event::Key {
                        key: Key::ArrowUp,
                        pressed: true,
                        ..
                    } if !visible.is_empty() => {
                        highlighted = step_highlight(&visible, highlighted, -1);
                        pending_scroll = true;
                        false
                    },
                    egui::Event::Key {
                        key: Key::Home,
                        pressed: true,
                        ..
                    } if !visible.is_empty() => {
                        highlighted = visible.first().copied();
                        pending_scroll = true;
                        false
                    },
                    egui::Event::Key {
                        key: Key::End,
                        pressed: true,
                        ..
                    } if !visible.is_empty() => {
                        highlighted = visible.last().copied();
                        pending_scroll = true;
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

        if keyboard_active && key_commit {
            commit_index = highlighted.and_then(|h| flat_options.get(h)).and_then(|o| o.index());
            close_requested = true;
        }

        if let Some(index) = commit_index {
            *selected = Some(index);
            ui.ctx().data_mut(|dd| dd.remove::<String>(filter_key));
        }
        if close_requested {
            Popover::close_by_id(ui.ctx(), popover_id);
            ui.ctx().data_mut(|dd| dd.remove::<String>(filter_key));
        }

        ui.ctx().data_mut(|dd| {
            dd.insert_temp(highlight_key, highlighted);
            if pending_scroll {
                dd.insert_temp(scroll_key, true);
            }
        });

        // ── Dropdown panel ───────────────────────────────────────────────
        let popover = Popover::new(popover_id).below().max_width(260.0);
        let _popover_resp = popover.show(ui, &response, |ui| {
            let mut filter: String =
                ui.ctx().data(|dd| dd.get_temp::<String>(filter_key).unwrap_or_default());
            let should_scroll =
                ui.ctx().data(|dd| dd.get_temp::<bool>(scroll_key).unwrap_or(false));

            ui.horizontal(|ui| {
                if searchable {
                    let tf = TextField::new(&mut filter)
                        .placeholder("Search…")
                        .desired_width(ui.available_width());
                    let _ = tf.show(ui);
                }
                if clearable
                    && ui
                        .add(Button::icon(egui_phosphor::regular::X).with_tooltip("Clear"))
                        .clicked()
                {
                    *selected = None;
                    ui.ctx().data_mut(|dd| dd.remove::<String>(filter_key));
                    Popover::close_by_id(ui.ctx(), popover_id);
                }
            });
            if searchable || clearable {
                ui.separator();
            }

            let visible_now = visible_positions(&flat_options, &filter);
            let mut scroll_target: Option<Rect> = None;

            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                for pos in visible_now {
                    match &flat_options[pos] {
                        FlatOption::Header(text) => {
                            ui.label(
                                egui::RichText::new(text).strong().color(t.palette.text.secondary),
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
                                &t,
                            );
                            if row_response.clicked() {
                                *selected = Some(*index);
                                ui.ctx().data_mut(|dd| {
                                    dd.remove::<String>(filter_key);
                                    dd.insert_temp(highlight_key, Some(pos));
                                });
                                Popover::close_by_id(ui.ctx(), popover_id);
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
            ui.ctx().data_mut(|dd| dd.insert_temp(scroll_key, false));

            ui.ctx().data_mut(|dd| {
                dd.insert_temp(filter_key, filter);
            });
        });

        // ── Trigger visuals ──────────────────────────────────────────────
        let radius = egui::CornerRadius::same(RADIUS_M as u8);
        let trigger_hovered = response.hovered();
        let slot = if !ui.is_enabled() {
            t.components.input.disabled
        } else if has_focus {
            t.components.input.focus
        } else if trigger_hovered {
            t.components.input.hover
        } else {
            t.components.input.normal
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
                egui::Stroke::new(STROKE_WIDTH, t.focus_ring()),
                egui::StrokeKind::Inside,
            );
        }

        let fg = if !ui.is_enabled() {
            t.components.input.disabled.fg
        } else if selected.is_some() {
            slot.fg
        } else {
            t.palette.text.muted
        };
        let caret_galley = ui.painter().layout_no_wrap(
            egui_phosphor::regular::CARET_DOWN.to_owned(),
            TextRole::Body.font_id(),
            fg,
        );
        let text_x = rect.min.x + s.space_3;
        let avail_text =
            (rect.width() - s.space_3 * 2.0 - caret_galley.size().x - s.space_2).max(0.0);
        let label_galley =
            ui.painter()
                .layout(current_label.clone(), TextRole::BodyS.font_id(), fg, avail_text);
        painter.galley(
            egui::pos2(text_x, rect.center().y - label_galley.size().y / 2.0),
            label_galley,
            fg,
        );
        painter.galley(
            egui::pos2(
                rect.max.x - s.space_3 - caret_galley.size().x,
                rect.center().y - caret_galley.size().y / 2.0,
            ),
            caret_galley,
            fg,
        );

        // Persist whether the popover is open for the next frame's open-transition.
        let still_open = Popover::is_open(ui.ctx(), popover_id);
        ui.ctx().data_mut(|dd| dd.insert_temp(was_open_key, still_open));

        let response = response.on_hover_cursor(egui::CursorIcon::Default);
        let accessible = current_label.clone();
        response.widget_info(|| {
            WidgetInfo::selected(
                WidgetType::ComboBox,
                ui.is_enabled(),
                selected.is_some(),
                accessible.clone(),
            )
        });
        response
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Positions (indices into `flat_options`) of rows visible under `filter`.
///
/// A header is included only when at least one following item matches, matching
/// the previous inline filtering behaviour.
fn visible_positions(flat_options: &[FlatOption], filter: &str) -> Vec<usize> {
    let mut pending_header: Option<usize> = None;
    let mut out: Vec<usize> = Vec::new();
    let filter_lower = filter.to_lowercase();

    for (i, opt) in flat_options.iter().enumerate() {
        match opt {
            FlatOption::Header(_) => pending_header = Some(i),
            FlatOption::Item { label, .. } => {
                let matches = filter.is_empty() || label.to_lowercase().contains(&filter_lower);
                if matches {
                    if let Some(h) = pending_header.take() {
                        out.push(h);
                    }
                    out.push(i);
                }
            },
        }
    }
    out
}

/// Move the highlighted position by `delta` within `visible` (wrapping-free,
/// clamped) and return the new position. `None` starts at the first row.
fn step_highlight(visible: &[usize], current: Option<usize>, delta: i32) -> Option<usize> {
    if visible.is_empty() {
        return None;
    }
    let Some(current) = current else {
        return visible.first().copied();
    };
    let Some(idx) = visible.iter().position(|p| *p == current) else {
        return visible.first().copied();
    };
    let next = if delta >= 0 {
        (idx + 1).min(visible.len() - 1)
    } else {
        idx.saturating_sub(1)
    };
    visible.get(next).copied()
}

/// Paint one option row using the `theme.components.list.*` slots.
fn paint_option_row(
    ui: &egui::Ui,
    rect: Rect,
    hovered: bool,
    selected: bool,
    highlighted: bool,
    label: &str,
    t: &crate::tokens::theme::Theme,
) {
    let s = crate::spatial(ui);
    let bg = if selected {
        t.components.list.selected.bg
    } else if highlighted {
        t.palette.accent.ghost
    } else if hovered {
        t.components.list.hover.bg
    } else {
        egui::Color32::TRANSPARENT
    };
    let painter = ui.painter_at(rect);
    if bg != egui::Color32::TRANSPARENT {
        painter.rect_filled(rect, egui::CornerRadius::same(2), bg);
    }
    let fg = if selected {
        t.components.list.selected.fg
    } else {
        t.palette.text.primary
    };
    painter.text(
        egui::pos2(rect.min.x + s.space_3, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        TextRole::BodyS.font_id(),
        fg,
    );
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
        let groups: &[(&str, &[&str])] = &[("Fruits", &["Apple", "Banana"]), ("Veg", &["Carrot"])];
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

    // ── Filtering ───────────────────────────────────────────────────────

    fn flat(labels: &[&str]) -> Vec<FlatOption> {
        labels
            .iter()
            .enumerate()
            .map(|(i, l)| FlatOption::Item {
                label: l.to_string(),
                index: i,
            })
            .collect()
    }

    #[test]
    fn visible_positions_empty_filter_keeps_all() {
        let opts = flat(&["A", "B", "C"]);
        assert_eq!(visible_positions(&opts, ""), vec![0, 1, 2]);
    }

    #[test]
    fn visible_positions_suppresses_empty_headers() {
        let groups: &[(&str, &[&str])] = &[("Fruits", &["Apple", "Banana"]), ("Veg", &["Carrot"])];
        let mut sel = None;
        let s = Select::grouped("g", &mut sel, groups);
        // "Car" only matches the Carrot item → the Fruits header is dropped but
        // the Veg header stays.
        let vis = visible_positions(&s.flat_options, "car");
        assert_eq!(vis, vec![3, 4]);
    }

    #[test]
    fn visible_positions_case_insensitive_substring() {
        let opts = flat(&["Alpha", "Beta", "Gamma", "Delta"]);
        // "ta" is in Be*ta* and Del*ta*; matching is case-insensitive.
        assert_eq!(visible_positions(&opts, "TA"), vec![1, 3]);
    }

    // ── Highlight navigation ────────────────────────────────────────────

    #[test]
    fn step_highlight_starts_at_first() {
        assert_eq!(step_highlight(&[0, 1, 2], None, 1), Some(0));
        assert_eq!(step_highlight(&[0, 1, 2], None, -1), Some(0));
    }

    #[test]
    fn step_highlight_moves_and_clamps() {
        assert_eq!(step_highlight(&[0, 1, 2], Some(0), 1), Some(1));
        assert_eq!(step_highlight(&[0, 1, 2], Some(2), 1), Some(2));
        assert_eq!(step_highlight(&[0, 1, 2], Some(0), -1), Some(0));
        assert_eq!(step_highlight(&[0, 1, 2], Some(1), -1), Some(0));
    }

    #[test]
    fn step_highlight_skips_filtered_rows() {
        // Only positions 1 and 3 are visible.
        assert_eq!(step_highlight(&[1, 3], Some(1), 1), Some(3));
        assert_eq!(step_highlight(&[1, 3], Some(3), -1), Some(1));
    }

    #[test]
    fn step_highlight_empty_is_none() {
        assert_eq!(step_highlight(&[], None, 1), None);
    }

    #[test]
    fn step_highlight_stale_current_resets_to_first() {
        assert_eq!(step_highlight(&[2, 5], Some(0), 1), Some(2));
    }

    // ── Trigger rect / layout ───────────────────────────────────────────

    /// The trigger must fill the rect supplied by `ui.add_sized`.
    #[test]
    fn trigger_fills_add_sized_rect() {
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 120.0));
        let mut sel = None;
        let mut rect = None;
        let _ = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| {
                let resp = ui.add_sized(
                    egui::vec2(180.0, 22.0),
                    Select::new("sized_select", &mut sel, &["A", "B"]),
                );
                rect = Some(resp.rect);
            },
        );
        let rect = rect.expect("trigger response");
        assert!(
            (rect.width() - 180.0).abs() < 1.0,
            "trigger width should match add_sized: {rect:?}"
        );
        assert!(
            (rect.height() - 22.0).abs() < 1.0,
            "trigger height should match add_sized: {rect:?}"
        );
    }

    // ── Keyboard navigation ─────────────────────────────────────────────

    fn base_input() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 240.0),
            )),
            ..Default::default()
        }
    }

    fn key_event(key: Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }
    }

    fn run_select(
        ctx: &egui::Context,
        input: egui::RawInput,
        id: &'static str,
        selected: &mut Option<usize>,
    ) {
        let _ = ctx.run_ui(input, |ui| {
            ui.add(Select::new(id, selected, &["A", "B", "C"]));
        });
    }

    /// ArrowDown must move the keyboard highlight and consume the event while
    /// the trigger is focused and the dropdown is open.
    #[test]
    fn arrow_down_moves_highlight_and_consumes_event() {
        let ctx = egui::Context::default();
        let select_id = egui::Id::new("kb_select");
        let trigger_id = select_id.with("__trigger");
        let highlight_key = select_id.with("__highlight");
        let popover_id = select_id.with("__popover");

        Popover::new(popover_id).open(&ctx, true);
        ctx.memory_mut(|m| m.request_focus(trigger_id));
        ctx.data_mut(|d| d.insert_temp(highlight_key, Some(0usize)));

        let mut selected: Option<usize> = None;
        let mut remaining = Vec::new();
        let mut input = base_input();
        input.events.push(key_event(Key::ArrowDown));
        let _ = ctx.run_ui(input, |ui| {
            ui.add(Select::new("kb_select", &mut selected, &["A", "B", "C"]));
            remaining = ui.input(|i| i.events.clone());
        });

        let highlight: Option<usize> = ctx.data(|d| d.get_temp(highlight_key)).unwrap_or(None);
        assert_eq!(highlight, Some(1), "ArrowDown should advance the highlight");
        assert!(
            !remaining.iter().any(|e| matches!(
                e,
                egui::Event::Key {
                    key: Key::ArrowDown,
                    ..
                }
            )),
            "focused open Select must consume the arrow key"
        );
        assert_eq!(selected, None, "arrow navigation must not commit a selection");
    }

    /// Enter commits the highlighted option and closes the popover.
    #[test]
    fn enter_commits_highlight_and_closes() {
        let ctx = egui::Context::default();
        let select_id = egui::Id::new("commit_select");
        let trigger_id = select_id.with("__trigger");
        let highlight_key = select_id.with("__highlight");
        let popover_id = select_id.with("__popover");

        Popover::new(popover_id).open(&ctx, true);
        ctx.memory_mut(|m| m.request_focus(trigger_id));
        ctx.data_mut(|d| d.insert_temp(highlight_key, Some(1usize)));

        let mut selected: Option<usize> = None;
        let mut input = base_input();
        input.events.push(key_event(Key::Enter));
        run_select(&ctx, input, "commit_select", &mut selected);

        assert_eq!(selected, Some(1), "Enter should commit the highlighted option");
        assert!(
            !Popover::is_open(&ctx, popover_id),
            "Enter must close the dropdown after committing"
        );
    }

    /// Escape closes the open dropdown without changing the selection.
    ///
    /// A warm-up frame is required because egui surrenders focus on Escape
    /// *before* widget code runs unless the focused widget has already
    /// registered an `escape: true` lock filter (which happens the first frame
    /// the trigger holds focus, i.e. the frame the user clicked it).
    #[test]
    fn escape_closes_without_commit() {
        let ctx = egui::Context::default();
        let select_id = egui::Id::new("esc_select");
        let trigger_id = select_id.with("__trigger");
        let popover_id = select_id.with("__popover");

        Popover::new(popover_id).open(&ctx, true);
        ctx.memory_mut(|m| m.request_focus(trigger_id));

        let mut selected: Option<usize> = None;
        // Warm-up frame registers the escape focus lock filter.
        run_select(&ctx, base_input(), "esc_select", &mut selected);
        assert!(
            ctx.memory(|m| m.has_focus(trigger_id)),
            "trigger should still hold focus after the warm-up frame"
        );

        let mut input = base_input();
        input.events.push(key_event(Key::Escape));
        run_select(&ctx, input, "esc_select", &mut selected);

        assert_eq!(selected, None, "Escape must not commit a selection");
        assert!(!Popover::is_open(&ctx, popover_id), "Escape must close the dropdown");
    }

    /// Keyboard navigation is inert while the trigger is not focused.
    #[test]
    fn arrow_ignored_when_trigger_unfocused() {
        let ctx = egui::Context::default();
        let select_id = egui::Id::new("unfocused_select");
        let highlight_key = select_id.with("__highlight");

        let mut selected: Option<usize> = None;
        let mut remaining = Vec::new();
        let mut input = base_input();
        input.events.push(key_event(Key::ArrowDown));
        let _ = ctx.run_ui(input, |ui| {
            ui.add(Select::new("unfocused_select", &mut selected, &["A", "B", "C"]));
            remaining = ui.input(|i| i.events.clone());
        });

        assert!(
            ctx.data(|d| d.get_temp::<Option<usize>>(highlight_key))
                .unwrap_or(None)
                .is_none(),
            "unfocused Select must not set a keyboard highlight"
        );
        assert!(
            remaining.iter().any(|e| matches!(
                e,
                egui::Event::Key {
                    key: Key::ArrowDown,
                    ..
                }
            )),
            "unfocused Select must not consume the arrow key"
        );
    }

    /// Unconstrained, the trigger is one row tall (not the whole panel height).
    #[test]
    fn trigger_default_height_is_one_row() {
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 400.0));
        let mut sel = None;
        let mut rect = None;
        let _ = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| {
                rect = Some(ui.add(Select::new("plain_select", &mut sel, &["A", "B"])).rect);
            },
        );
        let rect = rect.expect("trigger response");
        assert!(
            rect.height() <= 40.0,
            "unconstrained trigger should be roughly one row tall: {rect:?}"
        );
    }
}
