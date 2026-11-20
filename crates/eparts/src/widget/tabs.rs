use egui::{Id, Rect, Response, Sense, Vec2};

use crate::spatial;
use crate::tokens::spatial::{RADIUS_L, RADIUS_M, STROKE_WIDTH};
use crate::tokens::typography::TextRole;
use crate::widget::IntoWidgetId;
use crate::widget::traits::finish_response;

/// A stateful pill-style tab bar.
///
/// Prefer this over the standalone [`crate::widget::layout::pill_tab_bar`]
/// free function for new code; `pill_tab_bar` remains for backward compatibility.
///
/// The selected segment sits above a recessed track. Use `underlined` for a quieter
/// secondary navigation row. Labels remain accessible when only icons fit.
///
/// `TabBar` owns no cross-frame state; the caller stores the selected index
/// (typically in an app struct) and passes a mutable reference.
///
/// # Example
/// ```ignore
/// let mut selected = 0;
/// TabBar::new("my_tabs", &mut selected, &["Files", "Edit", "View"]).show(ui);
/// ```
pub struct TabBar<'a> {
    id: Id,
    selected_index: &'a mut usize,
    tabs: Vec<&'a str>,
    icons: &'a [&'a str],
    tooltips: &'a [&'a str],
    underlined: bool,
    height: f32,
    gap: f32,
    sense: Sense,
}

impl<'a> TabBar<'a> {
    /// Create a new tab bar.
    ///
    /// * `id_source` — unique id source for the bar (egui `Id`).
    /// * `selected_index` — mutable reference to the currently selected tab index.
    /// * `tabs` — tab labels. The bar is empty if `tabs` is empty.
    pub fn new(id: impl IntoWidgetId, selected_index: &'a mut usize, tabs: &[&'a str]) -> Self {
        Self {
            id: id.into_widget_id(),
            selected_index,
            tabs: tabs.to_vec(),
            icons: &[],
            tooltips: &[],
            underlined: false,
            height: crate::tokens::spatial::component::PILL_TAB_HEIGHT,
            gap: crate::tokens::spatial::component::PILL_TAB_GAP,
            sense: Sense::hover(),
        }
    }

    /// Supply optional leading icons in tab order.
    pub fn with_icons(mut self, icons: &'a [&'a str]) -> Self {
        self.icons = icons;
        self
    }

    /// Supply descriptive tooltips in tab order. By default each tab uses its label.
    pub fn with_tooltips(mut self, tooltips: &'a [&'a str]) -> Self {
        self.tooltips = tooltips;
        self
    }

    /// Draw a flat secondary navigation row with an underline for selection.
    pub fn underlined(mut self) -> Self {
        self.underlined = true;
        self
    }

    /// Override the bar height (px).
    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    /// Override the gap between tabs (px).
    pub fn gap(mut self, g: f32) -> Self {
        self.gap = g;
        self
    }

    /// Override the track interaction sense. Each tab always accepts clicks.
    pub fn sense(mut self, sense: Sense) -> Self {
        self.sense = sense;
        self
    }
}

impl<'a> crate::widget::Sizable for TabBar<'a> {
    fn with_size(mut self, size: crate::widget::Size) -> Self {
        self.height = size.row_height();
        self
    }
}

impl<'a> TabBar<'a> {
    pub fn show(self, ui: &mut egui::Ui) -> Response {
        let t = crate::theme(ui);
        let s = spatial(ui);
        let available = ui.available_width();
        // Density-aware defaults: override base consts only when not explicitly set
        let tab_h = if self.height == crate::tokens::spatial::component::PILL_TAB_HEIGHT {
            s.component.pill_tab_height
        } else {
            self.height
        };
        let gap = if self.gap == crate::tokens::spatial::component::PILL_TAB_GAP {
            s.component.pill_tab_gap
        } else {
            self.gap
        };
        let (bar_rect, mut bar_response) =
            ui.allocate_exact_size(Vec2::new(available, tab_h), self.sense);
        if self.tabs.is_empty() || !ui.is_rect_visible(bar_rect) {
            return bar_response;
        }
        let inset = s.space_1;
        if !self.underlined {
            ui.painter().rect(
                bar_rect,
                RADIUS_L,
                t.components.tab.inactive.bg,
                egui::Stroke::new(STROKE_WIDTH, t.palette.border.subtle),
                egui::StrokeKind::Inside,
            );
        }
        let inner = bar_rect.shrink2(egui::vec2(inset, 0.0));
        let tab_w = ((inner.width() - gap * (self.tabs.len() - 1) as f32) / self.tabs.len() as f32)
            .max(0.0);
        for (idx, &label) in self.tabs.iter().enumerate() {
            let x = inner.left() + idx as f32 * (tab_w + gap);
            let rect = Rect::from_min_size(egui::pos2(x, inner.top()), Vec2::new(tab_w, tab_h));
            let response = ui.interact(rect, self.id.with(("tab", idx)), Sense::click());
            let active = *self.selected_index == idx;
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::Tab, ui.is_enabled(), active, label)
            });
            let slots = &t.components.tab;
            let slot = if active {
                slots.active
            } else if response.hovered() {
                slots.hover
            } else {
                slots.inactive
            };
            let face = rect.shrink2(egui::vec2(0.0, inset * 1.5));
            if active && !self.underlined {
                ui.painter().add(t.elevation_raised().as_shape(face, RADIUS_M));
                ui.painter().rect(
                    face,
                    RADIUS_M,
                    slot.bg,
                    egui::Stroke::new(STROKE_WIDTH, t.palette.border.subtle),
                    egui::StrokeKind::Inside,
                );
            } else if response.hovered() {
                ui.painter().rect_filled(face, RADIUS_M, slots.hover.bg);
            }
            if active && self.underlined {
                let mark = Rect::from_center_size(
                    egui::pos2(face.center().x, face.bottom()),
                    egui::vec2(face.width().min(20.0), 2.0),
                );
                ui.painter().rect_filled(mark, 1.0, slot.indicator);
            }
            if response.has_focus() {
                ui.painter().rect_stroke(
                    face,
                    RADIUS_M,
                    egui::Stroke::new(STROKE_WIDTH, t.focus_ring()),
                    egui::StrokeKind::Inside,
                );
            }
            let icon = self.icons.get(idx).copied().unwrap_or("");
            let painter = ui.painter().with_clip_rect(face.intersect(ui.clip_rect()));
            let text = painter.layout_no_wrap(label.to_owned(), TextRole::BodyS.font_id(), slot.fg);
            let icon_space = s.component.icon_slot_width + s.space_2;
            let width = (face.width() - s.space_2).max(0.0);
            let both_fit = !icon.is_empty() && text.size().x + icon_space <= width;
            let show_text = both_fit || icon.is_empty() || text.size().x <= width;
            let mut center = face.center();
            if both_fit || !show_text {
                let icon_x = if both_fit {
                    center.x - (text.size().x + s.space_2) * 0.5
                } else {
                    center.x
                };
                painter.text(
                    egui::pos2(icon_x, center.y),
                    egui::Align2::CENTER_CENTER,
                    icon,
                    TextRole::Body.font_id(),
                    if active { slot.indicator } else { slot.fg },
                );
                center.x += icon_space * 0.5;
            }
            if show_text {
                let pos = egui::Align2::CENTER_CENTER.anchor_size(center, text.size()).min;
                painter.galley(pos, text, slot.fg);
            }
            if response.clicked() {
                bar_response = bar_response.union(response.clone());
                if !active {
                    *self.selected_index = idx;
                    bar_response.mark_changed();
                }
            }
            finish_response(ui, response, self.tooltips.get(idx).copied().unwrap_or(label));
        }
        finish_response(ui, bar_response, "")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::traits::Sizable;

    #[test]
    fn builder_fields() {
        let mut idx = 0;
        let bar = TabBar::new("test", &mut idx, &["A", "B"]).height(48.0).gap(8.0);
        assert_eq!(bar.height, 48.0);
        assert_eq!(bar.gap, 8.0);

        let bar = TabBar::new("test", &mut idx, &["A"]).sense(Sense::drag());
        assert_eq!(bar.sense, Sense::drag());
        let bar = TabBar::new("test", &mut idx, &["A"])
            .with_icons(&["a"])
            .with_tooltips(&["Action"])
            .underlined();
        assert_eq!(bar.icons, &["a"]);
        assert_eq!(bar.tooltips, &["Action"]);
        assert!(bar.underlined);
    }

    #[test]
    fn activating_a_tab_reports_clicks_and_only_changes_a_new_selection() {
        let ctx = egui::Context::default();
        let mut selected = 0;
        let bar = Id::unique("mode");
        let mut frame = |events: Vec<egui::Event>| {
            let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(240.0, 80.0));
            let input = egui::RawInput {
                screen_rect: Some(rect),
                events,
                ..Default::default()
            };
            let mut result = None;
            let _ = crate::run_ui(&ctx, input, |ui| {
                let response = TabBar::new(bar, &mut selected, &["One", "Two"]).show(ui);
                result = Some((response.clicked(), response.changed(), selected));
            });
            result.unwrap()
        };
        assert_eq!(frame(vec![]), (false, false, 0));
        let pos = egui::pos2(175.0, 14.0);
        let click = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        for changed in [true, false] {
            frame(vec![egui::Event::PointerMoved(pos), click(true)]);
            assert_eq!(frame(vec![click(false)]), (true, changed, 1));
        }
        ctx.memory_mut(|m| m.request_focus(bar.with(("tab", 0usize))));
        let enter = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        assert_eq!(frame(vec![enter]), (true, true, 0));
    }

    #[test]
    fn size_trait_sets_height() {
        let mut idx = 0;
        let bar = TabBar::new("test", &mut idx, &["A", "B"]).with_size(crate::widget::Size::Lg);
        assert_eq!(bar.height, crate::widget::Size::Lg.row_height());
    }
}
