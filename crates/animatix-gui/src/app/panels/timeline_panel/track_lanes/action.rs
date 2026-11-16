use super::*;
use crate::app::components::text_tooltip;

pub(crate) fn render_action_blocks(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    actor_label: &str,
    track_idx: usize,
    bar_area: Rect,
    coords: &TimelineCoords,
    theme: eparts::Theme,
) {
    let painter = ui.painter();
    for event in &timeline.action_events {
        if !event.targets.iter().any(|target| target == actor_label) {
            continue;
        }
        let start_s = event.start_time_ms as f64 / 1000.0;
        let end_s = (event.start_time_ms + event.duration_ms) as f64 / 1000.0;
        let left = coords.time_to_x(start_s);
        let right = coords.time_to_x(end_s);
        if right <= bar_area.left() || left >= bar_area.right() {
            continue;
        }

        let block = Rect::from_min_max(
            Pos2::new(left.max(bar_area.left()), bar_area.top() + 3.0),
            Pos2::new(right.min(bar_area.right()), bar_area.bottom() - 3.0),
        );
        let color = action_category_color(event.category, theme);
        painter.rect_filled(block, RADIUS_S, color.linear_multiply(0.5));
        painter.rect_stroke(
            block,
            RADIUS_S,
            Stroke::new(STROKE_WIDTH, color),
            egui::StrokeKind::Outside,
        );
        if block.width() > 40.0 {
            painter.text(
                block.center(),
                Align2::CENTER_CENTER,
                &event.verb,
                FontId::monospace(10.0),
                theme.palette.text.primary,
            );
        }

        let response = ui.interact(
            block,
            ui.scope_id().with(("action_block", track_idx, event.start_time_ms)),
            Sense::hover(),
        );
        text_tooltip(
            ui,
            response.id.with("tooltip"),
            &response,
            &format!(
                "{:?}: {}\n{:.2}s → {:.2}s\nTargets: {}",
                event.category,
                event.verb,
                start_s,
                end_s,
                event.targets.join(", ")
            ),
        );
    }
}
