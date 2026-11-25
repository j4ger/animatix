//! Timeline heading and scene-wide timing controls.

use super::markers::TimingMarker;
use super::review;
use super::ruler::{render_timeline_zoom_bar, zoom_controls_width};
use crate::app::PreviewPaneState;
use crate::app::commands::ActionQueue;
use crate::app::design_tokens::spatial::spatial;
use crate::app::design_tokens::typography::TextRole;

pub(super) fn review_controls(
    ui: &mut egui::Ui,
    preview: &mut PreviewPaneState,
    commands: &mut ActionQueue,
    id: egui::Id,
    markers: &[TimingMarker],
    source_current: bool,
) -> bool {
    let theme = eparts::theme(ui);
    let sp = spatial(ui);
    egui::Frame::new()
        .fill(theme.palette.surface.panel)
        .inner_margin(egui::Margin::symmetric(sp.base.space_4 as i8, sp.base.space_2 as i8))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = sp.base.space_2;
            let available = ui.available_width();
            let size = if available < 420.0 {
                eparts::Size::Sm
            } else {
                eparts::Size::Md
            };
            let zoom_width = zoom_controls_width(ui, size);
            let row_height = size.row_height_for(eparts::density(ui));
            let title_label = crate::app::i18n::text(ui, "gui-panel-timeline");
            let title = egui::RichText::new(&title_label).font(TextRole::BodyMedium.font_id());
            let title_width = ui
                .painter()
                .layout_no_wrap(
                    title_label,
                    TextRole::BodyMedium.font_id(),
                    theme.palette.text.primary,
                )
                .size()
                .x;
            let row = egui::vec2(available, row_height);
            let right_to_left = egui::Layout::right_to_left(egui::Align::Center);
            let left_to_right = egui::Layout::left_to_right(egui::Align::Center);
            let inline =
                available >= zoom_width + title_width + 7.0 * row_height + 10.0 * sp.base.space_2;
            let debug = ui
                .allocate_ui_with_layout(row, right_to_left, |ui| {
                    render_timeline_zoom_bar(ui, preview, size);
                    ui.with_layout(left_to_right, |ui| {
                        ui.label(title);
                        inline.then(|| {
                            ui.separator();
                            review::toolbar(
                                ui,
                                preview,
                                markers,
                                commands,
                                id,
                                size,
                                source_current,
                            )
                        })
                    })
                    .inner
                })
                .inner;
            debug.unwrap_or_else(|| {
                review::toolbar(ui, preview, markers, commands, id, size, source_current)
            })
        })
        .inner
}
