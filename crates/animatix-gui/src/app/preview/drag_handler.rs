//! Preview canvas drag interaction handler.

use egui::Pos2;

use crate::app::commands::{
    DocumentCommand, DragEvent, PropertyEdit, PropertyValue, ShellAction,
};
use crate::app::design_tokens::spatial::preview::{
    HANDLE_HIT_RADIUS as PREVIEW_HANDLE_HIT_RADIUS,
};
use crate::app::preview::context::PreviewContext;
use crate::app::preview::{self, DragState, drag_utils};
use animatix::timeline::TrackAccessor;

pub(crate) fn handle_preview_drag(
    ctx: &mut PreviewContext<'_>,
    ui: &mut egui::Ui,
    preview_rect: egui::Rect,
    response: &egui::Response,
) -> bool {
    let is_dragging = !matches!(ctx.drag_state, DragState::None);
    let raw_pointer_pos = ui.ctx().input(|i| i.pointer.latest_pos());

    if ui.input(|i| i.pointer.middle_down()) {
        return false;
    }

    let drag_started =
        response.drag_started() || (!is_dragging && ui.input(|i| i.pointer.primary_pressed()));
    let hit_radius = PREVIEW_HANDLE_HIT_RADIUS;

    if drag_started {
        if let (Some(actor), Some(mouse)) =
            (ctx.selected_actors.iter().next().cloned(), raw_pointer_pos)
        {
            let is_locked = ctx
                .timeline
                .and_then(|t| t.get_track(&actor))
                .map(|tr| tr.locked)
                .unwrap_or(false);
            if is_locked {
                return false;
            }
            let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
            let props = ctx.get_actor_props(&actor);

            if let Some(ref p) = props {
                let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;

                match *ctx.tool_mode {
                    preview::ToolMode::Move => {},
                    preview::ToolMode::Vertex => {},


                    preview::ToolMode::Scale => {},
                preview::ToolMode::Rotate => {},
                    preview::ToolMode::Pivot => {
                        let pivot_world_pt = preview::pivot_world(p);
                        let pivot_screen = ctx.preview_scene_to_screen(
                            preview_rect,
                            kurbo::Point::new(pivot_world_pt[0] as f64, pivot_world_pt[1] as f64),
                        );
                        if preview::hit_test_pivot(mouse, pivot_screen, hit_radius) {
                            *ctx.drag_state = DragState::MovePivot {
                                actor,
                                start_offset: p.pivot_offset,
                                start_scene: scene,
                            };
                            return true;
                        }
                    },
                    preview::ToolMode::Select => {
                        let pivot_world_pt = preview::pivot_world(p);
                        let pivot_screen = ctx.preview_scene_to_screen(
                            preview_rect,
                            kurbo::Point::new(pivot_world_pt[0] as f64, pivot_world_pt[1] as f64),
                        );
                        if preview::hit_test_pivot(mouse, pivot_screen, hit_radius) {
                            *ctx.drag_state = DragState::MovePivot {
                                actor: actor.clone(),
                                start_offset: p.pivot_offset,
                                start_scene: scene,
                            };
                            return true;
                        }
                    },
                }
            }

        } else if let Some(mouse) = raw_pointer_pos {
            ctx.selection.marquee_start = Some(mouse);
            ctx.selection.marquee_current = Some(mouse);
        }
    }

    if is_dragging {
        if let Some(mouse) = raw_pointer_pos {
            let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
            let shift = ui.input(|i| i.modifiers.shift);

            match ctx.drag_state.clone() {
                DragState::Reorder {
                    actor,
                    container,
                    source_index: _,
                    target_index: _,
                    layout_type,
                } => {
                    let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;
                    if let Some(timeline) = ctx.timeline {
                        let order = timeline.get_child_order(&container, time_ms);
                        let siblings: Vec<String> =
                            order.into_iter().filter(|l| l != &actor).collect();
                        let positions: Vec<f32> = siblings
                            .iter()
                            .map(|label| {
                                ctx.hit_regions
                                    .iter()
                                    .find(|(l, _)| l == label)
                                    .map(|(_, bounds)| {
                                        if layout_type == animatix::timeline::LayoutType::Row {
                                            (bounds.x0 + bounds.x1) as f32 / 2.0
                                        } else {
                                            (bounds.y0 + bounds.y1) as f32 / 2.0
                                        }
                                    })
                                    .or_else(|| {
                                        ctx.get_actor_props(label).map(|p| {
                                            if layout_type == animatix::timeline::LayoutType::Row {
                                                p.position[0]
                                            } else {
                                                p.position[1]
                                            }
                                        })
                                    })
                                    .unwrap_or(
                                        if layout_type == animatix::timeline::LayoutType::Row {
                                            scene.x as f32
                                        } else {
                                            scene.y as f32
                                        },
                                    )
                            })
                            .collect();

                        let mouse_coord = if layout_type == animatix::timeline::LayoutType::Row {
                            scene.x as f32
                        } else {
                            scene.y as f32
                        };
                        let mut insert_at = positions.len();
                        for (idx, coord) in positions.iter().enumerate() {
                            if mouse_coord < *coord {
                                insert_at = idx;
                                break;
                            }
                        }
                        if let DragState::Reorder { target_index, .. } = &mut *ctx.drag_state {
                            *target_index = insert_at;
                        }
                    }
                },

                DragState::MovePivot {
                    actor,
                    start_offset,
                    start_scene,
                } => {
                    let dx = (scene.x - start_scene.x) as f32;
                    let dy = (scene.y - start_scene.y) as f32;
                    if let Some(p) = ctx.get_actor_props(&actor) {
                        let cos = (-p.rotation).cos();
                        let sin = (-p.rotation).sin();
                        let local_dx = dx * cos - dy * sin;
                        let local_dy = dx * sin + dy * cos;
                        ctx.pivot_offsets.insert(
                            actor,
                            [start_offset[0] + local_dx, start_offset[1] + local_dy],
                        );
                    }
                },
                DragState::Move { .. } => {},
                DragState::Scale { .. } => {},
                DragState::Rotate { .. } => {},
                DragState::MotionPath { .. } => {},
                DragState::EditVertices { .. } => {},
                DragState::None => {},
            }
        }
    }

    let pointer_released = ui.input(|i| i.pointer.any_released());
    if is_dragging
        && (response.drag_stopped() || pointer_released || !ui.input(|i| i.pointer.any_down()))
    {
        let old_drag_state = ctx.drag_state.clone();
        drag_utils::finalize_drag_keyframes(&old_drag_state, ctx);

        if let DragState::Reorder {
            actor,
            container,
            source_index,
            target_index,
            ..
        } = ctx.drag_state.clone()
        {
            if source_index != target_index {
                let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;
                if let Some(timeline) = ctx.timeline {
                    let mut new_order = timeline.get_child_order(&container, time_ms);
                    if let Some(pos) = new_order.iter().position(|label| label == &actor) {
                        let item = new_order.remove(pos);
                        let insert_at = target_index.min(new_order.len());
                        new_order.insert(insert_at, item);
                        ctx.commands.push_back(
                            DocumentCommand::PropertyEdit(PropertyEdit {
                                time_s: None,
                                actor: container,
                                property: "child_order".into(),
                                value: PropertyValue::StringList(new_order),
                                create_keyframe: ctx.keyframe_mode,
                            })
                            .into(),
                        );
                    }
                }
            }
        }
        ctx.commands.push_back(ShellAction::Drag(DragEvent::DragEnded));
    }
    false
}
