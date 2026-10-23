use super::execute::*;
use super::*;

pub fn actor_label_ops(
    theme: &eparts::Theme,
    hit_regions: &[(String, kurbo::Rect)],
    tx: PreviewTransform,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    for (label, bounds) in hit_regions {
        let pos = kurbo::Point::new(
            (bounds.x0 + bounds.x1) / 2.0,
            bounds.y0 - px_to_scene(&tx, 4.0) as f64,
        );
        push_text(
            &mut ops,
            pos,
            Align2::CENTER_BOTTOM,
            label,
            egui::FontId::monospace(10.0),
            theme.palette.text.muted,
        );
    }
    ops
}

/// Generate the canvas grid overlay.
pub fn grid_ops(
    theme: &eparts::Theme,
    tx: PreviewTransform,
    grid_size: f32,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let grid_color = theme.palette.lines.grid;
    let grid_size = grid_size.max(1.0);
    let viewport = scene_viewport(&tx);
    let x0 = (viewport.x0 / grid_size as f64).floor() as i32 * grid_size as i32;
    let y0 = (viewport.y0 / grid_size as f64).floor() as i32 * grid_size as i32;
    let x1 = (viewport.x1 / grid_size as f64).ceil() as i32 * grid_size as i32;
    let y1 = (viewport.y1 / grid_size as f64).ceil() as i32 * grid_size as i32;

    let stroke = OverlayStroke::new(STROKE_WIDTH, grid_color);
    let mut x = x0 as f32;
    while x <= x1 as f32 {
        push_line(
            &mut ops,
            kurbo::Point::new(x as f64, viewport.y0),
            kurbo::Point::new(x as f64, viewport.y1),
            stroke,
        );
        x += grid_size;
    }
    let mut y = y0 as f32;
    while y <= y1 as f32 {
        push_line(
            &mut ops,
            kurbo::Point::new(viewport.x0, y as f64),
            kurbo::Point::new(viewport.x1, y as f64),
            stroke,
        );
        y += grid_size;
    }
    ops
}

/// Generate snap guide lines for a drag interaction.
pub fn snap_guide_ops(
    color: Color32,
    horizontal: &[f32],
    vertical: &[f32],
    tx: PreviewTransform,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let viewport = scene_viewport(&tx);
    let stroke = OverlayStroke::new(STROKE_WIDTH, color);
    for &sy in horizontal {
        push_line(
            &mut ops,
            kurbo::Point::new(viewport.x0, sy as f64),
            kurbo::Point::new(viewport.x1, sy as f64),
            stroke,
        );
    }
    for &sx in vertical {
        push_line(
            &mut ops,
            kurbo::Point::new(sx as f64, viewport.y0),
            kurbo::Point::new(sx as f64, viewport.y1),
            stroke,
        );
    }
    ops
}

#[cfg(test)]
mod tests {
    use animatix::timeline::SceneDimensions;

    use super::*;
    use crate::app::preview::PreviewTransform;

    fn theme() -> eparts::Theme {
        eparts::Theme::dark()
    }

    fn tx() -> PreviewTransform {
        PreviewTransform::new(
            SceneDimensions {
                width: 100,
                height: 100,
            },
            Rect::from_min_max(Pos2::ZERO, Pos2::new(200.0, 200.0)),
            1.0,
            Vec2::new(50.0, 50.0),
        )
    }

    #[test]
    fn selection_overlay_emits_outline_and_handles() {
        let props = ActorProps {
            position: [50.0, 50.0],
            size: [20.0, 10.0],
            rotation: 0.0,
            pivot_offset: [0.0, 0.0],
        };
        let ops = selection_overlay_ops(&theme(), Some(&props), None, false, true, 1.0, tx());
        assert!(
            ops.iter().any(|op| matches!(op, PreviewOverlayOp::Line { .. })),
            "selection overlay should emit bounding-box lines"
        );
        assert!(
            ops.iter().any(|op| matches!(op, PreviewOverlayOp::CircleFill { .. })),
            "selection overlay should emit handle fills"
        );
        assert!(
            ops.iter().any(|op| matches!(op, PreviewOverlayOp::Arc { .. })),
            "selection overlay should emit rotation arcs"
        );
    }

    #[test]
    fn hover_highlight_emits_dashed_outline_and_label() {
        let rect = kurbo::Rect::new(10.0, 10.0, 30.0, 20.0);
        let ops = hover_highlight_ops(&theme(), "box", rect, tx());
        assert!(ops.iter().any(|op| matches!(op, PreviewOverlayOp::DashedLine { .. })));
        assert!(
            ops.iter()
                .any(|op| matches!(op, PreviewOverlayOp::Screen(ScreenOverlayOp::Text { .. })))
        );
    }

    #[test]
    fn motion_paths_emit_lines_dots_and_labels() {
        // Minimal timeline with two position keyframes.
        let mut timeline = Timeline::new();
        timeline.spawn_entity("box", "Rect");
        let track = timeline.get_track_mut("box").expect("box track");
        track
            .geometry
            .position
            .get_or_insert_with(|| animatix::timeline::PropertyTrack::new([0.0, 0.0]))
            .add_keyframe(0, [0.0, 0.0], animatix::timeline::Easing::Linear);
        track.geometry.position.as_mut().unwrap().add_keyframe(
            1000,
            [100.0, 0.0],
            animatix::timeline::Easing::Linear,
        );

        let ops =
            motion_path_ops(&theme(), &timeline, std::iter::once("box".to_string()), 0.0, tx());
        assert!(ops.iter().any(|op| matches!(op, PreviewOverlayOp::Line { .. })));
        assert!(ops.iter().any(|op| matches!(op, PreviewOverlayOp::CircleFill { .. })));
        assert!(ops.iter().any(|op| matches!(op, PreviewOverlayOp::Text { .. })));
    }

    #[test]
    fn scene_bounds_grid_and_snap_lines_emit_expected_ops() {
        let t = tx();
        let bounds = scene_bounds_ops(&theme(), t);
        assert!(bounds.iter().any(|op| matches!(op, PreviewOverlayOp::RectStroke { .. })));

        let grid = grid_ops(&theme(), t, 20.0);
        assert!(grid.iter().any(|op| matches!(op, PreviewOverlayOp::Line { .. })));

        let snaps = snap_guide_ops(egui::Color32::RED, &[25.0], &[75.0], t);
        assert!(snaps.iter().any(|op| matches!(op, PreviewOverlayOp::Line { .. })));
    }

    #[test]
    fn actor_label_ops_emit_text_per_hit_region() {
        let ops = actor_label_ops(
            &theme(),
            &[("box".to_string(), kurbo::Rect::new(0.0, 0.0, 10.0, 10.0))],
            tx(),
        );
        assert!(ops.iter().any(|op| matches!(op, PreviewOverlayOp::Text { .. })));
    }
}
