use super::*;

fn render_scene_blocks(
    painter: &egui::Painter,
    composition: &Composition,
    track_rect: Rect,
    coords: &TimelineCoords,
    theme: eparts::Theme,
    label_color: Color32,
    duration_s: f64,
    scene_keyframe_times: &HashMap<String, Vec<f64>>,
) {
    let palette = [
        timeline::track_block_1(),
        timeline::track_block_2(),
        timeline::track_block_3(),
        timeline::track_block_4(),
        timeline::track_block_5(),
    ];
    for (idx, sn) in composition.declaration_order.iter().enumerate() {
        let Some(scene) = composition.scenes.get(sn) else {
            continue;
        };
        let Some(start_s) = composition.scene_start_times.get(sn).copied() else {
            continue;
        };
        let end_s = (start_s + scene.duration_s).min(duration_s);
        if end_s <= start_s {
            continue;
        }
        let sr = Rect::from_min_max(
            Pos2::new(coords.time_to_x(start_s), track_rect.top()),
            Pos2::new(coords.time_to_x(end_s), track_rect.bottom()),
        );
        painter.rect_filled(sr, 2.0, palette[idx % palette.len()]);

        // ── Keyframe density strip ──
        if let Some(times) = scene_keyframe_times.get(sn) {
            let strip_y = sr.bottom() - 6.0;
            let strip_h = 4.0;
            // Semi-transparent background for the density strip
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(sr.left(), strip_y),
                    Pos2::new(sr.right(), strip_y + strip_h),
                ),
                0.0,
                Color32::BLACK.linear_multiply(0.3),
            );
            // Draw tiny vertical marks for each keyframe
            for &kf_time_s in times {
                let kf_x = coords.time_to_x(start_s + kf_time_s);
                if kf_x >= sr.left() && kf_x <= sr.right() {
                    painter.line_segment(
                        [Pos2::new(kf_x, strip_y), Pos2::new(kf_x, strip_y + strip_h)],
                        Stroke::new(1.0, theme.palette.accent.primary),
                    );
                }
            }
        }

        // ── Scene name label ──
        if sr.width() > 24.0 {
            painter.text(
                Pos2::new(sr.center().x, sr.center().y - 2.0),
                Align2::CENTER_CENTER,
                sn.as_str(),
                FontId::monospace(10.0), // 10px mono: no TextRole
                label_color,
            );
        }

        // ── Duration label (bottom-left of block) ──
        if sr.width() > 40.0 {
            let dur = scene.duration_s;
            let dur_text = if dur < 1.0 {
                format!("{}ms", (dur * 1000.0).round() as u64)
            } else {
                format!("{:.1}s", dur)
            };
            painter.text(
                Pos2::new(sr.left() + 4.0, sr.bottom() - 2.0),
                Align2::LEFT_BOTTOM,
                dur_text,
                FontId::monospace(10.0), // 10px mono: no TextRole
                label_color.linear_multiply(0.6),
            );
        }
    }
}

/// Render the composition scene track: scene blocks, transitions, drag reordering, and scrubbing.
pub(crate) fn render_scene_track(
    ui: &mut egui::Ui,
    comp: &Composition,
    st_top: f32,
    st_bot: f32,
    scroll_rect: Rect,
    coords: &TimelineCoords,
    duration_s: f64,
    scene_keyframe_times: &HashMap<String, Vec<f64>>,
    preview: &PreviewPaneState,
    commands: &mut ActionQueue,
    playhead_x: f32,
    panel_id: egui::Id,
    theme: eparts::Theme,
    sp: &Spatial,
) {
    let painter = ui.painter();
    let label_rect = Rect::from_min_max(
        Pos2::new(scroll_rect.left(), st_top),
        Pos2::new(coords.bar_origin_x, st_bot),
    );
    painter.rect_filled(label_rect, 0.0, theme.palette.surface.base);
    painter.text(
        Pos2::new(scroll_rect.left() + sp.base.space_2, (st_top + st_bot) / 2.0),
        Align2::LEFT_CENTER,
        format!("{} Scenes", egui_phosphor::regular::FILM_STRIP),
        TextRole::BodyS.font_id(),
        theme.palette.text.muted,
    );

    let bar_area = Rect::from_min_max(
        Pos2::new(coords.bar_origin_x, st_top),
        Pos2::new(scroll_rect.right(), st_bot),
    );

    // ── Scene block drag state ──
    let scene_drag_id = panel_id.with("scene_drag");
    let scene_drag_data_id = scene_drag_id.with("data");
    let scene_drag: Option<(String, f32)> = ui.data(|d| d.get_temp(scene_drag_data_id));
    let mut new_scene_drag: Option<(String, f32)> = scene_drag.clone();

    render_scene_blocks(
        painter,
        comp,
        bar_area,
        coords,
        theme,
        theme.palette.text.dim,
        duration_s,
        scene_keyframe_times,
    );

    // Draw drag ghost if dragging
    if let Some((ref drag_name, drag_offset_x)) = scene_drag {
        if let Some(scene) = comp.scenes.get(drag_name) {
            if let Some(start_s) = comp.scene_start_times.get(drag_name).copied() {
                let end_s = start_s + scene.duration_s;
                let block_w = coords.time_to_x(end_s) - coords.time_to_x(start_s);
                let ghost_rect = Rect::from_min_size(
                    Pos2::new(bar_area.left() + drag_offset_x, bar_area.top()),
                    Vec2::new(block_w, bar_area.height()),
                );
                painter.rect_filled(
                    ghost_rect,
                    2.0,
                    Color32::from_rgba_premultiplied(60, 130, 230, 80),
                );
                painter.rect_stroke(
                    ghost_rect,
                    2.0,
                    Stroke::new(1.5, theme.palette.accent.primary),
                    egui::StrokeKind::Outside,
                );
                if ghost_rect.width() > 24.0 {
                    painter.text(
                        ghost_rect.center(),
                        Align2::CENTER_CENTER,
                        drag_name.as_str(),
                        FontId::monospace(10.0), // 10px mono: no TextRole
                        theme.palette.accent.primary,
                    );
                }
            }
        }
    }

    for (src_name, edge) in &comp.edges {
        let Some(src_scene) = comp.scenes.get(src_name) else {
            continue;
        };
        let Some(src_start) = comp.scene_start_times.get(src_name).copied() else {
            continue;
        };
        let src_right = coords.time_to_x(src_start + src_scene.duration_s);
        let Some(tgt_start) = comp.scene_start_times.get(&edge.to_scene).copied() else {
            continue;
        };
        let tgt_left = coords.time_to_x(tgt_start);
        if tgt_left <= src_right {
            continue;
        }
        let cy = bar_area.center().y;
        // Draw edge arrow line
        painter.line_segment(
            [Pos2::new(src_right, cy), Pos2::new(tgt_left, cy)],
            Stroke::new(STROKE_WIDTH, theme.palette.text.muted),
        );
        painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(tgt_left, cy),
                Pos2::new(tgt_left - 4.0, cy - 2.5),
                Pos2::new(tgt_left - 4.0, cy + 2.5),
            ],
            theme.palette.text.muted,
            Stroke::NONE,
        ));

        // ── Transition badge at midpoint ──
        let mid_x = (src_right + tgt_left) / 2.0;
        let badge_r = 7.0;
        let badge_rect =
            Rect::from_center_size(Pos2::new(mid_x, cy), Vec2::new(badge_r * 2.0, badge_r * 2.0));

        // Determine transition icon
        let icon = match edge.transition.id.as_str() {
            "fade" => "F",
            "wipe-left" | "wipe-right" | "wipe-up" | "wipe-down" => "W",
            _ => "C",
        };

        // Badge background circle
        painter.circle_filled(Pos2::new(mid_x, cy), badge_r, theme.palette.surface.surface);
        painter.circle_stroke(
            Pos2::new(mid_x, cy),
            badge_r,
            Stroke::new(1.0, theme.palette.text.muted),
        );

        // Badge icon text
        painter.text(
            badge_rect.center(),
            Align2::CENTER_CENTER,
            icon,
            FontId::monospace(10.0), // 10px mono: no TextRole
            theme.palette.text.primary,
        );

        // Tooltip on hover
        let badge_resp = ui.interact(
            badge_rect,
            ui.id().with(("transition_badge", src_name, &edge.to_scene)),
            Sense::hover(),
        );
        let dur_s = edge.transition.duration_ms as f64 / 1000.0;
        let dur_text = if dur_s < 1.0 {
            format!("{}ms", edge.transition.duration_ms)
        } else {
            format!("{:.1}s", dur_s)
        };
        text_tooltip(
            ui,
            ui.id().with(("transition_tooltip", src_name, &edge.to_scene)),
            &badge_resp,
            &format!("{:?} → {} [{}]", edge.transition.id, edge.to_scene, dur_text),
        );
    }

    draw_loop_region(painter, bar_area.top(), bar_area.bottom(), preview, coords);

    // Scene track interaction (click to scrub, drag scene blocks to reorder)
    let scene_bar_resp =
        ui.interact(bar_area, ui.id().with("scene_track"), Sense::click_and_drag());
    if scene_bar_resp.drag_started() {
        if let Some(pos) = scene_bar_resp.interact_pointer_pos() {
            // Check if click is on a scene block
            for scene_name in &comp.declaration_order {
                if let Some(scene) = comp.scenes.get(scene_name) {
                    if let Some(start_s) = comp.scene_start_times.get(scene_name).copied() {
                        let end_s = start_s + scene.duration_s;
                        let block_x0 = coords.time_to_x(start_s);
                        let block_x1 = coords.time_to_x(end_s);
                        if pos.x >= block_x0 && pos.x <= block_x1 {
                            new_scene_drag = Some((scene_name.clone(), pos.x - bar_area.left()));
                            break;
                        }
                    }
                }
            }
        }
    }
    if scene_bar_resp.dragged() && scene_drag.is_some() {
        if let Some(pos) = scene_bar_resp.interact_pointer_pos() {
            if let Some((ref name, _)) = new_scene_drag {
                let block_w = comp
                    .scenes
                    .get(name)
                    .and_then(|s| {
                        comp.scene_start_times
                            .get(name)
                            .map(|st| coords.time_to_x(st + s.duration_s) - coords.time_to_x(*st))
                    })
                    .unwrap_or(40.0);
                new_scene_drag =
                    Some((name.clone(), (pos.x - bar_area.left() - block_w / 2.0).max(0.0)));
            }
        }
    }
    if scene_bar_resp.drag_stopped() {
        if let Some((ref drag_name, drop_offset_x)) = scene_drag {
            let block_w = comp
                .scenes
                .get(drag_name)
                .and_then(|s| {
                    comp.scene_start_times
                        .get(drag_name)
                        .map(|st| coords.time_to_x(st + s.duration_s) - coords.time_to_x(*st))
                })
                .unwrap_or(80.0);
            let drop_center_x = bar_area.left() + drop_offset_x + block_w / 2.0;
            let drop_time = coords.x_to_time(drop_center_x);

            let mut new_order = comp.declaration_order.clone();
            if let Some(drag_idx) = new_order.iter().position(|n| n == drag_name) {
                let _ = new_order.remove(drag_idx);
                let mut insert_idx = new_order.len();
                for (i, name) in new_order.iter().enumerate() {
                    if let Some(start_s) = comp.scene_start_times.get(name).copied() {
                        let mid_s = start_s
                            + comp.scenes.get(name).map(|s| s.duration_s / 2.0).unwrap_or(0.0);
                        if drop_time < mid_s {
                            insert_idx = i;
                            break;
                        }
                    }
                }
                new_order.insert(insert_idx, drag_name.clone());

                if new_order != comp.declaration_order {
                    commands.push_back(ShellAction::Command(Command::ReorderScenes(new_order)));
                }
            }
        }
        new_scene_drag = None;
    }

    // Persist scene drag state
    ui.data_mut(|d| d.insert_temp(scene_drag_data_id, new_scene_drag));

    painter.line_segment(
        [
            Pos2::new(playhead_x, bar_area.top() - 2.0),
            Pos2::new(playhead_x, bar_area.bottom() + 2.0),
        ],
        Stroke::new(1.5, theme.palette.text.primary),
    );
    painter.line_segment(
        [
            Pos2::new(scroll_rect.left(), st_bot),
            Pos2::new(scroll_rect.right(), st_bot),
        ],
        Stroke::new(STROKE_WIDTH, theme.palette.border.default),
    );
}
