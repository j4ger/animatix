use super::execute::*;
use super::*;

pub fn selection_overlay_ops(
    theme: &eparts::Theme,
    props: Option<&ActorProps>,
    fallback_rect: Option<kurbo::Rect>,
    is_dragging: bool,
    // Draw the pivot crosshair. Only true in Pivot mode (or while dragging the
    // pivot): an always-on crosshair at the actor centre implied a handle that
    // also stole centre drags.
    show_pivot: bool,
    pixels_per_point: f32,
    tx: PreviewTransform,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let selection_color = theme.palette.accent.primary;
    let accent_hover_color = theme.palette.accent.hover;
    let text_primary = theme.palette.text.primary;
    let text_faint = theme.palette.text.faint;
    let cross_color = theme.palette.status.warning;

    let stroke = if is_dragging {
        OverlayStroke::new(1.5, accent_hover_color)
    } else {
        OverlayStroke::new(1.5, selection_color)
    };

    if let Some(p) = props {
        let hw = p.size[0] / 2.0;
        let hh = p.size[1] / 2.0;
        let local_corners: [[f32; 2]; 4] = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
        let world_corners: [kurbo::Point; 4] =
            std::array::from_fn(|i| local_to_world(local_corners[i], p.position, p.rotation));

        for i in 0..4 {
            let next = (i + 1) % 4;
            push_line(&mut ops, world_corners[i], world_corners[next], stroke);
        }
        if is_dragging {
            for i in 0..4 {
                push_dashed(
                    &mut ops,
                    world_corners[i],
                    world_corners[(i + 1) % 4],
                    DASH_LEN,
                    GAP_LEN,
                    OverlayStroke::new(STROKE_WIDTH, text_faint),
                );
            }
        }

        let handle_world = world_handle_positions(p);
        let corner_radius = px_to_scene(&tx, HANDLE_SIZE * 0.6 * pixels_per_point);
        for &pos in handle_world[..4].iter() {
            push_circle_fill(&mut ops, pos, corner_radius, text_primary);
            push_circle_stroke(
                &mut ops,
                pos,
                corner_radius,
                OverlayStroke::new(1.5, selection_color),
            );
        }

        let edge_handle_px = px_to_scene(&tx, HANDLE_SIZE * 0.7 * pixels_per_point);
        for &pos in handle_world[4..].iter() {
            let handle_rect = kurbo::Rect::from_center_size(
                pos,
                kurbo::Size::new(edge_handle_px as f64, edge_handle_px as f64),
            );
            push_rect_fill(&mut ops, handle_rect, 1.0, text_primary);
            push_rect_stroke(
                &mut ops,
                handle_rect,
                1.0,
                OverlayStroke::new(STROKE_WIDTH, selection_color),
                StrokeKind::Outside,
            );
        }

        if !is_dragging {
            let arc_radius = px_to_scene(&tx, HANDLE_SIZE * 1.5 * pixels_per_point);
            let arc_stroke = OverlayStroke::new(STROKE_WIDTH, selection_color.gamma_multiply(0.5));
            let arc_angles: [(f32, f32); 4] = [
                (std::f32::consts::PI, 3.0 * std::f32::consts::PI / 2.0),
                (3.0 * std::f32::consts::PI / 2.0, 2.0 * std::f32::consts::PI),
                (0.0, std::f32::consts::PI / 2.0),
                (std::f32::consts::PI / 2.0, std::f32::consts::PI),
            ];
            for i in 0..4 {
                ops.push(PreviewOverlayOp::Arc {
                    center: handle_world[i],
                    radius: arc_radius,
                    start_angle: arc_angles[i].0,
                    end_angle: arc_angles[i].1,
                    stroke: arc_stroke,
                });
            }
        }

        let rot_world = rotation_handle_world(p);
        let top_center_local = [0.0_f32, -hh];
        let top_center_world = local_to_world(top_center_local, p.position, p.rotation);
        push_line(
            &mut ops,
            top_center_world,
            rot_world,
            OverlayStroke::new(STROKE_WIDTH, selection_color),
        );
        let rot_radius = px_to_scene(&tx, ROTATION_RADIUS * pixels_per_point);
        push_circle_fill(&mut ops, rot_world, rot_radius, text_primary);
        push_circle_stroke(
            &mut ops,
            rot_world,
            rot_radius,
            OverlayStroke::new(STROKE_WIDTH, selection_color),
        );

        if show_pivot {
            let pivot_world_pt = pivot_world(p);
            let pivot_world = kurbo::Point::new(pivot_world_pt[0] as f64, pivot_world_pt[1] as f64);
            let cross_size = px_to_scene(&tx, CROSS_SIZE * pixels_per_point);
            push_line(
                &mut ops,
                kurbo::Point::new(pivot_world.x - cross_size as f64, pivot_world.y),
                kurbo::Point::new(pivot_world.x + cross_size as f64, pivot_world.y),
                OverlayStroke::new(1.5, cross_color),
            );
            push_line(
                &mut ops,
                kurbo::Point::new(pivot_world.x, pivot_world.y - cross_size as f64),
                kurbo::Point::new(pivot_world.x, pivot_world.y + cross_size as f64),
                OverlayStroke::new(1.5, cross_color),
            );
            push_circle_stroke(
                &mut ops,
                pivot_world,
                cross_size + px_to_scene(&tx, 2.0 * pixels_per_point),
                OverlayStroke::new(STROKE_WIDTH, cross_color),
            );
        }
    } else if let Some(sel_rect) = fallback_rect {
        push_rect_stroke(&mut ops, sel_rect, 0.0, stroke, StrokeKind::Outside);
        if is_dragging {
            let dash_stroke = OverlayStroke::new(STROKE_WIDTH, text_faint);
            let corners = [
                kurbo::Point::new(sel_rect.x0, sel_rect.y0),
                kurbo::Point::new(sel_rect.x1, sel_rect.y0),
                kurbo::Point::new(sel_rect.x1, sel_rect.y1),
                kurbo::Point::new(sel_rect.x0, sel_rect.y1),
            ];
            for i in 0..4 {
                push_dashed(
                    &mut ops,
                    corners[i],
                    corners[(i + 1) % 4],
                    DASH_LEN,
                    GAP_LEN,
                    dash_stroke,
                );
            }
        }
        let handle_positions = scale_handle_positions(sel_rect);
        let handle_px = px_to_scene(&tx, HANDLE_SIZE);
        for pos in &handle_positions {
            let handle_rect = kurbo::Rect::from_center_size(
                *pos,
                kurbo::Size::new(handle_px as f64, handle_px as f64),
            );
            push_rect_fill(&mut ops, handle_rect, 1.0, text_primary);
            push_rect_stroke(
                &mut ops,
                handle_rect,
                1.0,
                OverlayStroke::new(STROKE_WIDTH, selection_color),
                StrokeKind::Outside,
            );
        }
        let top_center =
            kurbo::Point::new(sel_rect.x0 + (sel_rect.x1 - sel_rect.x0) / 2.0, sel_rect.y0);
        let rot_center = kurbo::Point::new(
            top_center.x,
            top_center.y - px_to_scene(&tx, ROTATION_OFFSET) as f64,
        );
        push_line(
            &mut ops,
            top_center,
            rot_center,
            OverlayStroke::new(STROKE_WIDTH, selection_color),
        );
        let rot_radius = px_to_scene(&tx, ROTATION_RADIUS);
        push_circle_fill(&mut ops, rot_center, rot_radius, text_primary);
        push_circle_stroke(
            &mut ops,
            rot_center,
            rot_radius,
            OverlayStroke::new(STROKE_WIDTH, selection_color),
        );
    }

    ops
}

/// Generate the union bounding box and handles for multi-selection.
pub fn multi_selection_overlay_ops(
    theme: &eparts::Theme,
    scene_rects: &[kurbo::Rect],
    is_dragging: bool,
    pixels_per_point: f32,
    tx: PreviewTransform,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    if scene_rects.is_empty() {
        return ops;
    }

    let mut min = kurbo::Point::new(scene_rects[0].x0, scene_rects[0].y0);
    let mut max = kurbo::Point::new(scene_rects[0].x1, scene_rects[0].y1);
    for rect in &scene_rects[1..] {
        min.x = min.x.min(rect.x0);
        min.y = min.y.min(rect.y0);
        max.x = max.x.max(rect.x1);
        max.y = max.y.max(rect.y1);
    }
    let union_rect = kurbo::Rect::new(min.x, min.y, max.x, max.y);

    let selection_color = theme.palette.accent.primary;
    let accent_hover_color = theme.palette.accent.hover;
    let text_primary = theme.palette.text.primary;
    let text_faint = theme.palette.text.faint;
    let stroke = if is_dragging {
        OverlayStroke::new(1.5, accent_hover_color)
    } else {
        OverlayStroke::new(1.5, selection_color)
    };

    push_rect_stroke(&mut ops, union_rect, 0.0, stroke, StrokeKind::Outside);
    if is_dragging {
        let dash_stroke = OverlayStroke::new(STROKE_WIDTH, text_faint);
        let corners = [
            kurbo::Point::new(union_rect.x0, union_rect.y0),
            kurbo::Point::new(union_rect.x1, union_rect.y0),
            kurbo::Point::new(union_rect.x1, union_rect.y1),
            kurbo::Point::new(union_rect.x0, union_rect.y1),
        ];
        for i in 0..4 {
            push_dashed(&mut ops, corners[i], corners[(i + 1) % 4], DASH_LEN, GAP_LEN, dash_stroke);
        }
    }

    let handle_positions = scale_handle_positions(union_rect);
    let handle_px = px_to_scene(&tx, HANDLE_SIZE * pixels_per_point);
    for pos in &handle_positions {
        let handle_rect = kurbo::Rect::from_center_size(
            *pos,
            kurbo::Size::new(handle_px as f64, handle_px as f64),
        );
        push_rect_fill(&mut ops, handle_rect, 1.0, text_primary);
        push_rect_stroke(
            &mut ops,
            handle_rect,
            1.0,
            OverlayStroke::new(STROKE_WIDTH, selection_color),
            StrokeKind::Outside,
        );
    }

    ops
}
