//! Handle feedback and the interactive selection overlay.

use animatix::timeline::{EntityId, Timeline};
use egui::Pos2;

use crate::app::design_tokens::spatial::STROKE_WIDTH;
use crate::app::design_tokens::spatial::preview::HANDLE_HIT_RADIUS as PREVIEW_HANDLE_HIT_RADIUS;
use crate::app::design_tokens::typography::TextRole;
use crate::app::preview::overlay_ops::{
    execute_overlay_ops, ghost_overlay_ops, multi_selection_overlay_ops, reorder_overlay_ops,
    selection_overlay_ops,
};
use crate::app::preview::{self, ActorProps, DragState};
use crate::app::primitives::GizmoAffordance;

use super::PreviewContext;

impl PreviewContext<'_> {
    pub(crate) fn render_preview_cursor_feedback(&self, ui: &egui::Ui, preview_rect: egui::Rect) {
        let is_dragging = !matches!(self.drag_state, DragState::None);
        let raw_pointer_pos = ui.ctx().input(|i| i.pointer.latest_pos());
        let hit_radius = PREVIEW_HANDLE_HIT_RADIUS;

        if !is_dragging && !self.selection.context_menu_open {
            if let Some(mouse) = raw_pointer_pos {
                let scene = self.preview_screen_to_scene(preview_rect, mouse);

                let over_handle = self.selected_actors.iter().next().and_then(|a| {
                    let props = self.get_actor_props(a)?;
                    let pivot_world_pt = preview::pivot_world(&props);
                    let pivot_screen = self.preview_scene_to_screen(
                        preview_rect,
                        kurbo::Point::new(pivot_world_pt[0] as f64, pivot_world_pt[1] as f64),
                    );
                    if preview::hit_test_pivot(mouse, pivot_screen, hit_radius) {
                        return Some(9usize);
                    }
                    let handle_world = preview::world_handle_positions(&props);
                    let handle_screen: [Pos2; 8] = std::array::from_fn(|i| {
                        self.preview_scene_to_screen(preview_rect, handle_world[i])
                    });
                    if let Some(idx) = preview::hit_test_handle(mouse, &handle_screen, hit_radius) {
                        Some(idx)
                    } else {
                        let rot_world = preview::rotation_handle_world(&props);
                        let rot_screen = self.preview_scene_to_screen(preview_rect, rot_world);
                        if preview::hit_test_rotation_handle(mouse, rot_screen, hit_radius) {
                            Some(8usize)
                        } else {
                            None
                        }
                    }
                });

                if let Some(handle_idx) = over_handle {
                    let (icon, tooltip) = match handle_idx {
                        0 => (egui::CursorIcon::ResizeNwSe, "Scale from top-left"),
                        1 => (egui::CursorIcon::ResizeNeSw, "Scale from top-right"),
                        2 => (egui::CursorIcon::ResizeNwSe, "Scale from bottom-right"),
                        3 => (egui::CursorIcon::ResizeNeSw, "Scale from bottom-left"),
                        4 => (egui::CursorIcon::ResizeVertical, "Scale height"),
                        5 => (egui::CursorIcon::ResizeHorizontal, "Scale width"),
                        6 => (egui::CursorIcon::ResizeVertical, "Scale height"),
                        7 => (egui::CursorIcon::ResizeHorizontal, "Scale width"),
                        8 => (egui::CursorIcon::Crosshair, "Rotate"),
                        9 => (egui::CursorIcon::Move, "Move pivot"),
                        _ => (egui::CursorIcon::Default, ""),
                    };
                    ui.ctx().set_cursor_icon(icon);
                    egui::Tooltip::always_open(
                        ui.ctx().clone(),
                        ui.layer_id(),
                        egui::Id::new("handle_tooltip"),
                        egui::PopupAnchor::Pointer,
                    )
                    .show(|ui| {
                        ui.label(
                            egui::RichText::new(tooltip).size(
                                crate::app::design_tokens::typography::TextRole::BodyS.size(),
                            ),
                        );
                    });
                } else {
                    let is_over_selected = self
                        .selected_actors
                        .iter()
                        .next()
                        .and_then(|a| {
                            self.hit_regions
                                .iter()
                                .find(|(l, _)| l == a)
                                .map(|(_, b)| b.contains(scene))
                        })
                        .unwrap_or(false);
                    if is_over_selected {
                        let cursor = match *self.tool_mode {
                            preview::ToolMode::Move => egui::CursorIcon::Grab,
                            preview::ToolMode::Scale => egui::CursorIcon::ResizeNwSe,
                            preview::ToolMode::Rotate => egui::CursorIcon::Crosshair,
                            preview::ToolMode::Vertex => egui::CursorIcon::Crosshair,
                            preview::ToolMode::Pivot => egui::CursorIcon::Move,
                            preview::ToolMode::Select => egui::CursorIcon::Grab,
                        };
                        ui.ctx().set_cursor_icon(cursor);
                    } else if self.selection.hovered_actor.is_some() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                }
            }
        } else if !self.selection.context_menu_open {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
    }

    pub(crate) fn render_preview_selection_overlay(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
        is_dragging: bool,
    ) {
        let theme = eparts::theme(ui);

        if self.selected_actors.len() > 1 {
            self.render_multi_selection_overlay(ui, preview_rect, is_dragging, theme);
            return;
        }

        let show_pivot = *self.tool_mode == crate::app::preview::ToolMode::Pivot
            || matches!(self.drag_state, DragState::MovePivot { .. });
        for actor in self.selected_actors.iter() {
            let Some(timeline) = self.timeline else {
                break;
            };
            let Some(id) = timeline.get_entity_id(actor) else {
                continue;
            };
            let affordances = crate::app::primitives::actor_gizmo_affordances(id, timeline);
            let props = self.get_actor_props(actor);

            self.render_actor_selection_box(
                ui,
                preview_rect,
                actor,
                props.as_ref(),
                is_dragging,
                show_pivot,
                theme,
            );

            let time_ms = (self.preview.playback.current_time_s() * 1000.0) as u64;
            self.render_gizmo_handles(
                ui,
                preview_rect,
                actor,
                id,
                &affordances,
                props.as_ref(),
                time_ms,
                timeline,
                theme,
            );

            if is_dragging {
                self.render_drag_measurement(ui, preview_rect, theme);
            } else {
                self.render_ghost_overlay(ui, preview_rect, actor);
            }

            self.render_reorder_overlay(ui, preview_rect, actor, props.as_ref(), theme);
        }

        self.render_marquee(ui, theme);
    }

    /// Union box for a multi-selection; single actors fall through to handles.
    fn render_multi_selection_overlay(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
        is_dragging: bool,
        theme: eparts::Theme,
    ) {
        let Some(union) = self.selected_union_rect() else {
            return;
        };
        let tx = self.preview_transform(preview_rect);
        let ops = multi_selection_overlay_ops(
            &theme,
            &[union],
            is_dragging,
            ui.ctx().pixels_per_point(),
            tx,
        );
        execute_overlay_ops(ui.painter(), &ops, &tx);
    }

    /// Outline/pivot box for one selected actor.
    fn render_actor_selection_box(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
        actor: &str,
        props: Option<&ActorProps>,
        is_dragging: bool,
        show_pivot: bool,
        theme: eparts::Theme,
    ) {
        let fallback = self
            .hit_regions
            .iter()
            .find(|(l, _)| l.as_str() == actor)
            .map(|(_, bounds)| *bounds);
        let tx = self.preview_transform(preview_rect);
        let ops = selection_overlay_ops(
            &theme,
            props,
            fallback,
            is_dragging,
            show_pivot,
            ui.ctx().pixels_per_point(),
            tx,
        );
        execute_overlay_ops(ui.painter(), &ops, &tx);
    }

    /// Interactive gizmo handles (vertices, callout tip/label/place/standoff).
    fn render_gizmo_handles(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
        actor: &str,
        id: EntityId,
        affordances: &[GizmoAffordance],
        props: Option<&ActorProps>,
        time_ms: u64,
        timeline: &Timeline,
        theme: eparts::Theme,
    ) {
        if affordances.contains(&GizmoAffordance::ControlPoints) {
            self.render_vertex_handles(
                ui,
                preview_rect,
                actor,
                id,
                props,
                time_ms,
                timeline,
                theme,
            );
        }

        if affordances.contains(&GizmoAffordance::Callout) {
            self.render_callout_handles(ui, preview_rect, actor, id, time_ms, timeline, theme);
        }
    }

    fn render_vertex_handles(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
        actor: &str,
        id: EntityId,
        props: Option<&ActorProps>,
        time_ms: u64,
        timeline: &Timeline,
        theme: eparts::Theme,
    ) {
        let points = timeline
            .world()
            .shape(id)
            .and_then(|s| s.points.as_ref())
            .map(|pt| pt.evaluate(time_ms))
            .filter(|pts| !pts.is_empty());
        let (Some(props), Some(pts)) = (props, points) else {
            return;
        };

        let active_vertex = match &self.drag_state {
            DragState::EditVertices {
                actor: drag_actor,
                vertex,
                ..
            } if drag_actor.as_str() == actor => Some(*vertex),
            _ => None,
        };
        preview::draw_vertex_handles(
            ui.painter(),
            theme,
            props,
            &pts,
            preview_rect,
            self.scene_dimensions,
            active_vertex,
            ui.ctx().pixels_per_point(),
            self.preview.viewport.preview_zoom,
            self.preview.viewport.preview_pan,
        );
    }

    /// Callout handles: tip + label + place (targeted) + standoff (targeted).
    fn render_callout_handles(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
        actor: &str,
        id: EntityId,
        time_ms: u64,
        timeline: &Timeline,
        theme: eparts::Theme,
    ) {
        let geo = animatix::timeline::callout_geometry::derive_callout_geometry(
            timeline.world().geometry(id),
            timeline.world().shape_or_default(id),
            time_ms,
            Some(timeline),
            self.scene_dimensions,
        );
        let zoom = self.preview.viewport.preview_zoom;
        let pan = self.preview.viewport.preview_pan;
        let tip_world = kurbo::Point::new(geo.to[0] as f64, geo.to[1] as f64);
        let label_world = kurbo::Point::new(geo.label_point[0] as f64, geo.label_point[1] as f64);
        let tip_screen = self.preview_scene_to_screen(preview_rect, tip_world);
        let label_screen = self.preview_scene_to_screen(preview_rect, label_world);
        let active_tip = matches!(&self.drag_state, DragState::CalloutTip { actor: a, .. } if a.as_str() == actor);
        let active_label = matches!(&self.drag_state, DragState::CalloutLabel { actor: a, .. } if a.as_str() == actor);
        preview::draw_callout_handles(
            ui.painter(),
            theme,
            tip_screen,
            label_screen,
            active_tip,
            active_label,
            ui.ctx().pixels_per_point(),
        );

        if !geo.is_targeted {
            return;
        }

        let place_screens = preview::callout_place_handle_screens(
            &geo,
            preview_rect,
            self.scene_dimensions,
            zoom,
            pan,
        );
        // Individual place handles are never highlighted yet: intended to
        // highlight on tap, but not during drag. TODO: highlight the tapped
        // place once tap selection lands.
        let active_place = if self.selection.tapped_place_actor.as_deref() == Some(actor) {
            self.selection.tapped_place
        } else {
            None
        };
        preview::draw_callout_place_handles(
            ui.painter(),
            theme,
            place_screens,
            active_place,
            ui.ctx().pixels_per_point(),
        );

        // Standoff handle at `from`
        let from_world = kurbo::Point::new(geo.from[0] as f64, geo.from[1] as f64);
        let standoff_screen = self.preview_scene_to_screen(preview_rect, from_world);
        let active_standoff = matches!(&self.drag_state, DragState::CalloutStandoff { actor: a, .. } if a.as_str() == actor);
        preview::draw_callout_standoff_handle(
            ui.painter(),
            theme,
            standoff_screen,
            active_standoff,
            ui.ctx().pixels_per_point(),
        );
    }

    /// Delta measurements shown while moving, scaling, or rotating.
    fn render_drag_measurement(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
        theme: eparts::Theme,
    ) {
        let measurement_color = theme.palette.accent.primary;
        let text_color = theme.palette.text.primary;
        let font = egui::FontId::monospace(TextRole::Micro.size());

        match &self.drag_state {
            DragState::Move {
                primary,
                start_scene,
                ..
            } => self.render_move_measurement(
                ui,
                preview_rect,
                primary,
                start_scene,
                &font,
                measurement_color,
                text_color,
            ),
            DragState::Scale {
                actor, start_size, ..
            } => self.render_scale_measurement(
                ui,
                preview_rect,
                actor,
                start_size,
                &font,
                text_color,
            ),
            DragState::Rotate {
                actor,
                start_rotation,
                ..
            } => self.render_rotate_measurement(
                ui,
                preview_rect,
                actor,
                *start_rotation,
                &font,
                text_color,
            ),
            _ => {},
        }
    }

    fn render_move_measurement(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
        primary: &str,
        start_scene: &kurbo::Point,
        font: &egui::FontId,
        measurement_color: egui::Color32,
        text_color: egui::Color32,
    ) {
        let Some(props) = self.get_actor_props(primary) else {
            return;
        };

        let start_screen = self
            .preview_scene_to_screen(preview_rect, kurbo::Point::new(start_scene.x, start_scene.y));
        let current_screen = self.preview_scene_to_screen(
            preview_rect,
            kurbo::Point::new(props.position[0] as f64, props.position[1] as f64),
        );
        let y = (start_screen.y + current_screen.y) / 2.0;
        ui.painter().line_segment(
            [
                Pos2::new(start_screen.x.min(current_screen.x), y),
                Pos2::new(start_screen.x.max(current_screen.x), y),
            ],
            egui::Stroke::new(STROKE_WIDTH, measurement_color),
        );
        ui.painter().text(
            Pos2::new((start_screen.x + current_screen.x) / 2.0, y - 8.0),
            egui::Align2::CENTER_BOTTOM,
            format!("Δx: {:+.0}", props.position[0] - start_scene.x as f32),
            font.clone(),
            text_color,
        );
        let x = (start_screen.x + current_screen.x) / 2.0;
        ui.painter().line_segment(
            [
                Pos2::new(x, start_screen.y.min(current_screen.y)),
                Pos2::new(x, start_screen.y.max(current_screen.y)),
            ],
            egui::Stroke::new(STROKE_WIDTH, measurement_color),
        );
        ui.painter().text(
            Pos2::new(x + 4.0, (start_screen.y + current_screen.y) / 2.0),
            egui::Align2::LEFT_CENTER,
            format!("Δy: {:+.0}", props.position[1] - start_scene.y as f32),
            font.clone(),
            text_color,
        );
    }

    fn render_scale_measurement(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
        actor: &str,
        start_size: &[f32; 2],
        font: &egui::FontId,
        text_color: egui::Color32,
    ) {
        let Some(props) = self.get_actor_props(actor) else {
            return;
        };

        let screen_pos = self.preview_scene_to_screen(
            preview_rect,
            kurbo::Point::new(props.position[0] as f64, props.position[1] as f64),
        );
        let br = self.preview_scene_to_screen(
            preview_rect,
            kurbo::Point::new(
                props.position[0] as f64 + props.size[0] as f64 / 2.0,
                props.position[1] as f64 + props.size[1] as f64 / 2.0,
            ),
        );
        ui.painter().text(
            Pos2::new(screen_pos.x, br.y + 12.0),
            egui::Align2::CENTER_TOP,
            format!("w: {:.0} → {:.0}", start_size[0], props.size[0]),
            font.clone(),
            text_color,
        );
        ui.painter().text(
            Pos2::new(br.x + 4.0, screen_pos.y),
            egui::Align2::LEFT_CENTER,
            format!("h: {:.0} → {:.0}", start_size[1], props.size[1]),
            font.clone(),
            text_color,
        );
    }

    fn render_rotate_measurement(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
        actor: &str,
        start_rotation: f32,
        font: &egui::FontId,
        text_color: egui::Color32,
    ) {
        let Some(props) = self.get_actor_props(actor) else {
            return;
        };

        let screen_pos = self.preview_scene_to_screen(
            preview_rect,
            kurbo::Point::new(props.position[0] as f64, props.position[1] as f64),
        );
        ui.painter().text(
            Pos2::new(screen_pos.x, screen_pos.y - props.size[1] / 2.0 - 16.0),
            egui::Align2::CENTER_BOTTOM,
            format!("{:.0}° → {:.0}°", start_rotation.to_degrees(), props.rotation.to_degrees()),
            font.clone(),
            text_color,
        );
    }

    /// Ghost outlines of the previous/next keyframe pose while idle.
    fn render_ghost_overlay(&self, ui: &mut egui::Ui, preview_rect: egui::Rect, actor: &str) {
        let Some(timeline) = self.timeline else {
            return;
        };

        let current_time_ms = (self.preview.playback.current_time_s() * 1000.0) as u64;
        let keyframe_times = timeline.keyframe_times_s();
        let mut prev_time_ms: Option<u64> = None;
        let mut next_time_ms: Option<u64> = None;
        for &time_s in &keyframe_times {
            let time_ms = (time_s * 1000.0) as u64;
            if time_ms < current_time_ms {
                prev_time_ms = Some(time_ms);
            } else if time_ms > current_time_ms && next_time_ms.is_none() {
                next_time_ms = Some(time_ms);
            }
        }

        let tx = self.preview_transform(preview_rect);
        let mut ghost_ops = Vec::new();
        if let Some(prev_ms) = prev_time_ms {
            if let Some(prev_props) = self.get_actor_props_at_time(actor, prev_ms) {
                ghost_ops.extend(ghost_overlay_ops(
                    &prev_props,
                    crate::app::design_tokens::semantic::canvas::ghost_prev(),
                ));
            }
        }
        if let Some(next_ms) = next_time_ms {
            if let Some(next_props) = self.get_actor_props_at_time(actor, next_ms) {
                ghost_ops.extend(ghost_overlay_ops(
                    &next_props,
                    crate::app::design_tokens::semantic::canvas::ghost_next(),
                ));
            }
        }
        execute_overlay_ops(ui.painter(), &ghost_ops, &tx);
    }

    /// Drop-target outline while reordering a layout-managed child.
    fn render_reorder_overlay(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
        actor: &str,
        props: Option<&ActorProps>,
        theme: eparts::Theme,
    ) {
        let DragState::Reorder {
            actor: drag_actor,
            container,
            target_index,
            layout_type,
            ..
        } = &self.drag_state
        else {
            return;
        };
        if drag_actor.as_str() != actor {
            return;
        }
        let Some(timeline) = self.timeline else {
            return;
        };

        let time_ms = (self.preview.playback.current_time_s() * 1000.0) as u64;
        let order = timeline.get_child_order(container, time_ms);
        let siblings: Vec<(String, [f32; 2])> = order
            .into_iter()
            .filter(|label| label.as_str() != actor)
            .filter_map(|label| self.get_actor_props(&label).map(|p| (label, p.position)))
            .collect();

        let Some(props) = props else {
            return;
        };
        let tx = self.preview_transform(preview_rect);
        let ops = reorder_overlay_ops(
            &theme,
            props,
            *target_index,
            &siblings,
            tx,
            *layout_type == animatix::timeline::LayoutType::Row,
        );
        execute_overlay_ops(ui.painter(), &ops, &tx);
    }

    /// Rubber-band rectangle for an in-progress marquee selection.
    fn render_marquee(&self, ui: &egui::Ui, theme: eparts::Theme) {
        let (Some(start), Some(current)) =
            (self.selection.marquee_start, self.selection.marquee_current)
        else {
            return;
        };
        let marquee_rect = egui::Rect::from_two_pos(start, current);
        ui.painter().rect_filled(marquee_rect, 0.0, theme.palette.accent.faint);
        ui.painter().rect_stroke(
            marquee_rect,
            0.0,
            egui::Stroke::new(STROKE_WIDTH, theme.palette.accent.subtle),
            egui::StrokeKind::Outside,
        );
    }
}
