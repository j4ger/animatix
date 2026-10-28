//! Preview panel body.

use super::drag::*;
use super::hud::*;
use super::*;

pub(super) fn draw_existing_guides(
    ctx: &PreviewContext<'_>,
    ui: &mut egui::Ui,
    preview_rect: egui::Rect,
    theme: &eparts::Theme,
) {
    if ctx.preview.overlay.show_guides {
        let guide_color = theme.palette.status.warning;
        for &guide_y in &ctx.preview.guides.horizontal_guides {
            let screen_pt =
                ctx.preview_scene_to_screen(preview_rect, kurbo::Point::new(0.0, guide_y as f64));
            if screen_pt.y >= preview_rect.min.y && screen_pt.y <= preview_rect.max.y {
                ui.painter().line_segment(
                    [
                        egui::pos2(preview_rect.min.x, screen_pt.y),
                        egui::pos2(preview_rect.max.x, screen_pt.y),
                    ],
                    egui::Stroke::new(STROKE_WIDTH, guide_color),
                );
            }
        }
        for &guide_x in &ctx.preview.guides.vertical_guides {
            let screen_pt =
                ctx.preview_scene_to_screen(preview_rect, kurbo::Point::new(guide_x as f64, 0.0));
            if screen_pt.x >= preview_rect.min.x && screen_pt.x <= preview_rect.max.x {
                ui.painter().line_segment(
                    [
                        egui::pos2(screen_pt.x, preview_rect.min.y),
                        egui::pos2(screen_pt.x, preview_rect.max.y),
                    ],
                    egui::Stroke::new(STROKE_WIDTH, guide_color),
                );
            }
        }
    }
}

pub(super) fn handle_scroll_zoom(
    ctx: &mut PreviewContext<'_>,
    ui: &mut egui::Ui,
    preview_rect: egui::Rect,
    response: &egui::Response,
) {
    // ── Scroll zoom ──
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta);
        if scroll.y != 0.0 {
            let zoom_factor = 1.0 + scroll.y * 0.001;
            let new_zoom = (ctx.preview.viewport.preview_zoom * zoom_factor)
                .clamp(preview_spatial::MIN_ZOOM, 10.0);
            let prev_zoom = ctx.preview.viewport.preview_zoom;
            if let Some(cursor) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                let cursor_in_rect = preview_rect.contains(cursor);
                if cursor_in_rect && prev_zoom > 0.01 {
                    let scene_at_cursor = ctx.preview_screen_to_scene(preview_rect, cursor);
                    let rel = cursor - preview_rect.center();
                    ctx.preview.viewport.preview_zoom = new_zoom;
                    let tx = preview::PreviewTransform::new(
                        ctx.scene_dimensions,
                        preview_rect,
                        new_zoom,
                        Vec2::ZERO,
                    );
                    let (new_scale, _) = tx.scale();
                    let new_pan = Vec2::new(
                        (scene_at_cursor.x - rel.x as f64 * new_scale) as f32,
                        (scene_at_cursor.y - rel.y as f64 * new_scale) as f32,
                    );
                    ctx.preview.viewport.preview_pan = ctx.clamp_pan(new_pan, preview_rect);
                    ctx.preview.status =
                        format!("Zoom: {:.0}%", ctx.preview.viewport.preview_zoom * 100.0);
                }
            } else {
                ctx.preview.viewport.preview_zoom = new_zoom;
                ctx.preview.status =
                    format!("Zoom: {:.0}%", ctx.preview.viewport.preview_zoom * 100.0);
            }
        }
    }
}

pub(super) fn handle_middle_click_pan(
    ctx: &mut PreviewContext<'_>,
    ui: &mut egui::Ui,
    preview_rect: egui::Rect,
) {
    // ── Middle-click pan ──
    if ui.input(|i| i.pointer.middle_down()) {
        if let Some(mouse) = ui.ctx().input(|i| i.pointer.latest_pos()) {
            if preview_rect.contains(mouse) {
                let delta = ui.input(|i| i.pointer.delta());
                if delta != Vec2::ZERO {
                    let tx = preview::PreviewTransform::new(
                        ctx.scene_dimensions,
                        preview_rect,
                        ctx.preview.viewport.preview_zoom,
                        Vec2::ZERO,
                    );
                    let (scale, _) = tx.scale();
                    let new_pan = Vec2::new(
                        ctx.preview.viewport.preview_pan.x - delta.x * scale as f32,
                        ctx.preview.viewport.preview_pan.y - delta.y * scale as f32,
                    );
                    ctx.preview.viewport.preview_pan = ctx.clamp_pan(new_pan, preview_rect);
                }
            }
        }
    }

    // Clear snap lines from previous frame
}

pub(super) fn clear_frame_snap_state(ctx: &mut PreviewContext<'_>) {
    ctx.preview.snap.snap_lines_h.clear();
    ctx.preview.snap.snap_lines_v.clear();
    ctx.preview.snap.snap_line_color = None;
    ctx.preview.snap.snap_hud_label = None;
}

pub(super) fn handle_canvas_interaction(
    ctx: &mut PreviewContext<'_>,
    ui: &mut egui::Ui,
    preview_rect: egui::Rect,
    response: &egui::Response,
    is_dragging: bool,
) {
    // ── Time Lens ──
    let wants_keyboard = ui.ctx().egui_wants_keyboard_input();
    let t_held =
        !wants_keyboard && ui.input(|i| i.key_pressed(egui::Key::T) || i.key_down(egui::Key::T));

    let all_kf = if t_held {
        let mut all_kf: Vec<f64> = if let Some(tl) = ctx.timeline {
            tl.world()
                .root_entities()
                .iter()
                .copied()
                .flat_map(|id| {
                    animatix::timeline::collect_all_keyframe_times(
                        id,
                        tl.world().effects(id),
                        tl.world(),
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        all_kf.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        all_kf.dedup_by(|a, b| (*a - *b).abs() < 0.001);
        all_kf
    } else {
        Vec::new()
    };
    if let Some(new_time) = ctx.preview.time_lens.update_and_show(
        ui,
        ctx.preview.playback.current_time_s(),
        ctx.preview.playback.duration_s,
        &all_kf,
    ) {
        ctx.commands.push_back(PlaybackCommand::ScrubTo(new_time).into());
    }

    crate::app::preview::gesture_router::GestureRouter::handle_preview_gestures(
        ctx,
        ui,
        preview_rect,
        &response,
    );

    let pointer_pos =
        ui.ctx().input(|i| i.pointer.latest_pos()).filter(|p| preview_rect.contains(*p));
    let scene_dimensions = ctx.scene_dimensions;
    let zoom = ctx.preview.viewport.preview_zoom;
    let pan = ctx.preview.viewport.preview_pan;
    let screen_to_scene = {
        let tx = preview::PreviewTransform::new(scene_dimensions, preview_rect, zoom, pan);
        move |screen: egui::Pos2| tx.screen_to_scene(screen)
    };

    if !ctx.selection.context_menu_open {
        let unlocked_hit_regions: Vec<(String, kurbo::Rect)> = ctx
            .hit_regions
            .iter()
            .filter(|(label, _)| !ctx.timeline.is_some_and(|t| t.is_actor_locked(label)))
            .cloned()
            .collect();
        selection::update_hover(
            ctx.selection,
            &unlocked_hit_regions,
            pointer_pos,
            screen_to_scene,
            is_dragging,
        );
    } else {
        ctx.selection.hovered_actor = None;
    }

    ctx.handle_preview_selection(ui, preview_rect, &response);
    ctx.render_preview_cursor_feedback(ui, preview_rect);
    ctx.render_preview_overlays(ui, preview_rect);
}

pub(super) fn handle_presentation_input(
    ctx: &mut PreviewContext<'_>,
    ui: &mut egui::Ui,
    preview_rect: egui::Rect,
    response: &egui::Response,
) {
    if response.clicked() {
        ctx.commands.push_back(PlaybackCommand::StepNext.into());
    } else if response.secondary_clicked() {
        ctx.commands.push_back(PlaybackCommand::StepPrevious.into());
    }
    presentation_hud_ui(ctx, ui, preview_rect);
}

pub(super) fn handle_file_drop(
    ctx: &mut PreviewContext<'_>,
    ui: &mut egui::Ui,
    preview_rect: egui::Rect,
    response: &egui::Response,
) {
    // ── File drop ──
    if response.hovered() {
        let dropped_files = ui.input(|i| i.raw.dropped_files.clone());
        for file in dropped_files {
            if let Some(path) = file.path {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                let path_str = path.to_string_lossy().to_string();

                // .amx files: open directly instead of creating an actor
                if ext == "amx" {
                    ctx.commands.push_back(DocumentCommand::OpenFile(path).into());
                    continue;
                }

                let drop_pos = if let Some(mouse) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                    let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
                    [scene.x as f32, scene.y as f32]
                } else {
                    [
                        ctx.scene_dimensions.width as f32 / 2.0,
                        ctx.scene_dimensions.height as f32 / 2.0,
                    ]
                };
                let label = crate::app::utils::labels::unique_label(
                    None,
                    if ext == "svg" { "svg" } else { "image" },
                );
                let (ty, props) = if ext == "svg" {
                    (
                        "Svg".to_string(),
                        vec![animatix_syntax::ast::Property {
                            name: "url".into(),
                            value: animatix_syntax::ast::Expr::Str(path_str),
                            value_span: None,
                            trailing_comment: None,
                        }],
                    )
                } else {
                    (
                        "Image".to_string(),
                        vec![animatix_syntax::ast::Property {
                            name: "url".into(),
                            value: animatix_syntax::ast::Expr::Str(path_str),
                            value_span: None,
                            trailing_comment: None,
                        }],
                    )
                };
                ctx.commands.push_back(
                    ActorCommand::CreateActor {
                        ty,
                        label,
                        position: drop_pos,
                        props,
                    }
                    .into(),
                );
            }
        }
    }
}

pub(super) fn show_selection_editors(
    ctx: &mut PreviewContext<'_>,
    ui: &mut egui::Ui,
    preview_rect: egui::Rect,
    is_dragging: bool,
) {
    // ── Library drag-to-place ──
    // A Library/Asset row sets a payload while it is dragged; we own
    // the scene transform, so the drop point is resolved here.
    library_drag_drop_ui(ctx, ui, preview_rect);

    // Inline text editor (double-click on text actors)
    ctx.render_inline_text_editor(ui, preview_rect);

    // Floating property cards for selected actors (hide when inline editing)
    if !is_dragging && ctx.selected_actors.len() == 1 && ctx.preview.inline_edit.is_none() {
        if let Some(actor) = ctx.selected_actors.iter().next() {
            if let Some(props) = ctx.get_actor_props(actor) {
                let screen_pos = ctx.preview_scene_to_screen(
                    preview_rect,
                    kurbo::Point::new(props.position[0] as f64, props.position[1] as f64),
                );
                preview::property_popup::show_property_popup(
                    ui,
                    actor,
                    &props,
                    screen_pos,
                    ctx.commands,
                    is_dragging,
                    ctx.timeline,
                    ctx.preview.playback.current_time_s(),
                    ctx.scene_dimensions,
                    ctx.preview.viewport.preview_zoom,
                    preview_rect,
                    ctx.preview.viewport.preview_pan,
                    ctx.active_scene,
                );
            }
        }
    }

    // Render the Floating Tool Island HUD hovering over the canvas
    preview_floating_hud_ui(ctx, ui, preview_rect);
}
