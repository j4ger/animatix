//! Timeline review controls mutate only playback/workbench state.
use super::{
    TimelineCoords,
    markers::{self, TimingMarker},
};
use crate::app::PreviewPaneState;
use crate::app::commands::{ActionQueue, Command};
use animatix::composition::Composition;
use egui::{Align2, FontId, Pos2, Rect, Sense, Stroke};

pub(crate) fn toolbar(
    ui: &mut egui::Ui,
    preview: &mut PreviewPaneState,
    markers: &[TimingMarker],
    commands: &mut ActionQueue,
    id: egui::Id,
) -> bool {
    let debug_id = id.with("review_debug_tracks");
    let mut debug = ui.data(|d| d.get_temp::<bool>(debug_id)).unwrap_or(false);
    ui.horizontal_wrapped(|ui| {
        ui.strong("Timing review");
        ui.weak("Global seconds · read-only");
        markers::marker_menu(ui, markers, commands);
        let current = preview.playback.current_time_s();
        for (label, forward) in [("‹ Marker", false), ("Marker ›", true)] {
            let target = adjacent_marker(markers, current, forward);
            if ui.add_enabled(target.is_some(), egui::Button::new(label)).clicked()
                && let Some(marker) = target
            {
                markers::navigate(marker, commands);
            }
        }
        if ui
            .button("Loop visible")
            .on_hover_text("Preview-only loop over the visible timeline interval")
            .clicked()
        {
            let duration = preview.playback.duration_s;
            let start = preview.timeline_scroll_offset.clamp(0.0, duration);
            let end = (start + duration / preview.timeline_zoom.max(0.25) as f64).min(duration);
            if end > start {
                preview.playback.loop_start_s = Some(start);
                preview.playback.loop_end_s = Some(end);
            }
        }
        if ui.button("Clear loop").clicked() {
            preview.playback.loop_start_s = None;
            preview.playback.loop_end_s = None;
        }
        ui.checkbox(&mut debug, "Debug tracks")
            .on_hover_text("Read-only evaluated property tracks, not authored timing");
    });
    ui.data_mut(|d| d.insert_temp(debug_id, debug));
    debug
}

fn adjacent_marker(markers: &[TimingMarker], time: f64, forward: bool) -> Option<&TimingMarker> {
    markers
        .iter()
        .filter(|m| {
            m.time_s.is_some_and(|t| {
                if forward {
                    t > time + 0.0001
                } else {
                    t < time - 0.0001
                }
            })
        })
        .min_by(|a, b| {
            let order = a.time_s.unwrap().total_cmp(&b.time_s.unwrap());
            if forward { order } else { order.reverse() }
        })
}

pub(crate) fn scene_track(
    ui: &mut egui::Ui,
    composition: &Composition,
    rect: Rect,
    top: f32,
    bottom: f32,
    coords: &TimelineCoords,
    commands: &mut ActionQueue,
) {
    let theme = eparts::theme(ui);
    let painter = ui.painter().clone();
    painter.text(
        Pos2::new(rect.left() + 8.0, (top + bottom) * 0.5),
        Align2::LEFT_CENTER,
        "Scenes",
        FontId::proportional(11.0),
        theme.palette.text.secondary,
    );
    let bar =
        Rect::from_min_max(Pos2::new(coords.bar_origin_x, top), Pos2::new(rect.right(), bottom));
    let painter = painter.with_clip_rect(bar.intersect(ui.clip_rect()));
    for name in &composition.declaration_order {
        let Some(scene) = composition.scenes.get(name) else {
            continue;
        };
        let Some(start) = composition.scene_start_times.get(name).copied() else {
            continue;
        };
        let end = start + scene.duration_s;
        if end < coords.scroll_s || start > coords.scroll_s + coords.visible_s {
            continue;
        }
        let block = Rect::from_min_max(
            Pos2::new(coords.time_to_x(start), top + 2.0),
            Pos2::new(coords.time_to_x(end), bottom - 2.0),
        );
        painter.rect_filled(block, 3.0, theme.palette.surface.widget);
        painter.rect_stroke(
            block,
            3.0,
            Stroke::new(1.0, theme.palette.border.default),
            egui::StrokeKind::Inside,
        );
        let visible = block.intersect(bar);
        if visible.width() > 35.0 {
            painter.text(
                visible.center(),
                Align2::CENTER_CENTER,
                name,
                FontId::proportional(11.0),
                theme.palette.text.primary,
            );
        }
        let response =
            ui.interact(visible, ui.scope_id().with(("review_scene", name)), Sense::click());
        if response.clicked() {
            commands.push_back(Command::SelectScene(name.clone()).into());
            commands.push_back(Command::ScrubTo(start).into());
        }
        response.on_hover_text(format!("{name}\n{start:.3}s – {end:.3}s global\nClick to review. Scene order is defined in source."));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_orders_by_time_not_source_order() {
        let markers = [4.0, 1.0, 3.0].map(|time| TimingMarker {
            expression: String::new(),
            line: None,
            scene: None,
            time_s: Some(time),
            action_count: 0,
        });
        assert_eq!(adjacent_marker(&markers, 2.0, true).unwrap().time_s, Some(3.0));
        assert_eq!(adjacent_marker(&markers, 2.0, false).unwrap().time_s, Some(1.0));
        assert!(adjacent_marker(&markers, 4.0, true).is_none());
    }
}
