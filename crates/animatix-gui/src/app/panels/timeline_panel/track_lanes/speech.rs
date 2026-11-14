//! Transcript review. Identical Audio/Captions transcripts share a row.
use super::*;
#[derive(Debug, PartialEq)]
struct Cue {
    text: String,
    start: f64,
    end: f64,
}
struct TranscriptRow {
    label: String,
    cues: Vec<Cue>,
    markers: Vec<(String, f64)>,
}

fn rows(timeline: Option<&Timeline>) -> Vec<TranscriptRow> {
    let mut result: Vec<TranscriptRow> = Vec::new();
    if let Some(timeline) = timeline {
        for (id, transcript) in timeline.transcripts_iter() {
            let label = timeline.get_entity_label(id).unwrap_or("Transcript");
            let cues = transcript
                .segments
                .iter()
                .flat_map(|segment| {
                    if segment.words.is_empty() {
                        vec![Cue {
                            text: segment.text.clone(),
                            start: segment.start_s,
                            end: segment.end_s,
                        }]
                    } else {
                        segment
                            .words
                            .iter()
                            .map(|w| Cue {
                                text: w.word.clone(),
                                start: w.start_s,
                                end: w.end_s,
                            })
                            .collect()
                    }
                })
                .collect::<Vec<_>>();
            let mut markers: Vec<_> =
                transcript.markers.iter().map(|(name, time)| (name.clone(), *time)).collect();
            markers.sort_by(|a, b| a.0.cmp(&b.0));
            if let Some(existing) = result
                .iter_mut()
                .find(|existing| existing.cues == cues && existing.markers == markers)
            {
                existing.label.push_str(" / ");
                existing.label.push_str(label);
            } else {
                result.push(TranscriptRow {
                    label: label.to_owned(),
                    cues,
                    markers,
                });
            }
        }
    }
    result.sort_by(|a, b| a.label.cmp(&b.label));
    result
}

pub(crate) fn speech_row_count(timeline: Option<&Timeline>) -> usize {
    rows(timeline).len()
}

pub(crate) fn render_speech_track(
    ui: &mut egui::Ui,
    timeline: Option<&Timeline>,
    top: f32,
    bottom: f32,
    scroll_rect: Rect,
    coords: &TimelineCoords,
    preview: &mut PreviewPaneState,
    commands: &mut ActionQueue,
    scene_offset_s: f64,
    panel_id: egui::Id,
    theme: eparts::Theme,
    _sp: &Spatial,
) {
    let rows = rows(timeline);
    let row_height = (bottom - top) / rows.len().max(1) as f32;
    for (index, row) in rows.iter().enumerate() {
        let label = &row.label;
        let y = top + index as f32 * row_height;
        let bar = Rect::from_min_max(
            Pos2::new(coords.bar_origin_x, y),
            Pos2::new(scroll_rect.right(), y + row_height),
        );
        let painter = ui.painter();
        painter.rect_filled(bar, 0.0, theme.palette.surface.widget);
        painter.text(
            Pos2::new(scroll_rect.left() + 8.0, y + row_height * 0.5),
            Align2::LEFT_CENTER,
            "Transcript",
            TextRole::BodyS.font_id(),
            theme.palette.text.secondary,
        );
        let header = Rect::from_min_max(
            Pos2::new(scroll_rect.left(), y),
            Pos2::new(coords.bar_origin_x, y + row_height),
        );
        ui.interact(header,panel_id.with(("transcript_label", index)),Sense::hover())
            .on_hover_text(format!("{label}\nActive scene transcript · original timing + scene offset ({scene_offset_s:.3}s). Audio clip delay is not inferred."));
        for (cue_index, item) in row.cues.iter().enumerate() {
            cue(
                ui,
                &item.text,
                item.start,
                item.end,
                scene_offset_s,
                bar,
                coords,
                preview,
                commands,
                panel_id.with(("cue", index, cue_index)),
                theme,
            );
        }
        for (name, time) in &row.markers {
            let time = time + scene_offset_s;
            if time < coords.scroll_s || time > coords.scroll_s + coords.visible_s {
                continue;
            }
            let x = coords.time_to_x(time);
            ui.painter().line_segment(
                [Pos2::new(x, y), Pos2::new(x, y + row_height)],
                Stroke::new(1.0, theme.palette.status.warning),
            );
            let response = ui.interact(
                Rect::from_min_max(Pos2::new(x - 4.0, y), Pos2::new(x + 4.0, y + 8.0)),
                panel_id.with(("speech_marker", index, name)),
                Sense::click(),
            );
            if response.clicked() {
                commands.push_back(Command::ScrubTo(time).into());
            }
            response.on_hover_text(format!("@{name} · {time:.3}s global"));
        }
    }
}

fn cue(
    ui: &mut egui::Ui,
    text: &str,
    start: f64,
    end: f64,
    offset: f64,
    bar: Rect,
    coords: &TimelineCoords,
    preview: &mut PreviewPaneState,
    commands: &mut ActionQueue,
    id: egui::Id,
    theme: eparts::Theme,
) {
    let start = start + offset;
    let end = end + offset;
    if end <= coords.scroll_s || start >= coords.scroll_s + coords.visible_s || end <= start {
        return;
    }
    let rect = Rect::from_min_max(
        Pos2::new(coords.time_to_x(start).max(bar.left()), bar.top() + 4.0),
        Pos2::new(coords.time_to_x(end).min(bar.right()), bar.bottom() - 4.0),
    );
    let painter = ui.painter().with_clip_rect(bar.intersect(ui.clip_rect()));
    let current = preview.playback.current_time_s();
    painter.rect_filled(
        rect,
        3.0,
        if current >= start && current < end {
            theme.palette.accent.primary
        } else {
            theme.palette.surface.panel
        },
    );
    let galley = painter.layout_no_wrap(
        text.to_owned(),
        FontId::proportional(10.0),
        theme.palette.text.primary,
    );
    if galley.size().x + 6.0 <= rect.width() {
        painter.galley(rect.center() - galley.size() * 0.5, galley, theme.palette.text.primary);
    }
    let response = ui.interact(rect, id, Sense::click());
    if response.clicked() {
        commands.push_back(Command::ScrubTo(start).into());
    }
    if response.double_clicked() {
        set_loop(preview, start, end);
        commands.push_back(Command::ScrubTo(start).into());
    }
    response.context_menu(|ui| {
        if ui.button("Loop this cue (preview only)").clicked() {
            set_loop(preview, start, end);
            commands.push_back(Command::ScrubTo(start).into());
            ui.close();
        }
    });
    response.on_hover_text(format!(
        "{text}\n{start:.3}s – {end:.3}s global\nClick to seek · double-click to loop"
    ));
}

fn set_loop(preview: &mut PreviewPaneState, start: f64, end: f64) {
    let duration = preview.playback.duration_s;
    let start = start.clamp(0.0, duration);
    let end = end.clamp(0.0, duration);
    if end > start {
        preview.playback.loop_start_s = Some(start);
        preview.playback.loop_end_s = Some(end);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_loop_is_bounded_by_global_duration() {
        let mut preview = PreviewPaneState::new(
            8.0,
            animatix::timeline::SceneDimensions {
                width: 1920,
                height: 1080,
            },
        );
        preview.playback.duration_s = 8.0;
        set_loop(&mut preview, 6.0, 12.0);
        assert_eq!(preview.playback.loop_start_s, Some(6.0));
        assert_eq!(preview.playback.loop_end_s, Some(8.0));
        // A cue outside the composition must not replace the valid loop.
        set_loop(&mut preview, 9.0, 12.0);
        assert_eq!(preview.playback.loop_start_s, Some(6.0));
        assert_eq!(preview.playback.loop_end_s, Some(8.0));
    }
}
