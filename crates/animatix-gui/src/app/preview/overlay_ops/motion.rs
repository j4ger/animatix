use super::execute::*;
use super::layout::*;
use super::*;

pub fn cycle_indicator_ops(
    theme: &eparts::Theme,
    mouse_pos: Pos2,
    cycle_index: usize,
    total_candidates: usize,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    if total_candidates <= 1 {
        return ops;
    }
    let indicator_text = format!("{}/{}", cycle_index + 1, total_candidates);
    let indicator_pos = egui::pos2(mouse_pos.x + 16.0, mouse_pos.y - 8.0);
    let font = egui::FontId::proportional(12.0);
    let size = painter_text_size(&indicator_text, font.clone());
    let rect = Rect::from_center_size(indicator_pos, size + Vec2::new(6.0, 3.0));
    push_screen_rect_fill(&mut ops, rect, RADIUS_M, theme.palette.accent.strong);
    push_screen_text(
        &mut ops,
        rect.left_center() + Vec2::new(3.0, -size.y / 2.0),
        Align2::LEFT_TOP,
        indicator_text,
        font,
        theme.palette.text.primary,
    );
    ops
}

/// Generate motion-path lines, keyframe dots, and time labels.
pub fn motion_path_ops(
    theme: &eparts::Theme,
    timeline: &Timeline,
    selected_actors: impl Iterator<Item = String>,
    current_time_s: f64,
    tx: PreviewTransform,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    for actor in selected_actors {
        let track = match timeline.get_track(&actor) {
            Some(track) => track,
            None => continue,
        };
        let pos_track = match &track.geometry.position {
            Some(track) => track,
            None => continue,
        };
        if pos_track.keyframes().len() < 2 {
            continue;
        }
        let mut kf_points: Vec<(u64, [f32; 2])> = Vec::new();
        for (&time_ms, (val, _)) in pos_track.keyframes() {
            kf_points.push((time_ms, *val));
        }
        kf_points.sort_by_key(|(t, _)| *t);

        let path_color = theme.palette.accent.primary.gamma_multiply(0.6);
        let path_stroke = OverlayStroke::new(1.5, path_color);
        for i in 0..kf_points.len().saturating_sub(1) {
            let p1 = kurbo::Point::new(kf_points[i].1[0] as f64, kf_points[i].1[1] as f64);
            let p2 = kurbo::Point::new(kf_points[i + 1].1[0] as f64, kf_points[i + 1].1[1] as f64);
            push_line(&mut ops, p1, p2, path_stroke);
        }

        for (time_ms, pos) in &kf_points {
            let scene_point = kurbo::Point::new(pos[0] as f64, pos[1] as f64);
            let current_time_ms = (current_time_s * 1000.0) as u64;
            let is_current = *time_ms == current_time_ms;
            let dot_color = if is_current {
                theme.palette.status.warning
            } else {
                theme.palette.accent.primary
            };
            let dot_radius = if is_current {
                px_to_scene(&tx, 5.0)
            } else {
                px_to_scene(&tx, 3.5)
            };
            push_circle_fill(&mut ops, scene_point, dot_radius, dot_color);
            if is_current {
                push_circle_stroke(
                    &mut ops,
                    scene_point,
                    dot_radius + px_to_scene(&tx, 2.0),
                    OverlayStroke::new(1.0, theme.palette.status.warning),
                );
            }
            let time_label = format!("{:.1}s", *time_ms as f64 / 1000.0);
            push_text(
                &mut ops,
                kurbo::Point::new(
                    scene_point.x,
                    scene_point.y - dot_radius as f64 - px_to_scene(&tx, 4.0) as f64,
                ),
                Align2::CENTER_BOTTOM,
                time_label,
                egui::FontId::monospace(10.0),
                theme.palette.text.muted,
            );
        }
    }
    ops
}
