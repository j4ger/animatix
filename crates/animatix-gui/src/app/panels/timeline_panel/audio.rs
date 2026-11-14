//! Read-only source-audio waveforms. Clicking seeks; no clip mutation is exposed.

use animatix::timeline::AudioSegment;
use egui::{Align2, FontId, Pos2, Rect, Sense, Stroke};

use super::TimelineCoords;
use crate::app::audio::waveform::{WaveformCache, WaveformState};

/// Draw an audio row in global timeline coordinates. Asset paths must already
/// be document-relative resolved. The revision invalidates failed/stale assets.
pub(crate) fn render_audio_track(
    ui: &mut egui::Ui,
    rect: Rect,
    coords: &TimelineCoords,
    segments: &[AudioSegment],
    revision: u64,
) -> Option<f64> {
    let theme = eparts::theme(ui);
    let id = ui.scope_id().with("audio_waveforms");
    let cache = ui.data_mut(|data| data.get_temp_mut_or_default::<WaveformCache>(id).clone());
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, theme.palette.surface.base);
    painter.text(
        Pos2::new(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Audio",
        FontId::proportional(12.0),
        theme.palette.text.muted,
    );
    let bar = Rect::from_min_max(Pos2::new(coords.bar_origin_x, rect.top()), rect.max);
    let visible_end = coords.scroll_s + coords.visible_s;
    // Separate simultaneous assets vertically rather than suggest a mixed PCM
    // waveform. This row is a reference view, not a DAW clip editor.
    let visible: Vec<_> = segments
        .iter()
        .filter_map(|segment| {
            if segment.start_time_s >= visible_end {
                return None;
            }
            let state = cache.get(&segment.source, revision, ui.ctx());
            let duration = match &state {
                WaveformState::Ready(waveform) => segment.duration_s.unwrap_or(waveform.duration_s),
                _ => segment.duration_s.unwrap_or((visible_end - segment.start_time_s).max(0.0)),
            };
            let start = segment.start_time_s.max(coords.scroll_s);
            let end = (segment.start_time_s + duration).min(visible_end);
            (end > start && start.is_finite() && end.is_finite())
                .then_some((segment, state, duration, start, end))
        })
        .collect();
    let rows = visible.len().clamp(1, 4);
    for (index, (segment, state, duration, start, end)) in visible.iter().take(4).enumerate() {
        let height = (bar.height() - 4.0) / rows as f32;
        let clip = Rect::from_min_max(
            Pos2::new(coords.time_to_x(*start), bar.top() + 2.0 + index as f32 * height),
            Pos2::new(coords.time_to_x(*end), bar.top() + 2.0 + (index + 1) as f32 * height - 1.0),
        );
        let painter = ui.painter().with_clip_rect(bar);
        painter.rect_filled(clip, 2.0, theme.palette.accent.cyan.gamma_multiply(0.12));
        let status = match state {
            WaveformState::Ready(waveform) => {
                let columns = clip.width().ceil().max(1.0) as usize;
                let peaks = waveform.columns(
                    start - segment.start_time_s,
                    end - segment.start_time_s,
                    columns,
                );
                let middle = clip.center().y;
                let amplitude = (clip.height() * 0.44).max(1.0);
                for (column, peak) in peaks.iter().enumerate() {
                    let x = clip.left() + column as f32 * clip.width() / peaks.len() as f32;
                    painter.line_segment(
                        [
                            Pos2::new(x, middle - peak.max * amplitude),
                            Pos2::new(x, middle - peak.min * amplitude),
                        ],
                        Stroke::new(1.0, theme.palette.accent.cyan),
                    );
                }
                "Source amplitude (before volume automation)".to_owned()
            },
            WaveformState::Loading => {
                paint_status(&painter, clip, "Loading waveform…", theme.palette.text.muted);
                "Loading waveform in background".to_owned()
            },
            WaveformState::Failed(error) => {
                paint_status(&painter, clip, "Audio unavailable", theme.palette.status.warning);
                format!("Waveform unavailable: {error}. Rebuild to retry.")
            },
        };
        let duration_label =
            if segment.duration_s.is_some() || matches!(state, WaveformState::Ready(_)) {
                format!("{duration:.3}s duration")
            } else {
                "duration unavailable until decoded".to_owned()
            };
        ui.interact(clip, id.with(index), Sense::hover()).on_hover_text(format!(
            "{}\nStarts at {:.3}s · {}\n{}\nClick to seek; edit audio in source.",
            segment.source, segment.start_time_s, duration_label, status,
        ));
    }
    if visible.len() > 4 {
        painter.text(
            Pos2::new(rect.left() + 8.0, rect.bottom() - 2.0),
            Align2::LEFT_BOTTOM,
            format!("+{} clips", visible.len() - 4),
            FontId::proportional(9.0),
            theme.palette.text.muted,
        );
    }
    let response = ui.interact(bar, id.with("seek"), Sense::click_and_drag());
    if (response.clicked() || response.dragged())
        && let Some(position) = response.interact_pointer_pos()
    {
        return Some(coords.x_to_time(position.x));
    }
    None
}

fn paint_status(painter: &egui::Painter, rect: Rect, text: &str, color: egui::Color32) {
    if rect.width() > 110.0 {
        painter.text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(10.0), color);
    }
}
