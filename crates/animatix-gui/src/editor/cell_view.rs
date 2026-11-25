use super::cells::{Cell, Kind};
use crate::app::document::source_timing::SourceTiming;
use crate::highlighting::SourceLayout;
use egui::text::Galley;
use egui::text_edit::TextEditOutput;
use egui::{Align2, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2};
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct CellPlayback<'a> {
    pub timing: Option<&'a SourceTiming>,
    pub line: Option<usize>,
    pub seek: Option<f64>,
}

pub(super) fn gutter_width(lines: usize) -> f32 {
    (lines.max(1).ilog10() + 1).max(2) as f32 * 7.0 + 42.0
}

pub(super) fn galley(ui: &egui::Ui, layout: &SourceLayout, height: f32, width: f32) -> Arc<Galley> {
    let mut job = layout.job.clone();
    // Give every token on a header the same height so the hash and its label
    // share a baseline. Layout sections are source-ordered token runs.
    let mut headers = layout
        .cells
        .iter()
        .map(|cell| {
            let tail = &job.text[cell.byte..];
            cell.byte..cell.byte + tail.find('\n').unwrap_or(tail.len())
        })
        .peekable();
    for section in &mut job.sections {
        let start = section.byte_range.start.0;
        while headers.next_if(|header| header.end <= start).is_some() {}
        let is_header =
            headers.peek().is_some_and(|header| header.start < section.byte_range.end.0);
        section.format.line_height = Some(if is_header { height + 8.0 } else { height });
    }
    job.wrap.max_width = width;
    ui.fonts_mut(|f| f.layout_job(job))
}

/// The TextEdit's galley owns the layout, including soft wraps, pointer hit testing,
/// selection, and IME. Paint cards behind its text, using its final row coordinates.
pub(super) fn paint(
    ui: &egui::Ui,
    output: &TextEditOutput,
    cells: &[Cell],
    left: f32,
    background: egui::layers::ShapeIdx,
    playback: &mut CellPlayback<'_>,
    current: bool,
) {
    let palette = eparts::theme(ui).palette;
    let origin = output.galley_pos;
    let clip = ui.clip_rect();
    let mut rows = Vec::new();
    let mut start = 0;
    for (i, row) in output.galley.rows.iter().enumerate() {
        if row.ends_with_newline || i + 1 == output.galley.rows.len() {
            rows.push(start..i + 1);
            start = i + 1;
        }
    }
    let bounds = |line: usize| {
        let range = rows.get(line)?;
        let first = output.galley.rows[range.start].rect();
        let last = output.galley.rows[range.end - 1].rect();
        Some(first.union(last).translate(origin.to_vec2()))
    };
    let caret = output
        .cursor_range
        .filter(|_| output.response.has_focus())
        .map(|range| output.galley.pos_from_cursor(range.primary).center().y + origin.y);
    let rail = origin.x - 25.0;
    let right = output.response.rect.right().min(clip.right()) - 3.0;
    let mut shapes = Vec::new();
    for cell in cells {
        let (Some(first), Some(last)) = (bounds(cell.start), bounds(cell.end - 1)) else {
            continue;
        };
        let card = Rect::from_min_max(
            Pos2::new(rail, first.top() + 2.0),
            Pos2::new(right, last.bottom() - 1.0),
        );
        if !card.is_positive() || !card.intersects(clip) {
            continue;
        }
        let playing = current && playback.line == Some(cell.start);
        let focused = caret.is_some_and(|y| (first.top()..last.bottom()).contains(&y));
        let accent = if playing {
            palette.accent.primary
        } else {
            palette.text.secondary
        };
        let fill = if playing {
            palette.surface.panel.lerp_to_gamma(palette.accent.primary, 0.075)
        } else if cell.kind == Kind::Scene {
            palette.surface.widget
        } else {
            palette.surface.surface
        };
        let edge = if playing {
            fill.lerp_to_gamma(accent, 0.55)
        } else if focused {
            palette.border.strong
        } else {
            palette.border.default
        };
        shapes.push(Shape::rect_filled(card, 5.0, fill));
        shapes.push(Shape::rect_stroke(card, 5.0, Stroke::new(1.0, edge), StrokeKind::Inside));
        if playing {
            shapes.push(Shape::line_segment(
                [
                    card.left_top() + Vec2::new(1.0, 4.0),
                    card.left_bottom() + Vec2::new(1.0, -4.0),
                ],
                Stroke::new(2.0, accent),
            ));
        }
        let row = output.galley.rows[rows[cell.start].start].rect().translate(origin.to_vec2());
        let center = Pos2::new(rail + 12.5, row.center().y + 0.5);
        let target = playback.timing.filter(|_| current).and_then(|t| t.at_header(cell.start));
        let mut ink = if cell.kind == Kind::Scene {
            palette.accent.cyan
        } else {
            accent
        };
        if let Some(time) = target {
            let button = ui.interact(
                Rect::from_center_size(center, Vec2::new(21.0, row.height())),
                output.response.id.with(("seek-cell", cell.start)),
                Sense::click(),
            );
            let label = format!(
                "{} · {}",
                crate::app::i18n::text(ui, "gui-shortcut-sheet-scrub-to-time"),
                crate::app::i18n::render(
                    ui,
                    "gui-timeline-marker-time",
                    &[("time", format!("{time:.2}"))]
                )
            );
            button.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
            if button.hovered() || button.has_focus() {
                ui.painter().circle_filled(center, 9.0, palette.surface.hover);
                ink = palette.accent.primary;
            }
            if button
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(label)
                .clicked()
            {
                playback.seek = Some(time);
            }
        }
        if cell.kind == Kind::Scene {
            ui.painter().rect_stroke(
                Rect::from_center_size(center, Vec2::new(11.0, 8.0)),
                2.0,
                Stroke::new(1.4, ink),
                StrokeKind::Inside,
            );
            ui.painter().vline(
                center.x - 2.0,
                center.y - 3.0..=center.y + 3.0,
                Stroke::new(1.0, ink),
            );
        } else if cell.kind == Kind::Time {
            let r = if playing { 4.0 } else { 3.4 };
            let points = vec![
                center + Vec2::new(0.0, -r),
                center + Vec2::new(r, 0.0),
                center + Vec2::new(0.0, r),
                center + Vec2::new(-r, 0.0),
            ];
            ui.painter().add(Shape::convex_polygon(
                points,
                if playing { ink } else { fill },
                Stroke::new(1.2, ink),
            ));
        }
    }
    ui.painter().set(background, Shape::Vec(shapes));
    for (line, range) in rows.iter().enumerate() {
        let row = output.galley.rows[range.start].rect().translate(origin.to_vec2());
        if row.bottom() < clip.top() {
            continue;
        }
        if row.top() > clip.bottom() {
            break;
        }
        let color = if caret
            .is_some_and(|y| bounds(line).is_some_and(|r| r.top() <= y && y < r.bottom()))
        {
            palette.text.primary
        } else if current && playback.line == Some(line) {
            palette.accent.primary
        } else {
            palette.text.muted
        };
        ui.painter().text(
            Pos2::new((rail - 8.0).max(left), row.center().y),
            Align2::RIGHT_CENTER,
            (line + 1).to_string(),
            FontId::monospace(10.0),
            color,
        );
    }
}

pub(super) fn navigate_headers(
    ui: &egui::Ui,
    state: &mut egui::text_edit::TextEditState,
    source: &str,
    cells: &[Cell],
) -> Option<usize> {
    use egui::text::{CCursor, CCursorRange};
    use egui::{Key, Modifiers};
    let range = state.cursor.char_range()?;
    let forward = ui.input_mut(|input| {
        if input.consume_key(Modifiers::ALT, Key::ArrowDown) {
            Some(true)
        } else if input.consume_key(Modifiers::ALT, Key::ArrowUp) {
            Some(false)
        } else {
            None
        }
    })?;
    let byte = super::char_to_byte_offset(source, range.primary.index.0);
    let cell = if forward {
        cells.iter().find(|c| c.byte > byte)
    } else {
        cells.iter().rev().find(|c| c.byte < byte)
    }?;
    state
        .cursor
        .set_char_range(Some(CCursorRange::one(CCursor::new(source[..cell.byte].chars().count()))));
    Some(cell.start)
}
