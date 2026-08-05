use egui::{Id, Rect, Response, Sense, Vec2};

use crate::spatial;
use crate::tokens::spatial::{RADIUS_M, STROKE_WIDTH};
use crate::tokens::typography::TextRole;

/// A stateful pill-style tab bar.
///
/// Prefer this over the standalone [`crate::widget::layout::pill_tab_bar`]
/// free function for new code; `pill_tab_bar` remains for backward compatibility.
///
/// The bar renders tabs with `theme.tab.active` / `theme.tab.inactive` /
/// `theme.tab.hover` slots and an active indicator stripe using
/// `theme.tab.active.indicator`. Selection is committed on click.
///
/// `TabBar` owns no cross-frame state; the caller stores the selected index
/// (typically in an app struct) and passes a mutable reference.
///
/// # Example
/// ```ignore
/// let mut selected = 0;
/// ui.add(TabBar::new("my_tabs", &mut selected, &["Files", "Edit", "View"]));
/// ```
pub struct TabBar<'a> {
    id: Id,
    selected_index: &'a mut usize,
    tabs: Vec<&'a str>,
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
    pub fn new(id: impl Into<Id>, selected_index: &'a mut usize, tabs: &[&'a str]) -> Self {
        Self {
            id: id.into(),
            selected_index,
            tabs: tabs.to_vec(),
            height: crate::tokens::spatial::component::PILL_TAB_HEIGHT,
            gap: crate::tokens::spatial::component::PILL_TAB_GAP,
            sense: Sense::click(),
        }
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

    /// Override the interaction sense (defaults to `Sense::click()`).
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
        let tab_w = if self.tabs.is_empty() {
            0.0
        } else {
            (available - gap * (self.tabs.len().saturating_sub(1)) as f32) / self.tabs.len() as f32
        };

        let (bar_rect, bar_response) =
            ui.allocate_exact_size(Vec2::new(available, tab_h), self.sense);

        // Bar background — use the inactive slot color as the base.
        ui.painter().rect_filled(bar_rect, RADIUS_M, t.tab.inactive.bg);

        let mut clicked_index = None;

        for (idx, label) in self.tabs.iter().enumerate() {
            let is_active = *self.selected_index == idx;
            let x = bar_rect.min.x + idx as f32 * (tab_w + gap);
            let tab_rect =
                Rect::from_min_size(egui::pos2(x, bar_rect.min.y), Vec2::new(tab_w, tab_h));

            let response = ui.interact(tab_rect, self.id.with(("tab", idx)), Sense::click());

            let slot = if is_active {
                &t.tab.active
            } else if response.hovered() {
                &t.tab.hover
            } else {
                &t.tab.inactive
            };

            // Draw pill background for the tab.
            let pill = tab_rect.shrink2(Vec2::new(2.0, 2.0));
            ui.painter().rect_filled(pill, RADIUS_M, slot.bg);

            if is_active {
                ui.painter().rect_stroke(
                    pill,
                    RADIUS_M,
                    egui::Stroke::new(STROKE_WIDTH, t.border.strong),
                    egui::StrokeKind::Inside,
                );

                // Active indicator stripe at the bottom of the pill.
                let indicator_h = 3.0;
                let indicator_rect = Rect::from_min_size(
                    egui::pos2(pill.min.x, pill.max.y - indicator_h),
                    Vec2::new(pill.width(), indicator_h),
                );
                ui.painter().rect_filled(indicator_rect, 0.0, slot.indicator);
            }

            ui.painter().text(
                tab_rect.center(),
                egui::Align2::CENTER_CENTER,
                *label,
                TextRole::BodyS.font_id(),
                slot.fg,
            );

            if response.clicked() {
                clicked_index = Some(idx);
            }
        }

        if let Some(idx) = clicked_index {
            *self.selected_index = idx;
        }

        bar_response.on_hover_cursor(egui::CursorIcon::Default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::traits::Sizable;

    #[test]
    fn builder_sets_height_and_gap() {
        let mut idx = 0;
        let bar = TabBar::new("test", &mut idx, &["A", "B"]).height(48.0).gap(8.0);
        assert_eq!(bar.height, 48.0);
        assert_eq!(bar.gap, 8.0);
    }

    #[test]
    fn builder_sets_sense() {
        let mut idx = 0;
        let bar = TabBar::new("test", &mut idx, &["A"]).sense(Sense::drag());
        assert_eq!(bar.sense, Sense::drag());
    }

    #[test]
    fn size_trait_sets_height() {
        let mut idx = 0;
        let bar = TabBar::new("test", &mut idx, &["A", "B"]).with_size(crate::widget::Size::Lg);
        assert_eq!(bar.height, crate::widget::Size::Lg.row_height());
    }
}
