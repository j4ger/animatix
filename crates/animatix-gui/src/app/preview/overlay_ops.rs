//! Screen-space overlay operations for the preview canvas.
//!
//! Preview overlays are generated as a list of [`PreviewOverlayOp`] and executed
//! by a single painter pass. This decouples overlay geometry/state from egui
//! painting so behavior can be unit-tested without a GPU or a live UI frame.

use animatix::timeline::Timeline;
use egui::{Align2, Color32, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2};

use super::{
    ActorProps, PreviewTransform, local_to_world, pivot_world, rotation_handle_world,
    world_handle_positions,
};
use crate::app::design_tokens::spatial::STROKE_WIDTH;

/// Colors used by the layout debug overlay.
struct LayoutDebugStyle {
    container_color: Color32,
    slot_color: Color32,
    size_color: Color32,
    spacing_color: Color32,
}

impl LayoutDebugStyle {
    fn from_theme(theme: &eparts::Theme) -> Self {
        Self {
            container_color: theme.accent.primary,
            slot_color: Color32::from_rgba_premultiplied(
                theme.status.warning.r(),
                theme.status.warning.g(),
                theme.status.warning.b(),
                150,
            ),
            size_color: theme.status.warning,
            spacing_color: Color32::from_rgba_premultiplied(200, 80, 80, 60),
        }
    }
}

const RADIUS_M: f32 = 4.0;

/// A stroke used by overlay operations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayStroke {
    pub width: f32,
    pub color: Color32,
}

impl OverlayStroke {
    pub fn new(width: f32, color: Color32) -> Self {
        Self { width, color }
    }
}

impl From<Stroke> for OverlayStroke {
    fn from(stroke: Stroke) -> Self {
        Self::new(stroke.width, stroke.color)
    }
}

impl From<&OverlayStroke> for Stroke {
    fn from(stroke: &OverlayStroke) -> Self {
        Stroke::new(stroke.width, stroke.color)
    }
}

/// A drawable preview overlay primitive.
///
/// All geometry is in screen space; builders receive a [`PreviewTransform`] when
/// they need to map scene coordinates to the canvas.
#[derive(Clone, Debug)]
pub enum PreviewOverlayOp {
    Line {
        from: Pos2,
        to: Pos2,
        stroke: OverlayStroke,
    },
    DashedLine {
        from: Pos2,
        to: Pos2,
        dash_len: f32,
        gap_len: f32,
        stroke: OverlayStroke,
    },
    RectStroke {
        rect: Rect,
        corner_radius: f32,
        stroke: OverlayStroke,
        stroke_kind: StrokeKind,
    },
    RectFill {
        rect: Rect,
        corner_radius: f32,
        color: Color32,
    },
    CircleFill {
        center: Pos2,
        radius: f32,
        color: Color32,
    },
    CircleStroke {
        center: Pos2,
        radius: f32,
        stroke: OverlayStroke,
    },
    Arc {
        center: Pos2,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        stroke: OverlayStroke,
    },
    Text {
        pos: Pos2,
        anchor: Align2,
        text: String,
        font: FontId,
        color: Color32,
    },
    Arrow {
        from: Pos2,
        to: Pos2,
        stroke: OverlayStroke,
    },
    Badge {
        pos: Pos2,
        text: String,
        bg: Color32,
        fg: Color32,
        border_stroke: OverlayStroke,
    },
}

/// Execute overlay operations onto the canvas painter.
pub fn execute_overlay_ops(painter: &egui::Painter, ops: &[PreviewOverlayOp]) {
    for op in ops {
        match op {
            PreviewOverlayOp::Line { from, to, stroke } => {
                painter.line_segment([*from, *to], stroke);
            },
            PreviewOverlayOp::DashedLine {
                from,
                to,
                dash_len,
                gap_len,
                stroke,
            } => {
                let total = from.distance(*to);
                let mut pos = 0.0;
                while pos < total {
                    let t0 = pos / total;
                    let t1 = ((pos + dash_len).min(total)) / total;
                    let p0 =
                        Pos2::new(from.x + (to.x - from.x) * t0, from.y + (to.y - from.y) * t0);
                    let p1 =
                        Pos2::new(from.x + (to.x - from.x) * t1, from.y + (to.y - from.y) * t1);
                    painter.line_segment([p0, p1], stroke);
                    pos += dash_len + gap_len;
                }
            },
            PreviewOverlayOp::RectStroke {
                rect,
                corner_radius,
                stroke,
                stroke_kind,
            } => {
                painter.rect_stroke(*rect, *corner_radius, stroke, *stroke_kind);
            },
            PreviewOverlayOp::RectFill {
                rect,
                corner_radius,
                color,
            } => {
                painter.rect_filled(*rect, *corner_radius, *color);
            },
            PreviewOverlayOp::CircleFill {
                center,
                radius,
                color,
            } => {
                painter.circle_filled(*center, *radius, *color);
            },
            PreviewOverlayOp::CircleStroke {
                center,
                radius,
                stroke,
            } => {
                painter.circle_stroke(*center, *radius, stroke);
            },
            PreviewOverlayOp::Arc {
                center,
                radius,
                start_angle,
                end_angle,
                stroke,
            } => {
                let segments = 8;
                for i in 0..segments {
                    let t0 = i as f32 / segments as f32;
                    let t1 = (i + 1) as f32 / segments as f32;
                    let a0 = start_angle + (end_angle - start_angle) * t0;
                    let a1 = start_angle + (end_angle - start_angle) * t1;
                    let p0 = Pos2::new(center.x + radius * a0.cos(), center.y + radius * a0.sin());
                    let p1 = Pos2::new(center.x + radius * a1.cos(), center.y + radius * a1.sin());
                    painter.line_segment([p0, p1], stroke);
                }
            },
            PreviewOverlayOp::Text {
                pos,
                anchor,
                text,
                font,
                color,
            } => {
                painter.text(*pos, *anchor, text, font.clone(), *color);
            },
            PreviewOverlayOp::Arrow { from, to, stroke } => {
                painter.arrow(*from, *to - *from, stroke);
            },
            PreviewOverlayOp::Badge {
                pos,
                text,
                bg,
                fg,
                border_stroke,
            } => {
                let galley = painter.layout_no_wrap(text.clone(), FontId::proportional(12.0), *fg);
                let size = galley.size() + Vec2::new(8.0, 4.0);
                let rect = Rect::from_center_size(*pos, size);
                painter.rect_filled(rect, 2.0, *bg);
                painter.rect_stroke(rect, 2.0, border_stroke, StrokeKind::Outside);
                painter.galley(rect.min + Vec2::new(4.0, 2.0), galley, *fg);
            },
        }
    }
}

/// Add a dashed line op to the given list.
fn push_dashed(
    ops: &mut Vec<PreviewOverlayOp>,
    from: Pos2,
    to: Pos2,
    dash_len: f32,
    gap_len: f32,
    stroke: OverlayStroke,
) {
    ops.push(PreviewOverlayOp::DashedLine {
        from,
        to,
        dash_len,
        gap_len,
        stroke,
    });
}

fn push_line(ops: &mut Vec<PreviewOverlayOp>, from: Pos2, to: Pos2, stroke: OverlayStroke) {
    ops.push(PreviewOverlayOp::Line { from, to, stroke });
}

fn push_rect_stroke(
    ops: &mut Vec<PreviewOverlayOp>,
    rect: Rect,
    corner_radius: f32,
    stroke: OverlayStroke,
    stroke_kind: StrokeKind,
) {
    ops.push(PreviewOverlayOp::RectStroke {
        rect,
        corner_radius,
        stroke,
        stroke_kind,
    });
}

fn push_circle_fill(ops: &mut Vec<PreviewOverlayOp>, center: Pos2, radius: f32, color: Color32) {
    ops.push(PreviewOverlayOp::CircleFill {
        center,
        radius,
        color,
    });
}

fn push_circle_stroke(
    ops: &mut Vec<PreviewOverlayOp>,
    center: Pos2,
    radius: f32,
    stroke: OverlayStroke,
) {
    ops.push(PreviewOverlayOp::CircleStroke {
        center,
        radius,
        stroke,
    });
}

fn push_text(
    ops: &mut Vec<PreviewOverlayOp>,
    pos: Pos2,
    anchor: Align2,
    text: impl Into<String>,
    font: FontId,
    color: Color32,
) {
    ops.push(PreviewOverlayOp::Text {
        pos,
        anchor,
        text: text.into(),
        font,
        color,
    });
}

const DASH_LEN: f32 = 4.0;
const GAP_LEN: f32 = 3.0;
const HANDLE_SIZE: f32 = 8.0;
const ROTATION_OFFSET: f32 = 24.0;
const ROTATION_RADIUS: f32 = 6.0;
const CROSS_SIZE: f32 = 8.0;

fn scale_handle_positions(sel_rect: Rect) -> [Pos2; 8] {
    [
        sel_rect.left_top(),
        sel_rect.right_top(),
        sel_rect.right_bottom(),
        sel_rect.left_bottom(),
        Pos2::new(sel_rect.center().x, sel_rect.top()),
        Pos2::new(sel_rect.right(), sel_rect.center().y),
        Pos2::new(sel_rect.center().x, sel_rect.bottom()),
        Pos2::new(sel_rect.left(), sel_rect.center().y),
    ]
}

/// Generate the single-selection outline, handles, rotation ring, and pivot.
pub fn selection_overlay_ops(
    theme: &eparts::Theme,
    props: Option<&ActorProps>,
    fallback_rect: Option<Rect>,
    is_dragging: bool,
    pixels_per_point: f32,
    tx: PreviewTransform,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let selection_color = theme.accent.primary;
    let accent_hover_color = theme.accent.hover;
    let text_primary = theme.text.primary;
    let text_faint = theme.text.faint;
    let cross_color = theme.status.warning;

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
        let screen_corners: [Pos2; 4] =
            std::array::from_fn(|i| tx.scene_to_screen(world_corners[i]));

        for i in 0..4 {
            let next = (i + 1) % 4;
            push_line(&mut ops, screen_corners[i], screen_corners[next], stroke);
        }
        if is_dragging {
            for i in 0..4 {
                push_dashed(
                    &mut ops,
                    screen_corners[i],
                    screen_corners[(i + 1) % 4],
                    DASH_LEN,
                    GAP_LEN,
                    OverlayStroke::new(STROKE_WIDTH, text_faint),
                );
            }
        }

        let handle_world = world_handle_positions(p);
        let handle_screen: [Pos2; 8] = std::array::from_fn(|i| tx.scene_to_screen(handle_world[i]));

        let corner_radius = HANDLE_SIZE * 0.6 * pixels_per_point;
        for &pos in handle_screen[..4].iter() {
            push_circle_fill(&mut ops, pos, corner_radius, text_primary);
            push_circle_stroke(
                &mut ops,
                pos,
                corner_radius,
                OverlayStroke::new(1.5, selection_color),
            );
        }

        let edge_handle_px = HANDLE_SIZE * 0.7 * pixels_per_point;
        for &pos in handle_screen[4..].iter() {
            let handle_rect =
                Rect::from_center_size(pos, Vec2::new(edge_handle_px, edge_handle_px));
            ops.push(PreviewOverlayOp::RectFill {
                rect: handle_rect,
                corner_radius: 1.0,
                color: text_primary,
            });
            push_rect_stroke(
                &mut ops,
                handle_rect,
                1.0,
                OverlayStroke::new(STROKE_WIDTH, selection_color),
                StrokeKind::Outside,
            );
        }

        if !is_dragging {
            let arc_radius = HANDLE_SIZE * 1.5 * pixels_per_point;
            let arc_stroke = OverlayStroke::new(STROKE_WIDTH, selection_color.gamma_multiply(0.5));
            let arc_angles: [(f32, f32); 4] = [
                (std::f32::consts::PI, 3.0 * std::f32::consts::PI / 2.0),
                (3.0 * std::f32::consts::PI / 2.0, 2.0 * std::f32::consts::PI),
                (0.0, std::f32::consts::PI / 2.0),
                (std::f32::consts::PI / 2.0, std::f32::consts::PI),
            ];
            for i in 0..4 {
                ops.push(PreviewOverlayOp::Arc {
                    center: handle_screen[i],
                    radius: arc_radius,
                    start_angle: arc_angles[i].0,
                    end_angle: arc_angles[i].1,
                    stroke: arc_stroke,
                });
            }
        }

        let rot_world = rotation_handle_world(p);
        let rot_screen = tx.scene_to_screen(rot_world);
        let top_center_local = [0.0_f32, -hh];
        let top_center_world = local_to_world(top_center_local, p.position, p.rotation);
        let top_center_screen = tx.scene_to_screen(top_center_world);
        push_line(
            &mut ops,
            top_center_screen,
            rot_screen,
            OverlayStroke::new(STROKE_WIDTH, selection_color),
        );
        let rot_radius = ROTATION_RADIUS * pixels_per_point;
        push_circle_fill(&mut ops, rot_screen, rot_radius, text_primary);
        push_circle_stroke(
            &mut ops,
            rot_screen,
            rot_radius,
            OverlayStroke::new(STROKE_WIDTH, selection_color),
        );

        let pivot_world_pt = pivot_world(p);
        let pivot_screen = tx
            .scene_to_screen(kurbo::Point::new(pivot_world_pt[0] as f64, pivot_world_pt[1] as f64));
        let cross_size = CROSS_SIZE * pixels_per_point;
        push_line(
            &mut ops,
            Pos2::new(pivot_screen.x - cross_size, pivot_screen.y),
            Pos2::new(pivot_screen.x + cross_size, pivot_screen.y),
            OverlayStroke::new(1.5, cross_color),
        );
        push_line(
            &mut ops,
            Pos2::new(pivot_screen.x, pivot_screen.y - cross_size),
            Pos2::new(pivot_screen.x, pivot_screen.y + cross_size),
            OverlayStroke::new(1.5, cross_color),
        );
        push_circle_stroke(
            &mut ops,
            pivot_screen,
            cross_size + 2.0 * pixels_per_point,
            OverlayStroke::new(STROKE_WIDTH, cross_color),
        );
    } else if let Some(sel_rect) = fallback_rect {
        push_rect_stroke(&mut ops, sel_rect, 0.0, stroke, StrokeKind::Outside);
        if is_dragging {
            let dash_stroke = OverlayStroke::new(STROKE_WIDTH, text_faint);
            let corners = [
                sel_rect.left_top(),
                sel_rect.right_top(),
                sel_rect.right_bottom(),
                sel_rect.left_bottom(),
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
        for pos in &handle_positions {
            let handle_rect = Rect::from_center_size(*pos, Vec2::new(HANDLE_SIZE, HANDLE_SIZE));
            ops.push(PreviewOverlayOp::RectFill {
                rect: handle_rect,
                corner_radius: 1.0,
                color: text_primary,
            });
            push_rect_stroke(
                &mut ops,
                handle_rect,
                1.0,
                OverlayStroke::new(STROKE_WIDTH, selection_color),
                StrokeKind::Outside,
            );
        }
        let top_center = Pos2::new(sel_rect.center().x, sel_rect.top());
        let rot_center = Pos2::new(top_center.x, top_center.y - ROTATION_OFFSET);
        push_line(
            &mut ops,
            top_center,
            rot_center,
            OverlayStroke::new(STROKE_WIDTH, selection_color),
        );
        push_circle_fill(&mut ops, rot_center, ROTATION_RADIUS, text_primary);
        push_circle_stroke(
            &mut ops,
            rot_center,
            ROTATION_RADIUS,
            OverlayStroke::new(STROKE_WIDTH, selection_color),
        );
    }

    ops
}

/// Generate the union bounding box and handles for multi-selection.
pub fn multi_selection_overlay_ops(
    theme: &eparts::Theme,
    screen_rects: &[Rect],
    is_dragging: bool,
    pixels_per_point: f32,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    if screen_rects.is_empty() {
        return ops;
    }

    let mut min = screen_rects[0].min;
    let mut max = screen_rects[0].max;
    for rect in &screen_rects[1..] {
        min.x = min.x.min(rect.min.x);
        min.y = min.y.min(rect.min.y);
        max.x = max.x.max(rect.max.x);
        max.y = max.y.max(rect.max.y);
    }
    let union_rect = Rect::from_min_max(min, max);

    let selection_color = theme.accent.primary;
    let accent_hover_color = theme.accent.hover;
    let text_primary = theme.text.primary;
    let text_faint = theme.text.faint;
    let stroke = if is_dragging {
        OverlayStroke::new(1.5, accent_hover_color)
    } else {
        OverlayStroke::new(1.5, selection_color)
    };

    push_rect_stroke(&mut ops, union_rect, 0.0, stroke, StrokeKind::Outside);
    if is_dragging {
        let dash_stroke = OverlayStroke::new(STROKE_WIDTH, text_faint);
        let corners = [
            union_rect.left_top(),
            union_rect.right_top(),
            union_rect.right_bottom(),
            union_rect.left_bottom(),
        ];
        for i in 0..4 {
            push_dashed(&mut ops, corners[i], corners[(i + 1) % 4], DASH_LEN, GAP_LEN, dash_stroke);
        }
    }

    let handle_positions = scale_handle_positions(union_rect);
    let handle_px = HANDLE_SIZE * pixels_per_point;
    for pos in &handle_positions {
        let handle_rect = Rect::from_center_size(*pos, Vec2::new(handle_px, handle_px));
        ops.push(PreviewOverlayOp::RectFill {
            rect: handle_rect,
            corner_radius: 1.0,
            color: text_primary,
        });
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

/// Generate dashed ghost outlines for an actor at another point in time.
pub fn ghost_overlay_ops(
    props: &ActorProps,
    tx: PreviewTransform,
    color: Color32,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let hw = props.size[0] / 2.0;
    let hh = props.size[1] / 2.0;
    let local_corners: [[f32; 2]; 4] = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
    let world_corners: [kurbo::Point; 4] =
        std::array::from_fn(|i| local_to_world(local_corners[i], props.position, props.rotation));
    let screen_corners: [Pos2; 4] = std::array::from_fn(|i| tx.scene_to_screen(world_corners[i]));
    let dash_stroke = OverlayStroke::new(STROKE_WIDTH, color);
    for i in 0..4 {
        push_dashed(
            &mut ops,
            screen_corners[i],
            screen_corners[(i + 1) % 4],
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
    let ghost_color = theme.accent.hover;
    let hw = props.size[0] / 2.0;
    let hh = props.size[1] / 2.0;
    let local_corners: [[f32; 2]; 4] = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
    let world_corners: [kurbo::Point; 4] =
        std::array::from_fn(|i| local_to_world(local_corners[i], props.position, props.rotation));
    let screen_corners: [Pos2; 4] = std::array::from_fn(|i| tx.scene_to_screen(world_corners[i]));
    for i in 0..4 {
        let next = (i + 1) % 4;
        push_line(
            &mut ops,
            screen_corners[i],
            screen_corners[next],
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

    let accent_color = theme.accent.primary;
    let insertion_screen = if is_row {
        let insertion_pt = tx.scene_to_screen(kurbo::Point::new(insertion_coord as f64, 0.0));
        push_line(
            &mut ops,
            Pos2::new(insertion_pt.x, tx.preview_rect.top()),
            Pos2::new(insertion_pt.x, tx.preview_rect.bottom()),
            OverlayStroke::new(2.5, accent_color),
        );
        Pos2::new(insertion_pt.x, tx.preview_rect.top() + 16.0)
    } else {
        let insertion_pt = tx.scene_to_screen(kurbo::Point::new(0.0, insertion_coord as f64));
        push_line(
            &mut ops,
            Pos2::new(tx.preview_rect.left(), insertion_pt.y),
            Pos2::new(tx.preview_rect.right(), insertion_pt.y),
            OverlayStroke::new(2.5, accent_color),
        );
        Pos2::new(tx.preview_rect.left() + 16.0, insertion_pt.y)
    };

    ops.push(PreviewOverlayOp::Badge {
        pos: insertion_screen,
        text: format!("→ {}", target_index + 1),
        bg: theme.overlay.badge_bg,
        fg: theme.text.primary,
        border_stroke: OverlayStroke::new(STROKE_WIDTH, accent_color),
    });

    let shift_color = theme.status.warning_subtle;
    for (i, (_, pos)) in sibling_positions.iter().enumerate() {
        let screen_pos = tx.scene_to_screen(kurbo::Point::new(pos[0] as f64, pos[1] as f64));
        let arrow_size = 8.0;
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
            from: screen_pos,
            to: Pos2::new(screen_pos.x + dx, screen_pos.y + dy),
            stroke: OverlayStroke::new(1.5, shift_color),
        });
    }

    let tooltip_pos = tx.preview_rect.left_top() + Vec2::new(10.0, 10.0);
    let tooltip_text = format!("Reorder: move to position {}", target_index + 1);
    let font = egui::FontId::proportional(12.0);
    let galley_size = painter_text_size(&tooltip_text, font.clone());
    let tooltip_rect = Rect::from_min_size(tooltip_pos, galley_size + Vec2::new(12.0, 8.0));
    ops.push(PreviewOverlayOp::RectFill {
        rect: tooltip_rect,
        corner_radius: 4.0,
        color: theme.overlay.tooltip_bg,
    });
    push_rect_stroke(
        &mut ops,
        tooltip_rect,
        4.0,
        OverlayStroke::new(STROKE_WIDTH, accent_color),
        StrokeKind::Outside,
    );
    push_text(
        &mut ops,
        tooltip_rect.min + Vec2::new(6.0, 4.0),
        Align2::LEFT_TOP,
        tooltip_text,
        font,
        theme.text.primary,
    );

    ops
}

/// Generate the hover dashed outline and actor-name tooltip.
pub fn hover_highlight_ops(
    theme: &eparts::Theme,
    hovered_actor: &str,
    hover_rect: Rect,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let hover_color = theme.accent.ghost;
    let corners = [
        hover_rect.left_top(),
        hover_rect.right_top(),
        hover_rect.right_bottom(),
        hover_rect.left_bottom(),
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

    let tooltip_pos = egui::pos2(hover_rect.center().x, hover_rect.top() - 20.0);
    let font = egui::FontId::proportional(12.0);
    let text_size = painter_text_size(hovered_actor, font.clone());
    let tooltip_rect = Rect::from_center_size(tooltip_pos, text_size + Vec2::new(8.0, 4.0));
    ops.push(PreviewOverlayOp::RectFill {
        rect: tooltip_rect,
        corner_radius: RADIUS_M,
        color: theme.overlay.badge_bg,
    });
    push_rect_stroke(
        &mut ops,
        tooltip_rect,
        RADIUS_M,
        OverlayStroke::new(STROKE_WIDTH, theme.border.default),
        StrokeKind::Outside,
    );
    push_text(
        &mut ops,
        tooltip_rect.left_center() + Vec2::new(4.0, -text_size.y / 2.0),
        Align2::LEFT_TOP,
        hovered_actor,
        font,
        theme.text.primary,
    );
    ops
}

/// Generate the click-cycle indicator near the pointer.
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
    ops.push(PreviewOverlayOp::RectFill {
        rect,
        corner_radius: RADIUS_M,
        color: theme.accent.strong,
    });
    push_text(
        &mut ops,
        rect.left_center() + Vec2::new(3.0, -size.y / 2.0),
        Align2::LEFT_TOP,
        indicator_text,
        font,
        theme.text.primary,
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

        let path_color = theme.accent.primary.gamma_multiply(0.6);
        let path_stroke = OverlayStroke::new(1.5, path_color);
        for i in 0..kf_points.len().saturating_sub(1) {
            let p1_screen = tx.scene_to_screen(kurbo::Point::new(
                kf_points[i].1[0] as f64,
                kf_points[i].1[1] as f64,
            ));
            let p2_screen = tx.scene_to_screen(kurbo::Point::new(
                kf_points[i + 1].1[0] as f64,
                kf_points[i + 1].1[1] as f64,
            ));
            push_line(&mut ops, p1_screen, p2_screen, path_stroke);
        }

        for (time_ms, pos) in &kf_points {
            let screen = tx.scene_to_screen(kurbo::Point::new(pos[0] as f64, pos[1] as f64));
            let current_time_ms = (current_time_s * 1000.0) as u64;
            let is_current = *time_ms == current_time_ms;
            let dot_color = if is_current {
                theme.status.warning
            } else {
                theme.accent.primary
            };
            let dot_radius = if is_current { 5.0 } else { 3.5 };
            push_circle_fill(&mut ops, screen, dot_radius, dot_color);
            if is_current {
                push_circle_stroke(
                    &mut ops,
                    screen,
                    dot_radius + 2.0,
                    OverlayStroke::new(1.0, theme.status.warning),
                );
            }
            let time_label = format!("{:.1}s", *time_ms as f64 / 1000.0);
            push_text(
                &mut ops,
                egui::pos2(screen.x, screen.y - dot_radius - 4.0),
                Align2::CENTER_BOTTOM,
                time_label,
                egui::FontId::monospace(10.0),
                theme.text.muted,
            );
        }
    }
    ops
}

/// Generate the container/layout debug overlay.
pub fn layout_debug_ops(
    theme: &eparts::Theme,
    timeline: &Timeline,
    time_ms: u64,
    tx: PreviewTransform,
    draw_spacing: bool,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let style = LayoutDebugStyle::from_theme(theme);
    for (container_label, metadata) in timeline.container_metadata() {
        let Some(track) = timeline.get_track(container_label) else {
            continue;
        };
        let pos = track
            .geometry
            .position
            .as_ref()
            .map(|p| p.evaluate(time_ms))
            .unwrap_or([0.0, 0.0]);
        let size = track.geometry.size.as_ref().map(|s| s.evaluate(time_ms)).unwrap_or([0.0, 0.0]);
        let half_w = size[0] / 2.0;
        let half_h = size[1] / 2.0;
        let container_tl = tx
            .scene_to_screen(kurbo::Point::new((pos[0] - half_w) as f64, (pos[1] - half_h) as f64));
        let container_br = tx
            .scene_to_screen(kurbo::Point::new((pos[0] + half_w) as f64, (pos[1] + half_h) as f64));
        let container_rect = Rect::from_min_max(container_tl, container_br);
        if !container_rect.intersects(tx.preview_rect) {
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
            Pos2::new(container_rect.left() + 2.0, container_rect.top() + 2.0),
            Align2::LEFT_TOP,
            format!("{} ({})", container_label, kind_str),
            egui::FontId::monospace(10.0),
            style.container_color,
        );

        let layout_children = timeline.layout_children_for(container_label);
        for child in &layout_children {
            let Some(child_track) = timeline.get_track(&child.label) else {
                continue;
            };
            let child_pos = child_track
                .geometry
                .position
                .as_ref()
                .map(|p| p.evaluate(time_ms))
                .unwrap_or([0.0, 0.0]);
            let child_size = child_track
                .geometry
                .size
                .as_ref()
                .map(|s| s.evaluate(time_ms))
                .unwrap_or([0.0, 0.0]);
            let child_tl = tx.scene_to_screen(kurbo::Point::new(
                (child_pos[0] - child_size[0] / 2.0) as f64,
                (child_pos[1] - child_size[1] / 2.0) as f64,
            ));
            let child_br = tx.scene_to_screen(kurbo::Point::new(
                (child_pos[0] + child_size[0] / 2.0) as f64,
                (child_pos[1] + child_size[1] / 2.0) as f64,
            ));
            let child_rect = Rect::from_min_max(child_tl, child_br);
            push_rect_stroke(
                &mut ops,
                child_rect,
                0.0,
                OverlayStroke::new(1.0, style.slot_color),
                StrokeKind::Outside,
            );
            let layout_s = child_track.layout_size_get(time_ms).unwrap_or([0.0, 0.0]);
            push_text(
                &mut ops,
                Pos2::new(child_rect.left() + 2.0, child_rect.bottom() - 12.0),
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
                .filter_map(|child| {
                    let track = timeline.get_track(&child.label)?;
                    let child_pos = track
                        .geometry
                        .position
                        .as_ref()
                        .map(|p| p.evaluate(time_ms))
                        .unwrap_or([0.0, 0.0]);
                    let child_size = track
                        .geometry
                        .size
                        .as_ref()
                        .map(|s| s.evaluate(time_ms))
                        .unwrap_or([0.0, 0.0]);
                    Some((child_pos, child_size))
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
                            tx.scene_to_screen(kurbo::Point::new(
                                (gap_center - gap_size / 2.0) as f64,
                                (pos_a[1] - size_a[1] / 2.0) as f64,
                            )),
                            tx.scene_to_screen(kurbo::Point::new(
                                (gap_center + gap_size / 2.0) as f64,
                                (pos_a[1] + size_a[1] / 2.0) as f64,
                            )),
                        )
                    } else {
                        (
                            tx.scene_to_screen(kurbo::Point::new(
                                (pos_a[0] - size_a[0] / 2.0) as f64,
                                (gap_center - gap_size / 2.0) as f64,
                            )),
                            tx.scene_to_screen(kurbo::Point::new(
                                (pos_a[0] + size_a[0] / 2.0) as f64,
                                (gap_center + gap_size / 2.0) as f64,
                            )),
                        )
                    };
                    ops.push(PreviewOverlayOp::RectFill {
                        rect: Rect::from_min_max(gap_tl, gap_br),
                        corner_radius: 0.0,
                        color: style.spacing_color,
                    });
                }
            }
        }
    }
    ops
}

fn painter_text_size(text: &str, font: FontId) -> Vec2 {
    // Approximate size without a painter. The overlay executor is responsible
    // for actual glyph layout; this keeps op generation deterministic.
    let chars = text.chars().count().max(1) as f32;
    Vec2::new(chars * font.size * 0.62, font.size * 1.2)
}

/// Generate the scene bounds outline.
pub fn scene_bounds_ops(
    theme: &eparts::Theme,
    tx: PreviewTransform,
    preview_rect: Rect,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let tl = tx.scene_to_screen(kurbo::Point::new(0.0, 0.0));
    let br = tx.scene_to_screen(kurbo::Point::new(
        tx.scene_dimensions.width as f64,
        tx.scene_dimensions.height as f64,
    ));
    let bounds_screen = Rect::from_min_max(tl, br).intersect(preview_rect);
    if bounds_screen.is_positive() {
        push_rect_stroke(
            &mut ops,
            bounds_screen,
            0.0,
            OverlayStroke::new(STROKE_WIDTH, theme.border.strong),
            StrokeKind::Inside,
        );
    }
    ops
}

/// Generate actor label text near each hit region.
pub fn actor_label_ops(
    theme: &eparts::Theme,
    hit_regions: &[(String, kurbo::Rect)],
    tx: PreviewTransform,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    for (label, bounds) in hit_regions {
        let center =
            tx.scene_to_screen(kurbo::Point::new((bounds.x0 + bounds.x1) / 2.0, bounds.y0 - 4.0));
        push_text(
            &mut ops,
            center,
            Align2::CENTER_BOTTOM,
            label,
            egui::FontId::monospace(10.0),
            theme.text.muted,
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
    let grid_color = theme.lines.grid;
    let grid_size = grid_size.max(1.0);
    let preview_rect = tx.preview_rect;

    let scene_tl = tx.screen_to_scene(preview_rect.left_top());
    let scene_br = tx.screen_to_scene(preview_rect.right_bottom());
    let x0 = (scene_tl.x / grid_size as f64).floor() as i32 * grid_size as i32;
    let y0 = (scene_tl.y / grid_size as f64).floor() as i32 * grid_size as i32;
    let x1 = (scene_br.x / grid_size as f64).ceil() as i32 * grid_size as i32;
    let y1 = (scene_br.y / grid_size as f64).ceil() as i32 * grid_size as i32;

    let stroke = OverlayStroke::new(STROKE_WIDTH, grid_color);
    let mut x = x0 as f32;
    while x <= x1 as f32 {
        let screen_pt = tx.scene_to_screen(kurbo::Point::new(x as f64, 0.0));
        if screen_pt.x >= preview_rect.min.x && screen_pt.x <= preview_rect.max.x {
            push_line(
                &mut ops,
                Pos2::new(screen_pt.x, preview_rect.min.y),
                Pos2::new(screen_pt.x, preview_rect.max.y),
                stroke,
            );
        }
        x += grid_size;
    }
    let mut y = y0 as f32;
    while y <= y1 as f32 {
        let screen_pt = tx.scene_to_screen(kurbo::Point::new(0.0, y as f64));
        if screen_pt.y >= preview_rect.min.y && screen_pt.y <= preview_rect.max.y {
            push_line(
                &mut ops,
                Pos2::new(preview_rect.min.x, screen_pt.y),
                Pos2::new(preview_rect.max.x, screen_pt.y),
                stroke,
            );
        }
        y += grid_size;
    }
    ops
}

/// Generate snap guide lines for a drag interaction.
pub fn snap_guide_ops(
    color: Color32,
    preview_rect: Rect,
    horizontal: &[f32],
    vertical: &[f32],
    tx: PreviewTransform,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let stroke = OverlayStroke::new(STROKE_WIDTH, color);
    for &sy in horizontal {
        let screen_pt = tx.scene_to_screen(kurbo::Point::new(0.0, sy as f64));
        if screen_pt.y >= preview_rect.min.y && screen_pt.y <= preview_rect.max.y {
            push_line(
                &mut ops,
                Pos2::new(preview_rect.min.x, screen_pt.y),
                Pos2::new(preview_rect.max.x, screen_pt.y),
                stroke,
            );
        }
    }
    for &sx in vertical {
        let screen_pt = tx.scene_to_screen(kurbo::Point::new(sx as f64, 0.0));
        if screen_pt.x >= preview_rect.min.x && screen_pt.x <= preview_rect.max.x {
            push_line(
                &mut ops,
                Pos2::new(screen_pt.x, preview_rect.min.y),
                Pos2::new(screen_pt.x, preview_rect.max.y),
                stroke,
            );
        }
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
        let ops = selection_overlay_ops(&theme(), Some(&props), None, false, 1.0, tx());
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
        let rect = Rect::from_min_max(Pos2::new(10.0, 10.0), Pos2::new(30.0, 20.0));
        let ops = hover_highlight_ops(&theme(), "box", rect);
        assert!(ops.iter().any(|op| matches!(op, PreviewOverlayOp::DashedLine { .. })));
        assert!(ops.iter().any(|op| matches!(op, PreviewOverlayOp::Text { .. })));
    }

    #[test]
    fn motion_paths_emit_lines_dots_and_labels() {
        // Minimal timeline with two position keyframes.
        let timeline = Timeline::new();
        let mut timeline = timeline;
        let mut track = animatix::timeline::AnimationTrack::new("box".to_string());
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
        timeline.tracks_mut().insert("box".to_string(), track);

        let ops =
            motion_path_ops(&theme(), &timeline, std::iter::once("box".to_string()), 0.0, tx());
        assert!(ops.iter().any(|op| matches!(op, PreviewOverlayOp::Line { .. })));
        assert!(ops.iter().any(|op| matches!(op, PreviewOverlayOp::CircleFill { .. })));
        assert!(ops.iter().any(|op| matches!(op, PreviewOverlayOp::Text { .. })));
    }

    #[test]
    fn scene_bounds_grid_and_snap_lines_emit_expected_ops() {
        let t = tx();
        let bounds = scene_bounds_ops(&theme(), t, t.preview_rect);
        assert!(bounds.iter().any(|op| matches!(op, PreviewOverlayOp::RectStroke { .. })));

        let grid = grid_ops(&theme(), t, 20.0);
        assert!(grid.iter().any(|op| matches!(op, PreviewOverlayOp::Line { .. })));

        let snaps = snap_guide_ops(egui::Color32::RED, t.preview_rect, &[25.0], &[75.0], t);
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
