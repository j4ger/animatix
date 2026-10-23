use super::*;

pub(crate) fn render_action_blocks(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    actor_label: &str,
    track_idx: usize,
    bar_area: Rect,
    coords: &TimelineCoords,
    action_drag: Option<&ActionDragState>,
    new_action_drag: &mut Option<ActionDragState>,
    commands: &mut ActionQueue,
    theme: eparts::Theme,
) {
    let painter = ui.painter();
    for event in &timeline.action_events {
        if !event.targets.iter().any(|t| t == actor_label) {
            continue;
        }
        let start_s = event.start_time_ms as f64 / 1000.0;
        let end_s = (event.start_time_ms + event.duration_ms) as f64 / 1000.0;
        let duration_s = event.duration_ms as f64 / 1000.0;
        let left = coords.time_to_x(start_s);
        let right = coords.time_to_x(end_s);
        if right > bar_area.left() && left < bar_area.right() {
            let br = Rect::from_min_max(
                Pos2::new(left.max(bar_area.left()), bar_area.top() + 3.0),
                Pos2::new(right.min(bar_area.right()), bar_area.bottom() - 3.0),
            );
            let color = action_category_color(event.category, theme);

            // Check if this block is being dragged
            let is_action_drag = action_drag
                .is_some_and(|(ti, ms, _, _, _, _)| *ti == track_idx && *ms == event.start_time_ms);

            // Draw drag handles (left/right edges)
            let handle_w = 8.0;
            if br.width() > handle_w * 2.0 + 4.0 {
                let left_handle =
                    Rect::from_min_max(br.left_top(), Pos2::new(br.left() + handle_w, br.bottom()));
                let right_handle = Rect::from_min_max(
                    Pos2::new(br.right() - handle_w, br.top()),
                    br.right_bottom(),
                );
                let handle_color = if is_action_drag {
                    theme.palette.accent.primary
                } else {
                    color.linear_multiply(0.7)
                };
                painter.rect_filled(left_handle, RADIUS_S, handle_color);
                painter.rect_filled(right_handle, RADIUS_S, handle_color);
            }

            // Draw main block body
            painter.rect_filled(br, RADIUS_S, color.linear_multiply(0.5));
            painter.rect_stroke(
                br,
                RADIUS_S,
                Stroke::new(
                    if is_action_drag { 2.0 } else { STROKE_WIDTH },
                    if is_action_drag {
                        theme.palette.accent.primary
                    } else {
                        color
                    },
                ),
                egui::StrokeKind::Outside,
            );
            if br.width() > 40.0 {
                painter.text(
                    br.center(),
                    Align2::CENTER_CENTER,
                    &event.verb,
                    FontId::monospace(10.0), // 10px mono: no TextRole
                    theme.palette.text.primary,
                );
            }

            // Interaction: click_and_drag for resize
            let action_resp = ui.interact(
                br,
                ui.id().with(("action_block", track_idx, event.start_time_ms)),
                Sense::click_and_drag(),
            );

            // Drag start: detect which edge by proximity
            if action_resp.drag_started() {
                if let Some(pos) = action_resp.interact_pointer_pos() {
                    let dist_left = (pos.x - br.left()).abs();
                    let dist_right = (pos.x - br.right()).abs();
                    let edge = if dist_left <= dist_right {
                        Edge::Left
                    } else {
                        Edge::Right
                    };
                    *new_action_drag =
                        Some((track_idx, event.start_time_ms, edge, pos.x, start_s, duration_s));
                }
            }

            // During drag: visual feedback
            if is_action_drag {
                if let Some(&(_, _, edge, init_x, orig_start, orig_dur)) = action_drag {
                    if let Some(pos) = action_resp.interact_pointer_pos() {
                        let dx = pos.x - init_x;
                        let dt = (dx / (coords.bar_width / coords.visible_s as f32)) as f64;
                        match edge {
                            Edge::Right => {
                                let new_end_x = coords.time_to_x(orig_start + orig_dur + dt);
                                painter.line_segment(
                                    [
                                        Pos2::new(new_end_x, br.top()),
                                        Pos2::new(new_end_x, br.bottom()),
                                    ],
                                    Stroke::new(2.0, theme.palette.accent.primary),
                                );
                                let new_dur = (orig_dur + dt).max(0.1);
                                let dur_text = if new_dur < 1.0 {
                                    format!("{}ms", (new_dur * 1000.0).round())
                                } else {
                                    format!("{:.2}s", new_dur)
                                };
                                painter.text(
                                    Pos2::new(new_end_x, br.top() - 4.0),
                                    Align2::CENTER_BOTTOM,
                                    dur_text,
                                    FontId::monospace(10.0), // 10px mono: no TextRole
                                    theme.palette.accent.primary,
                                );
                            },
                            Edge::Left => {
                                let new_start_x = coords.time_to_x(orig_start + dt);
                                painter.line_segment(
                                    [
                                        Pos2::new(new_start_x, br.top()),
                                        Pos2::new(new_start_x, br.bottom()),
                                    ],
                                    Stroke::new(2.0, theme.palette.accent.primary),
                                );
                            },
                        }
                    }
                }
            }

            // Drag stop: emit ResizeAction command
            if action_resp.drag_stopped() {
                if let Some((_, _, edge, init_x, orig_start, orig_dur)) = *new_action_drag {
                    if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                        let dx = pos.x - init_x;
                        let dt = (dx / (coords.bar_width / coords.visible_s as f32)) as f64;
                        let (new_start_s, new_duration_s) = match edge {
                            Edge::Right => (orig_start, (orig_dur + dt).max(0.1)),
                            Edge::Left => {
                                let ns = (orig_start + dt).max(0.0);
                                (ns, (orig_dur - dt).max(0.1))
                            },
                        };
                        commands.push_back(ShellAction::Command(Command::ResizeAction {
                            verb: event.verb.clone(),
                            targets: event.targets.clone(),
                            old_start_s: orig_start,
                            new_start_s,
                            new_duration_s,
                        }));
                    }
                }
                *new_action_drag = None;
            }

            // Tooltip on hover (when not dragging)
            if !is_action_drag {
                text_tooltip(
                    ui,
                    action_resp.id.with("tooltip"),
                    &action_resp,
                    &format!(
                        "{:?}: {}\n{:.2}s → {:.2}s\nDrag edges to resize\nTargets: {}",
                        event.category,
                        event.verb,
                        start_s,
                        end_s,
                        event.targets.join(", ")
                    ),
                );
            }
        }
    }
}
