use super::execute::*;
use super::*;

pub fn layout_debug_ops(
    theme: &eparts::Theme,
    timeline: &Timeline,
    time_ms: u64,
    tx: PreviewTransform,
    draw_spacing: bool,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let style = LayoutDebugStyle::from_theme(theme);
    let viewport = scene_viewport(&tx);
    for (container_id, metadata) in timeline.container_layouts() {
        let container_label = timeline.get_entity_label(container_id).expect("container entity");
        let pos = timeline
            .world()
            .geometry(container_id)
            .position
            .as_ref()
            .map(|p| p.evaluate(time_ms))
            .unwrap_or([0.0, 0.0]);
        let size = timeline
            .world()
            .geometry(container_id)
            .size
            .as_ref()
            .map(|s| s.evaluate(time_ms))
            .unwrap_or([0.0, 0.0]);
        let half_w = size[0] / 2.0;
        let half_h = size[1] / 2.0;
        let container_rect = kurbo::Rect::new(
            (pos[0] - half_w) as f64,
            (pos[1] - half_h) as f64,
            (pos[0] + half_w) as f64,
            (pos[1] + half_h) as f64,
        );
        if container_rect.intersect(viewport).area() <= 0.0 {
            continue;
        }
        push_rect_stroke(
            &mut ops,
            container_rect,
            0.0,
            OverlayStroke::new(1.5, style.container_color),
            StrokeKind::Outside,
        );
        let kind_str = format!("{:?}", metadata.layout_type);
        push_text(
            &mut ops,
            kurbo::Point::new(
                container_rect.x0 + px_to_scene(&tx, 2.0) as f64,
                container_rect.y0 + px_to_scene(&tx, 2.0) as f64,
            ),
            Align2::LEFT_TOP,
            format!("{} ({})", container_label, kind_str),
            egui::FontId::monospace(10.0),
            style.container_color,
        );

        let layout_children = timeline
            .get_entity_id(container_label)
            .map(|container_id| timeline.layout_children_for(container_id))
            .unwrap_or_default();
        for child in layout_children.iter() {
            let child_id = child.id;
            let child_pos = timeline
                .world()
                .geometry(child_id)
                .position
                .as_ref()
                .map(|p| p.evaluate(time_ms))
                .unwrap_or([0.0, 0.0]);
            let child_size = timeline
                .world()
                .geometry(child_id)
                .size
                .as_ref()
                .map(|s| s.evaluate(time_ms))
                .unwrap_or([0.0, 0.0]);
            let child_rect = kurbo::Rect::new(
                (child_pos[0] - child_size[0] / 2.0) as f64,
                (child_pos[1] - child_size[1] / 2.0) as f64,
                (child_pos[0] + child_size[0] / 2.0) as f64,
                (child_pos[1] + child_size[1] / 2.0) as f64,
            );
            push_rect_stroke(
                &mut ops,
                child_rect,
                0.0,
                OverlayStroke::new(1.0, style.slot_color),
                StrokeKind::Outside,
            );
            let layout_s = timeline
                .world()
                .geometry(child_id)
                .layout_size_get(time_ms)
                .unwrap_or([0.0, 0.0]);
            push_text(
                &mut ops,
                kurbo::Point::new(
                    child_rect.x0 + px_to_scene(&tx, 2.0) as f64,
                    child_rect.y1 - px_to_scene(&tx, 12.0) as f64,
                ),
                Align2::LEFT_BOTTOM,
                format!("{:.0}×{:.0}", layout_s[0], layout_s[1]),
                egui::FontId::monospace(9.0),
                style.size_color,
            );
        }

        if draw_spacing {
            let is_row = metadata.layout_type == animatix::timeline::LayoutType::Row;
            let gap = metadata.gap;
            let children_ordered: Vec<([f32; 2], [f32; 2])> = layout_children
                .iter()
                .map(|child| {
                    let child_id = child.id;
                    let child_pos = timeline
                        .world()
                        .geometry(child_id)
                        .position
                        .as_ref()
                        .map(|p| p.evaluate(time_ms))
                        .unwrap_or([0.0, 0.0]);
                    let child_size = timeline
                        .world()
                        .geometry(child_id)
                        .size
                        .as_ref()
                        .map(|s| s.evaluate(time_ms))
                        .unwrap_or([0.0, 0.0]);
                    (child_pos, child_size)
                })
                .collect();
            if children_ordered.len() >= 2 && gap.iter().any(|&g| g > 0.0) {
                for pair in children_ordered.windows(2) {
                    let (pos_a, size_a) = pair[0];
                    let (pos_b, size_b) = pair[1];
                    let (gap_center, gap_size) = if is_row {
                        let right_a = pos_a[0] + size_a[0] / 2.0;
                        let left_b = pos_b[0] - size_b[0] / 2.0;
                        ((right_a + left_b) / 2.0, (left_b - right_a).abs().max(1.0))
                    } else {
                        let bottom_a = pos_a[1] + size_a[1] / 2.0;
                        let top_b = pos_b[1] - size_b[1] / 2.0;
                        ((bottom_a + top_b) / 2.0, (top_b - bottom_a).abs().max(1.0))
                    };
                    if gap_size <= 0.0 {
                        continue;
                    }
                    let (gap_tl, gap_br) = if is_row {
                        (
                            kurbo::Point::new(
                                (gap_center - gap_size / 2.0) as f64,
                                (pos_a[1] - size_a[1] / 2.0) as f64,
                            ),
                            kurbo::Point::new(
                                (gap_center + gap_size / 2.0) as f64,
                                (pos_a[1] + size_a[1] / 2.0) as f64,
                            ),
                        )
                    } else {
                        (
                            kurbo::Point::new(
                                (pos_a[0] - size_a[0] / 2.0) as f64,
                                (gap_center - gap_size / 2.0) as f64,
                            ),
                            kurbo::Point::new(
                                (pos_a[0] + size_a[0] / 2.0) as f64,
                                (gap_center + gap_size / 2.0) as f64,
                            ),
                        )
                    };
                    push_rect_fill(
                        &mut ops,
                        kurbo::Rect::new(gap_tl.x, gap_tl.y, gap_br.x, gap_br.y),
                        0.0,
                        style.spacing_color,
                    );
                }
            }
        }
    }
    ops
}

pub(super) fn painter_text_size(text: &str, font: FontId) -> Vec2 {
    // Approximate size without a painter. The overlay executor is responsible
    // for actual glyph layout; this keeps op generation deterministic.
    let chars = text.chars().count().max(1) as f32;
    Vec2::new(chars * font.size * 0.62, font.size * 1.2)
}

/// Generate the scene bounds outline, clipped to the visible viewport.
pub fn scene_bounds_ops(theme: &eparts::Theme, tx: PreviewTransform) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let scene_rect = kurbo::Rect::new(
        0.0,
        0.0,
        tx.scene_dimensions.width as f64,
        tx.scene_dimensions.height as f64,
    );
    let visible = scene_rect.intersect(scene_viewport(&tx));
    if visible.area() <= 0.0 {
        return ops;
    }
    push_rect_stroke(
        &mut ops,
        visible,
        0.0,
        OverlayStroke::new(STROKE_WIDTH, theme.palette.border.strong),
        StrokeKind::Inside,
    );
    ops
}
