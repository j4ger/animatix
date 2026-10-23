use super::*;

pub(crate) fn render_speech_track(
    ui: &mut egui::Ui,
    timeline: Option<&Timeline>,
    sp_top: f32,
    sp_bot: f32,
    scroll_rect: Rect,
    coords: &TimelineCoords,
    preview: &PreviewPaneState,
    commands: &mut ActionQueue,
    playhead_x: f32,
    panel_id: egui::Id,
    theme: eparts::Theme,
    sp: &Spatial,
) {
    let painter = ui.painter();
    let label_rect = Rect::from_min_max(
        Pos2::new(scroll_rect.left(), sp_top),
        Pos2::new(coords.bar_origin_x, sp_bot),
    );
    painter.rect_filled(label_rect, 0.0, theme.palette.surface.base);
    painter.text(
        Pos2::new(scroll_rect.left() + sp.base.space_2, (sp_top + sp_bot) / 2.0),
        Align2::LEFT_CENTER,
        format!("{} Speech", egui_phosphor::regular::TEXT_ALIGN_CENTER),
        TextRole::BodyS.font_id(),
        theme.palette.text.muted,
    );

    let bar_area = Rect::from_min_max(
        Pos2::new(coords.bar_origin_x, sp_top),
        Pos2::new(scroll_rect.right(), sp_bot),
    );
    painter.rect_filled(bar_area, 0.0, theme.components.list.odd.bg);

    let current_time_s = preview.playback.current_time_s();

    // Background click to seek
    let speech_resp =
        ui.interact(bar_area, panel_id.with("speech_track_bar"), Sense::click_and_drag());
    if speech_resp.clicked() || speech_resp.dragged() {
        if let Some(pos) = speech_resp.interact_pointer_pos() {
            let t = coords.x_to_time(pos.x);
            commands.push_back(PlaybackCommand::ScrubTo(t).into());
        }
    }

    if let Some(tl) = timeline {
        for (actor_id, transcript) in tl.transcripts_iter() {
            let actor_label = tl.get_entity_label(actor_id).unwrap_or("");
            // Draw words
            for seg in &transcript.segments {
                if !seg.words.is_empty() {
                    for w in &seg.words {
                        let x0 = coords.time_to_x(w.start_s);
                        let x1 = coords.time_to_x(w.end_s);
                        if x1 < coords.bar_origin_x || x0 > scroll_rect.right() {
                            continue;
                        }
                        let pill_min_x = x0.max(coords.bar_origin_x);
                        let pill_max_x = x1.min(scroll_rect.right()).max(pill_min_x + 4.0);
                        let pill_rect = Rect::from_min_max(
                            Pos2::new(pill_min_x, sp_top + 3.0),
                            Pos2::new(pill_max_x, sp_bot - 3.0),
                        );

                        let is_active = current_time_s >= w.start_s && current_time_s < w.end_s;
                        let (bg_color, text_color, border_stroke) = if is_active {
                            (
                                theme.palette.accent.primary,
                                theme.palette.text.primary,
                                Stroke::new(1.0, theme.palette.accent.primary),
                            )
                        } else {
                            (
                                theme.palette.surface.widget,
                                theme.palette.text.secondary,
                                Stroke::new(1.0, theme.palette.border.subtle),
                            )
                        };

                        painter.rect_filled(pill_rect, 3.0, bg_color);
                        painter.rect_stroke(
                            pill_rect,
                            3.0,
                            border_stroke,
                            egui::StrokeKind::Inside,
                        );

                        if pill_rect.width() > 14.0 {
                            let galley = painter.layout_no_wrap(
                                w.word.clone(),
                                FontId::monospace(9.0),
                                text_color,
                            );
                            if galley.size().x <= pill_rect.width() - 4.0 {
                                painter.galley(
                                    Pos2::new(
                                        pill_rect.center().x - galley.size().x / 2.0,
                                        pill_rect.center().y - galley.size().y / 2.0,
                                    ),
                                    galley,
                                    text_color,
                                );
                            } else if pill_rect.width() > 18.0 {
                                painter.text(
                                    pill_rect.center(),
                                    Align2::CENTER_CENTER,
                                    "…",
                                    FontId::monospace(9.0),
                                    text_color,
                                );
                            }
                        }

                        let word_id = panel_id.with((
                            "speech_word",
                            actor_label,
                            w.word.as_str(),
                            w.start_s.to_bits(),
                        ));
                        let w_resp = ui.interact(pill_rect, word_id, Sense::click());
                        if w_resp.clicked() {
                            commands.push_back(PlaybackCommand::ScrubTo(w.start_s).into());
                        }
                        text_tooltip(
                            ui,
                            word_id.with("tip"),
                            &w_resp,
                            &format!("\"{}\"  ({:.2}s – {:.2}s)", w.word, w.start_s, w.end_s),
                        );
                    }
                } else {
                    // Line-level cue fallback
                    let x0 = coords.time_to_x(seg.start_s);
                    let x1 = coords.time_to_x(seg.end_s);
                    if x1 >= coords.bar_origin_x && x0 <= scroll_rect.right() {
                        let seg_min_x = x0.max(coords.bar_origin_x);
                        let seg_max_x = x1.min(scroll_rect.right()).max(seg_min_x + 8.0);
                        let seg_rect = Rect::from_min_max(
                            Pos2::new(seg_min_x, sp_top + 4.0),
                            Pos2::new(seg_max_x, sp_bot - 4.0),
                        );
                        painter.rect_filled(seg_rect, 2.0, theme.palette.surface.widget);
                        painter.rect_stroke(
                            seg_rect,
                            2.0,
                            Stroke::new(1.0, theme.palette.border.subtle),
                            egui::StrokeKind::Inside,
                        );
                        let galley = painter.layout_no_wrap(
                            seg.text.clone(),
                            FontId::monospace(9.0),
                            theme.palette.text.secondary,
                        );
                        if galley.size().x <= seg_rect.width() - 4.0 {
                            painter.galley(
                                Pos2::new(
                                    seg_rect.min.x + 3.0,
                                    seg_rect.center().y - galley.size().y / 2.0,
                                ),
                                galley,
                                theme.palette.text.secondary,
                            );
                        }
                    }
                }
            }

            // Draw markers (@intro, @feature, etc.)
            for (marker_name, &marker_time) in &transcript.markers {
                let mx = coords.time_to_x(marker_time);
                if mx < coords.bar_origin_x || mx > scroll_rect.right() {
                    continue;
                }
                let marker_color = theme.palette.status.warning;
                painter.line_segment(
                    [Pos2::new(mx, sp_top + 10.0), Pos2::new(mx, sp_bot)],
                    Stroke::new(1.0, marker_color.linear_multiply(0.7)),
                );
                let pts = vec![
                    Pos2::new(mx, sp_top + 1.0),
                    Pos2::new(mx + 4.0, sp_top + 5.5),
                    Pos2::new(mx, sp_top + 10.0),
                    Pos2::new(mx - 4.0, sp_top + 5.5),
                ];
                painter.add(egui::Shape::convex_polygon(pts, marker_color, Stroke::NONE));

                let marker_id = panel_id.with(("speech_marker", actor_label, marker_name.as_str()));
                let m_rect =
                    Rect::from_min_max(Pos2::new(mx - 5.0, sp_top), Pos2::new(mx + 5.0, sp_bot));
                let m_resp = ui.interact(m_rect, marker_id, Sense::click());
                if m_resp.clicked() {
                    commands.push_back(PlaybackCommand::ScrubTo(marker_time).into());
                }
                text_tooltip(
                    ui,
                    marker_id.with("tip"),
                    &m_resp,
                    &format!("@{}  ({:.2}s)", marker_name, marker_time),
                );
            }
        }
    }

    // Playhead tick
    painter.line_segment(
        [Pos2::new(playhead_x, sp_top), Pos2::new(playhead_x, sp_bot)],
        Stroke::new(1.5, theme.palette.text.primary),
    );
    // Border bottom
    painter.line_segment(
        [
            Pos2::new(scroll_rect.left(), sp_bot),
            Pos2::new(scroll_rect.right(), sp_bot),
        ],
        Stroke::new(STROKE_WIDTH, theme.palette.border.default),
    );
}
