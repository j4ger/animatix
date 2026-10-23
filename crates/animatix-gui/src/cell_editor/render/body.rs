use super::*;

use super::frames::*;

pub(super) fn render_cell_body(
    ui: &mut egui::Ui,
    index: usize,
    body: &mut String,
    body_start_line: usize,
    state: &mut CellEditorState,
    source_changed: &mut bool,
    theme: eparts::Theme,
    style: &egui::Style,
    cell_diags: &[animatix_analyzer::Diagnostic],
) {
    apply_pending_cursor(ui, index, state);

    let layouter_style = style.clone();
    let cell_diags_ref = cell_diags.to_vec();
    let cell_semantic: Vec<_> = state
        .semantic_highlights
        .iter()
        .filter(|sh| sh.cell_index == index)
        .cloned()
        .collect();
    let cell_find: Vec<(usize, usize, bool)> = state
        .find_matches
        .iter()
        .filter(|m| m.cell_index == index)
        .map(|m| (m.rel_start_byte, m.rel_end_byte, m.is_current))
        .collect();
    let find_match_bg = theme.palette.accent.selection;
    let find_current_bg = theme.palette.accent.subtle;

    let body_text = body.to_string();
    let cached_job = state
        .cached_highlight_jobs
        .get(&index)
        .filter(|(cached_body, _)| cached_body == &body_text)
        .map(|(_, job)| job.clone());
    let base_job = if let Some(job) = cached_job {
        job
    } else {
        let new_job =
            highlight_source(&body_text, &layouter_style, &cell_diags_ref, None, &cell_semantic);
        state.cached_highlight_jobs.insert(index, (body_text.clone(), new_job.clone()));
        new_job
    };

    let mut layouter = move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
        let buf_text = buf.as_str();
        let mut job = if buf_text == base_job.text.as_str() {
            base_job.clone()
        } else {
            highlight_source(buf_text, &layouter_style, &cell_diags_ref, None, &cell_semantic)
        };
        apply_find_highlights(&mut job, &cell_find, find_match_bg, find_current_bg);
        job.wrap.max_width = wrap_width;
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    };

    let font_id = TextRole::Mono.font_id();
    let line_height = ui.fonts_mut(|f| f.row_height(&font_id));
    let char_width = ui.fonts_mut(|f| f.glyph_width(&font_id, '0'));

    let sp = spatial(ui);
    let line_count = body.split('\n').count().max(1);
    let max_line = body_start_line + line_count;
    let digits = format!("{max_line}").len().max(2);
    let gutter_w = digits as f32 * char_width + sp.editor.gutter_padding;

    let text_edit_id = ui.id().with(("cell_body", index));

    let edit_response = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = sp.base.space_4;
            // Reserve space for the gutter on the left
            let _ = ui.allocate_exact_size(Vec2::new(gutter_w, 0.0), egui::Sense::hover());
            ui.add(
                egui::TextEdit::multiline(body)
                    .id(text_edit_id)
                    .code_editor()
                    .lock_focus(true)
                    .frame(Frame::NONE)
                    .desired_width(f32::INFINITY)
                    .layouter(&mut layouter),
            )
        })
        .inner;

    if edit_response.changed() {
        *source_changed = true;
    }
    if state.pending_cursor_cell == Some(index) {
        edit_response.request_focus();
        state.pending_cursor_cell = None;
    }
    track_focus(index, &edit_response, state);
    track_cursor(&edit_response, state);

    // Gutter line numbers and subtle divider
    let text_rect = edit_response.rect;
    let gutter_right = text_rect.left() - sp.editor.gutter_divider_inset;
    let gutter_divider_x = gutter_right + sp.editor.gutter_divider_inset * 0.5;

    ui.painter().line_segment(
        [
            Pos2::new(gutter_divider_x, text_rect.top()),
            Pos2::new(gutter_divider_x, text_rect.bottom()),
        ],
        Stroke::new(STROKE_WIDTH, theme.palette.border.subtle),
    );

    for i in 0..line_count {
        let line_num = body_start_line + i + 1;
        let y = text_rect.top() + (i as f32) * line_height;
        let has_error = cell_diags.iter().any(|d| d.line == i);
        let num_color = if has_error {
            theme.palette.status.error
        } else {
            theme.palette.text.muted
        };
        ui.painter().text(
            Pos2::new(gutter_right, y),
            egui::Align2::RIGHT_TOP,
            format!("{line_num}"),
            font_id.clone(),
            num_color,
        );
    }

    let cell_underlines: Vec<CellDiagnostic> =
        state.diagnostics.iter().filter(|d| d.cell_index == index).cloned().collect();
    if !cell_underlines.is_empty() {
        draw_wavy_underlines(ui, &cell_underlines, text_rect, theme);
    }
}

// ── Cell card components ─────────────────────────────────────────────────

pub(super) fn render_code_cell(
    ui: &mut egui::Ui,
    index: usize,
    cell: &mut Cell,
    style: &egui::Style,
    highlighted: bool,
    state: &mut CellEditorState,
    source_changed: &mut bool,
    start_line: usize,
    end_line: usize,
) -> egui::Response {
    render_cell_shell(
        ui,
        index,
        cell,
        style,
        highlighted,
        state,
        source_changed,
        start_line,
        start_line,
        end_line,
        "code block",
        |ui, _cell, _state, _source_changed, theme, _sp| {
            ui.label(
                RichText::new(egui_phosphor::regular::CODE)
                    .size(TextRole::BodyS.size())
                    .color(theme.palette.text.muted),
            );
            ui.label(
                RichText::new(format!("Code {index}"))
                    .size(TextRole::BodyS.size())
                    .color(theme.palette.text.secondary),
            );
        },
    )
}

// ── Keyframe cell ────────────────────────────────────────────────────────

pub(super) fn render_keyframe_cell(
    ui: &mut egui::Ui,
    index: usize,
    cell: &mut Cell,
    style: &egui::Style,
    highlighted: bool,
    state: &mut CellEditorState,
    on_scrub_to_time: &mut dyn FnMut(f64),
    source_changed: &mut bool,
    start_line: usize,
    end_line: usize,
) -> egui::Response {
    let time_s = cell.time_s().unwrap_or(0.0);
    let header_lines = match cell {
        Cell::Keyframe {
            attached_comment, ..
        } => attached_comment.as_ref().map(|c| c.lines().count()).unwrap_or(0) + 1,
        _ => 0,
    };
    let body_start_line = start_line + header_lines;

    render_cell_shell(
        ui,
        index,
        cell,
        style,
        highlighted,
        state,
        source_changed,
        body_start_line,
        start_line,
        end_line,
        "keyframe",
        |ui, cell, state, source_changed, theme, _sp| {
            // Keyframe type icon
            ui.label(
                RichText::new(egui_phosphor::regular::FILM_STRIP)
                    .size(TextRole::BodyS.size())
                    .color(theme.palette.accent.primary),
            );

            // Editable timestamp
            render_timestamp_editor(ui, cell, source_changed, state, index);

            // Play
            if header_btn(ui, egui_phosphor::regular::PLAY, "Play from this keyframe") {
                on_scrub_to_time(time_s);
            }
        },
    )
}

// ── Timestamp inline editor ──────────────────────────────────────────────

pub(super) fn render_timestamp_editor(
    ui: &mut egui::Ui,
    cell: &mut Cell,
    source_changed: &mut bool,
    state: &mut CellEditorState,
    cell_index: usize,
) {
    let (raw_ts, is_rel) = match cell {
        Cell::Keyframe {
            timestamp,
            is_relative,
            ..
        } => (timestamp.clone(), *is_relative),
        _ => return,
    };

    let theme = eparts::theme(ui);
    let is_editing = state.editing_timestamp_cell == Some(cell_index);

    if is_editing {
        let mut edited = raw_ts.clone();
        let ts_response = ui.add(
            egui::TextEdit::singleline(&mut edited)
                .font(TextRole::Mono.font_id())
                .desired_width(editor::TIMESTAMP_EDIT_WIDTH)
                .frame(Frame::NONE)
                .text_color(theme.palette.accent.primary),
        );

        if ts_response.changed() {
            let new_is_relative = if edited.starts_with('+') {
                true
            } else {
                is_rel
            };

            if let Cell::Keyframe {
                timestamp,
                is_relative,
                ..
            } = cell
            {
                *timestamp = edited.trim().to_string();
                *is_relative = new_is_relative;
                *source_changed = true;
            }
        }

        if ts_response.lost_focus() {
            state.editing_timestamp_cell = None;
        }

        track_focus(cell_index, &ts_response, state);
    } else {
        let display = cell.display_timestamp().unwrap_or_else(|| raw_ts.clone());
        let label_response = ui.add(
            egui::Label::new(
                RichText::new(display)
                    .font(TextRole::Mono.font_id())
                    .color(theme.palette.accent.primary),
            )
            .sense(egui::Sense::click()),
        );
        if label_response.clicked() {
            state.editing_timestamp_cell = Some(cell_index);
        }
        if label_response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
    }
}

// ── Focus tracking ───────────────────────────────────────────────────────

pub(super) fn track_focus(index: usize, response: &egui::Response, state: &mut CellEditorState) {
    if response.gained_focus() {
        state.focused_cell = Some(index);
        state.highlighted_cell = None;
    }
}

/// Record the live caret char offset inside the focused cell body so callers
/// (completion in particular) can operate at the caret rather than assuming
/// end-of-document.
pub(super) fn track_cursor(response: &egui::Response, state: &mut CellEditorState) {
    if !response.has_focus() {
        return;
    }
    state.focused_rect = Some(response.rect);
    if let Some(te_state) = egui::text_edit::TextEditState::load(&response.ctx, response.id) {
        if let Some(range) = te_state.cursor.char_range() {
            state.focused_cursor_char = Some(range.primary.index);
        }
    }
}

/// Draw wavy squiggly underlines for diagnostics below the text.
///
/// Uses the egui painter to draw a series of small zigzag segments under each
/// diagnostic span.  Error diagnostics get red waves, warnings get amber waves.
pub(super) fn draw_wavy_underlines(
    ui: &egui::Ui,
    diags: &[CellDiagnostic],
    text_rect: egui::Rect,
    theme: eparts::Theme,
) {
    let sp = spatial(ui);
    let painter = ui.painter();
    let font_id = TextRole::Mono.font_id();

    // Get monospace metrics for position calculation
    let char_width = ui.fonts_mut(|f| f.glyph_width(&font_id, 'm'));
    let line_height = ui.fonts_mut(|f| f.row_height(&font_id));

    for d in diags {
        let color = match d.severity {
            animatix_syntax::diagnostics::DiagnosticSeverity::Error => theme.palette.status.error,
            animatix_syntax::diagnostics::DiagnosticSeverity::Warning
            | animatix_syntax::diagnostics::DiagnosticSeverity::Info
            | animatix_syntax::diagnostics::DiagnosticSeverity::Hint => {
                theme.palette.status.warning
            },
        };

        // Y position: baseline below the diagnostic line
        let y = text_rect.top() + (d.rel_line as f32 + 1.0) * line_height - sp.base.space_1;

        // X positions
        let x_start = text_rect.left() + d.rel_col as f32 * char_width;
        let x_end = text_rect.left() + d.rel_end_col as f32 * char_width;

        if x_end <= x_start
            || y < text_rect.top() - line_height
            || y > text_rect.bottom() + line_height
        {
            continue;
        }

        // Draw wavy line: series of small zigzag segments
        let width = x_end - x_start;
        let wave_len = sp.base.space_2; // pixels per half-wave
        let wave_amp = 1.5; // amplitude in pixels
        let num_segments = (width / wave_len).ceil() as usize;
        let actual_seg_width = width / num_segments.max(1) as f32;

        let mut points = Vec::with_capacity(num_segments + 1);
        for i in 0..=num_segments {
            let x = x_start + i as f32 * actual_seg_width;
            let y_off = if i % 2 == 0 { 0.0 } else { wave_amp };
            points.push(egui::Pos2::new(x, y + y_off));
        }

        painter.add(egui::Shape::line(points, egui::Stroke::new(1.5, color)));
    }
}

// ── Divider between cells ────────────────────────────────────────────────

pub(super) fn divider(ui: &mut egui::Ui, after_index: usize, state: &mut CellEditorState) {
    let theme = eparts::theme(ui);
    let sp = spatial(ui);
    let available = ui.available_width();
    let height = sp.editor.divider_height;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(available, height), egui::Sense::click());

    let center = rect.center();
    let y = center.y;
    let left = rect.left() + sp.editor.divider_line_inset;
    let right = rect.right() - sp.editor.divider_line_inset;

    // Smooth hover transition for the divider line
    let t = anim::animate_toward(
        ui.ctx(),
        ui.id().with(("divider_line", after_index)),
        if response.hovered() { 1.0 } else { 0.0 },
        motion::HOVER,
    );

    // ── Divider line (always visible, brightens on hover) ──
    if left < right {
        let line_color = if t > 0.0 {
            theme.palette.border.strong
        } else {
            theme.palette.border.default
        };
        let line_a = egui::lerp(120.0..=220.0, t) as u8;
        ui.painter().line_segment(
            [egui::pos2(left, y), egui::pos2(right, y)],
            Stroke::new(
                egui::lerp(1.0..=1.5, t),
                Color32::from_rgba_premultiplied(
                    line_color.r(),
                    line_color.g(),
                    line_color.b(),
                    line_a,
                ),
            ),
        );
    }

    // ── Center "add" button — ALWAYS visible ──
    let btn_size = Vec2::splat(sp.editor.divider_btn_size);
    let btn_rect = egui::Rect::from_center_size(center, btn_size);

    // Animate the button independently so it has smooth transitions even though
    // it's always on screen.
    let btn_t = anim::animate_toward(
        ui.ctx(),
        ui.id().with(("divider_btn", after_index)),
        if response.hovered() || response.is_pointer_button_down_on() {
            1.0
        } else {
            0.0
        },
        motion::HOVER,
    );
    let pressed = response.is_pointer_button_down_on();

    // Background (always visible — no alpha tricks)
    let bg_idle = theme.palette.surface.widget;
    let bg_hover = theme.palette.surface.hover;
    let bg = if pressed {
        theme.palette.accent.primary_active
    } else {
        lerp_color(bg_idle, bg_hover, btn_t)
    };

    // Border (subtle idle, stronger hover)
    let border = if pressed {
        theme.palette.accent.primary
    } else if btn_t > 0.0 {
        theme.palette.border.strong
    } else {
        theme.palette.border.default
    };

    // Icon color
    let icon = if pressed {
        theme.palette.text.on_accent
    } else if btn_t > 0.0 {
        theme.palette.text.primary
    } else {
        theme.palette.text.secondary
    };

    ui.painter().rect_filled(btn_rect, RADIUS_M, bg);
    ui.painter().rect_stroke(
        btn_rect,
        RADIUS_M,
        Stroke::new(STROKE_WIDTH, border),
        egui::StrokeKind::Inside,
    );

    ui.painter().text(
        center,
        egui::Align2::CENTER_CENTER,
        egui_phosphor::regular::PLUS,
        TextRole::Body.font_id(),
        icon,
    );

    if response.clicked() {
        state.pending_insert_after = Some(after_index);
    }

    use crate::app::components::context_menu::{MenuEntry, render_menu};

    response.context_menu(|ui| {
        let entries = vec![
            MenuEntry::item_with_icon(egui_phosphor::regular::FILM_STRIP, "Insert keyframe"),
            MenuEntry::item_with_icon(egui_phosphor::regular::CODE, "Insert code block"),
        ];
        if let Some(idx) = render_menu(ui, &entries) {
            match idx {
                0 => state.pending_insert_after = Some(after_index),
                1 => state.pending_insert_code_after = Some(after_index),
                _ => {},
            }
            ui.close();
        }
    });
}
