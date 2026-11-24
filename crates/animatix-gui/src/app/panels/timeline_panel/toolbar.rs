//! One compact timeline toolbar, with zoom pinned on the trailing edge.
use super::narration::{ReviewTrack, ScriptReview};
use super::review;
use super::ruler::{render_timeline_zoom_bar, zoom_controls_width};
use crate::app::PreviewPaneState;
use crate::app::commands::ActionQueue;
use crate::app::design_tokens::spatial::Spatial;

pub(super) fn review_controls(
    ui: &mut egui::Ui,
    preview: &mut PreviewPaneState,
    commands: &mut ActionQueue,
    panel_id: egui::Id,
    sp: &Spatial,
    script: ScriptReview<'_>,
) -> (bool, ReviewTrack) {
    let theme = eparts::theme(ui);
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
            let gap = ui.spacing().item_spacing.x;
            let left_width = available - zoom_width - gap;
            let row_height = size.row_height_for(eparts::density(ui));
            let row_size = egui::vec2(available, row_height);
            let zoom_layout = egui::Layout::right_to_left(egui::Align::Center);
            // At genuinely constrained widths, move paragraph tools as a group;
            // keep marker tools and zoom on the same row whenever both fit.
            let inline_script = left_width >= 380.0;
            let fits_both = left_width >= 6.0 * row_height + 5.0 * gap;
            let (show_debug, track) = if fits_both {
                ui.allocate_ui_with_layout(row_size, zoom_layout, |ui| {
                    // Allocate from the outside in, so the remainder given to
                    // paragraph controls cannot take the trailing zoom region.
                    render_timeline_zoom_bar(ui, preview, size);
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let show_debug = review::toolbar(
                            ui,
                            preview,
                            script.markers,
                            commands,
                            panel_id,
                            size,
                            script.current,
                        );
                        let track = inline_script.then(|| script.show(ui, commands, panel_id));
                        (show_debug, track)
                    })
                    .inner
                })
                .inner
            } else {
                let show_debug = review::toolbar(
                    ui,
                    preview,
                    script.markers,
                    commands,
                    panel_id,
                    size,
                    script.current,
                );
                ui.allocate_ui_with_layout(row_size, zoom_layout, |ui| {
                    render_timeline_zoom_bar(ui, preview, size);
                });
                (show_debug, None)
            };
            (show_debug, track.unwrap_or_else(|| script.show(ui, commands, panel_id)))
        })
        .inner
}

#[cfg(test)]
mod toolbar_tests {
    use super::*;
    use egui::{Pos2, RawInput, Rect, Shape, pos2};

    #[test]
    fn zoom_stays_at_right_on_same_row_with_or_without_paragraphs() {
        for (width, source) in [
            (1080.0, ""),
            (1080.0, "/// A paragraph"),
            (260.0, "/// A paragraph"),
        ] {
            let ctx = egui::Context::default();
            let mut preview = PreviewPaneState::new(
                5.0,
                crate::app::SceneDimensions {
                    width: 640,
                    height: 360,
                },
            );
            let mut commands = ActionQueue::default();
            let mut render = |ui: &mut egui::Ui| {
                let sp = crate::app::design_tokens::spatial::spatial(ui);
                review_controls(
                    ui,
                    &mut preview,
                    &mut commands,
                    egui::Id::unique("toolbar"),
                    &sp,
                    ScriptReview {
                        history: &Default::default(),
                        source,
                        anchor_changes: &[],
                        current: true,
                        epoch: crate::app::document::version::SourceEpoch(1),
                        revision: 0,
                        scene: None,
                        offset: 0.0,
                        timeline: None,
                        markers: &[],
                    },
                );
                let ruler_rect = ui.available_rect_before_wrap();
                let theme = eparts::theme(ui);
                let coords = super::super::TimelineCoords {
                    bar_origin_x: ruler_rect.left() + 80.0,
                    bar_width: ruler_rect.width() - 80.0,
                    scroll_s: 0.0,
                    visible_s: 5.0,
                };
                super::super::ruler::render_ruler(
                    ui.painter(),
                    ruler_rect,
                    ruler_rect.top(),
                    ruler_rect.top() + sp.timeline.ruler_height,
                    &coords,
                    theme,
                    &sp,
                );
            };
            let mut positions = std::collections::HashMap::new();
            let input = || RawInput {
                screen_rect: Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(width, 260.0))),
                ..Default::default()
            };
            ctx.run_ui(input(), &mut render).textures_delta.clear();
            let mut output = ctx.run_ui(input(), &mut render);
            output.textures_delta.clear();
            for shape in &output.shapes {
                gather_text(&shape.shape, &mut positions);
            }
            let marker = positions[egui_phosphor::regular::DIAMOND];
            let zoom = positions["100%"];
            assert!(zoom.x > width - 100.0, "{width}: {positions:?}");
            assert!((zoom.y - marker.y).abs() < 3.0, "{width}: {positions:?}");
            let ruler = positions["Tracks"];
            assert!(zoom.y < 35.0 && ruler.y > zoom.y && ruler.y < 100.0, "{width}: {positions:?}");
            if !source.is_empty() {
                let paragraph = positions["All paragraphs"];
                if width > 500.0 {
                    assert!((zoom.y - paragraph.y).abs() < 3.0, "{positions:?}");
                } else {
                    assert!(paragraph.y > zoom.y + 12.0, "{positions:?}");
                }
            }
        }
    }

    fn gather_text(shape: &Shape, out: &mut std::collections::HashMap<String, Pos2>) {
        match shape {
            Shape::Text(text) => {
                out.insert(text.galley.text().to_owned(), text.visual_bounding_rect().center());
            },
            Shape::Vec(shapes) => {
                for child in shapes {
                    gather_text(child, out);
                }
            },
            _ => {},
        }
    }
}
