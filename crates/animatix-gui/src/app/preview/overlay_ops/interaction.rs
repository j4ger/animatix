use super::execute::*;
use super::layout::*;
use super::*;

pub fn ghost_overlay_ops(props: &ActorProps, color: Color32) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let hw = props.size[0] / 2.0;
    let hh = props.size[1] / 2.0;
    let local_corners: [[f32; 2]; 4] = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
    let world_corners: [kurbo::Point; 4] =
        std::array::from_fn(|i| local_to_world(local_corners[i], props.position, props.rotation));
    let dash_stroke = OverlayStroke::new(STROKE_WIDTH, color);
    for i in 0..4 {
        push_dashed(
            &mut ops,
            world_corners[i],
            world_corners[(i + 1) % 4],
            DASH_LEN,
            GAP_LEN,
            dash_stroke,
        );
    }
    ops
}

/// Generate the reorder ghost, insertion line, badge, and shift arrows.
pub fn reorder_overlay_ops(
    theme: &eparts::Theme,
    props: &ActorProps,
    target_index: usize,
    sibling_positions: &[(String, [f32; 2])],
    tx: PreviewTransform,
    is_row: bool,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let ghost_color = theme.palette.accent.hover;
    let hw = props.size[0] / 2.0;
    let hh = props.size[1] / 2.0;
    let local_corners: [[f32; 2]; 4] = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
    let world_corners: [kurbo::Point; 4] =
        std::array::from_fn(|i| local_to_world(local_corners[i], props.position, props.rotation));
    for i in 0..4 {
        let next = (i + 1) % 4;
        push_line(
            &mut ops,
            world_corners[i],
            world_corners[next],
            OverlayStroke::new(1.5, ghost_color),
        );
    }

    let coords: Vec<f32> = sibling_positions
        .iter()
        .map(|(_, pos)| if is_row { pos[0] } else { pos[1] })
        .collect();
    let insertion_coord = if coords.is_empty() {
        if is_row {
            props.position[0]
        } else {
            props.position[1]
        }
    } else if target_index == 0 {
        if coords.len() == 1 {
            coords[0]
        } else {
            coords[0] - (coords[1] - coords[0]) * 0.5
        }
    } else if target_index >= coords.len() {
        if coords.len() == 1 {
            coords[0]
        } else {
            let last = coords[coords.len() - 1];
            let prev = coords[coords.len() - 2];
            last + (last - prev) * 0.5
        }
    } else {
        (coords[target_index - 1] + coords[target_index]) * 0.5
    };

    let accent_color = theme.palette.accent.primary;
    let viewport = scene_viewport(&tx);
    let insertion_badge_pos = if is_row {
        let insertion_pt = tx.scene_to_screen(kurbo::Point::new(insertion_coord as f64, 0.0));
        push_line(
            &mut ops,
            kurbo::Point::new(insertion_coord as f64, viewport.y0),
            kurbo::Point::new(insertion_coord as f64, viewport.y1),
            OverlayStroke::new(2.5, accent_color),
        );
        tx.screen_to_scene(Pos2::new(insertion_pt.x, tx.preview_rect.top() + 16.0))
    } else {
        let insertion_pt = tx.scene_to_screen(kurbo::Point::new(0.0, insertion_coord as f64));
        push_line(
            &mut ops,
            kurbo::Point::new(viewport.x0, insertion_coord as f64),
            kurbo::Point::new(viewport.x1, insertion_coord as f64),
            OverlayStroke::new(2.5, accent_color),
        );
        tx.screen_to_scene(Pos2::new(tx.preview_rect.left() + 16.0, insertion_pt.y))
    };

    ops.push(PreviewOverlayOp::Badge {
        pos: insertion_badge_pos,
        text: format!("→ {}", target_index + 1),
        bg: theme.palette.overlay.badge_bg,
        fg: theme.palette.text.primary,
        border_stroke: OverlayStroke::new(STROKE_WIDTH, accent_color),
    });

    let shift_color = theme.palette.status.warning_subtle;
    for (i, (_, pos)) in sibling_positions.iter().enumerate() {
        let scene_pos = kurbo::Point::new(pos[0] as f64, pos[1] as f64);
        let arrow_size = px_to_scene(&tx, 8.0);
        let (dx, dy) = if i == target_index {
            if is_row {
                (arrow_size, 0.0)
            } else {
                (0.0, arrow_size)
            }
        } else if target_index > 0 && i == target_index - 1 {
            if is_row {
                (-arrow_size, 0.0)
            } else {
                (0.0, -arrow_size)
            }
        } else {
            continue;
        };
        ops.push(PreviewOverlayOp::Arrow {
            from: scene_pos,
            to: kurbo::Point::new(scene_pos.x + dx as f64, scene_pos.y + dy as f64),
            stroke: OverlayStroke::new(1.5, shift_color),
        });
    }

    let tooltip_pos = tx.preview_rect.left_top() + Vec2::new(10.0, 10.0);
    let tooltip_text = format!("Reorder: move to position {}", target_index + 1);
    let font = egui::FontId::proportional(12.0);
    let galley_size = painter_text_size(&tooltip_text, font.clone());
    let tooltip_rect = Rect::from_min_size(tooltip_pos, galley_size + Vec2::new(12.0, 8.0));
    push_screen_rect_fill(&mut ops, tooltip_rect, 4.0, theme.palette.overlay.tooltip_bg);
    push_screen_rect_stroke(
        &mut ops,
        tooltip_rect,
        4.0,
        OverlayStroke::new(STROKE_WIDTH, accent_color),
        StrokeKind::Outside,
    );
    push_screen_text(
        &mut ops,
        tooltip_rect.min + Vec2::new(6.0, 4.0),
        Align2::LEFT_TOP,
        tooltip_text,
        font,
        theme.palette.text.primary,
    );

    ops
}

/// Generate the hover dashed outline and actor-name tooltip.
pub fn hover_highlight_ops(
    theme: &eparts::Theme,
    hovered_actor: &str,
    hover_rect: kurbo::Rect,
    tx: PreviewTransform,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let hover_color = theme.palette.accent.ghost;
    let corners = [
        kurbo::Point::new(hover_rect.x0, hover_rect.y0),
        kurbo::Point::new(hover_rect.x1, hover_rect.y0),
        kurbo::Point::new(hover_rect.x1, hover_rect.y1),
        kurbo::Point::new(hover_rect.x0, hover_rect.y1),
    ];
    for i in 0..4 {
        push_dashed(
            &mut ops,
            corners[i],
            corners[(i + 1) % 4],
            DASH_LEN,
            GAP_LEN,
            OverlayStroke::new(STROKE_WIDTH, hover_color),
        );
    }

    let hover_screen = scene_rect_to_screen(&tx, &hover_rect);
    let tooltip_pos = egui::pos2(hover_screen.center().x, hover_screen.top() - 20.0);
    let font = egui::FontId::proportional(12.0);
    let text_size = painter_text_size(hovered_actor, font.clone());
    let tooltip_rect = Rect::from_center_size(tooltip_pos, text_size + Vec2::new(8.0, 4.0));
    push_screen_rect_fill(&mut ops, tooltip_rect, RADIUS_M, theme.palette.overlay.badge_bg);
    push_screen_rect_stroke(
        &mut ops,
        tooltip_rect,
        RADIUS_M,
        OverlayStroke::new(STROKE_WIDTH, theme.palette.border.default),
        StrokeKind::Outside,
    );
    push_screen_text(
        &mut ops,
        tooltip_rect.left_center() + Vec2::new(4.0, -text_size.y / 2.0),
        Align2::LEFT_TOP,
        hovered_actor,
        font,
        theme.palette.text.primary,
    );
    ops
}
