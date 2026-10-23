use super::*;
use crate::cell_editor::Cell;
use egui::{Context, Event, Modifiers, Pos2, Rect, Vec2};

fn input() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(480.0, 320.0))),
        ..Default::default()
    }
}

fn render_cells(ctx: &Context, cells: &mut [Cell], state: &mut CellEditorState) {
    let _ = ctx.run_ui(input(), |ui| {
        render_cell_editor(ui, cells, state, &mut |_| {}, &mut |_| {});
    });
}

fn click_cells(ctx: &Context, cells: &mut [Cell], state: &mut CellEditorState, pos: Pos2) {
    let mut raw = input();
    raw.events.extend([
        Event::PointerMoved(pos),
        Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]);
    let _ = ctx.run_ui(raw, |ui| {
        render_cell_editor(ui, cells, state, &mut |_| {}, &mut |_| {});
    });
}

fn send_ime(ctx: &Context, cells: &mut [Cell], state: &mut CellEditorState, commit: &str) {
    let mut raw = input();
    raw.events.extend([
        Event::Ime(egui::ImeEvent::Enabled),
        Event::Ime(egui::ImeEvent::Preedit("ni".to_string())),
        Event::Ime(egui::ImeEvent::Commit(commit.to_string())),
    ]);
    let _ = ctx.run_ui(raw, |ui| {
        render_cell_editor(ui, cells, state, &mut |_| {}, &mut |_| {});
    });
}

#[test]
fn ime_commit_reaches_focused_existing_cell() {
    let ctx = Context::default();
    let mut cells = vec![Cell::Code {
        body: "existing".to_string(),
        expanded: true,
    }];
    let mut state = CellEditorState::default();
    render_cells(&ctx, &mut cells, &mut state);

    // Click into the body editor of the existing code cell.
    click_cells(&ctx, &mut cells, &mut state, Pos2::new(120.0, 34.0));
    assert!(ctx.text_edit_focused(), "clicking the existing cell should focus its TextEdit");

    send_ime(&ctx, &mut cells, &mut state, "你");

    assert_eq!(cells[0].body(), "existing你");
}

#[test]
fn commit_only_ime_reaches_existing_cell_without_stale_cursor() {
    let ctx = Context::default();
    let mut cells = vec![Cell::Code {
        body: "existing".to_string(),
        expanded: true,
    }];
    let mut state = CellEditorState::default();
    render_cells(&ctx, &mut cells, &mut state);

    click_cells(&ctx, &mut cells, &mut state, Pos2::new(200.0, 34.0));
    assert!(ctx.text_edit_focused(), "clicking the existing cell should focus its TextEdit");

    // Some native IME integrations deliver only the final Commit without
    // Preedit/Enabled. The focused TextEdit must still insert it.
    let mut raw = input();
    raw.events.push(Event::Ime(egui::ImeEvent::Commit("你".to_string())));
    let _ = ctx.run_ui(raw, |ui| {
        render_cell_editor(ui, &mut cells, &mut state, &mut |_| {}, &mut |_| {});
    });

    assert!(cells[0].body().contains('你'));
}

#[test]
fn ime_commit_reaches_cell_after_focus_switch() {
    let ctx = Context::default();
    let mut cells = vec![
        Cell::Code {
            body: "first".to_string(),
            expanded: true,
        },
        Cell::Code {
            body: "second".to_string(),
            expanded: true,
        },
    ];
    let mut state = CellEditorState::default();
    render_cells(&ctx, &mut cells, &mut state);

    // Focus the first cell, then switch to the second existing cell.
    click_cells(&ctx, &mut cells, &mut state, Pos2::new(120.0, 34.0));
    assert_eq!(state.focused_cell, Some(0));
    click_cells(&ctx, &mut cells, &mut state, Pos2::new(120.0, 190.0));
    assert_eq!(state.focused_cell, Some(1), "second cell should receive focus");
    assert!(ctx.text_edit_focused(), "second cell TextEdit should be focused");

    send_ime(&ctx, &mut cells, &mut state, "你");

    assert_eq!(cells[0].body(), "first");
    assert_eq!(cells[1].body(), "second你");
}

// ── Find-match overlay ───────────────────────────────────────────────

const MATCH_BG: Color32 = Color32::from_rgba_premultiplied(1, 2, 3, 60);
const CURRENT_BG: Color32 = Color32::from_rgba_premultiplied(4, 5, 6, 200);

/// A single-section job covering the whole text.
fn plain_job(text: &str) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: TextRole::Mono.font_id(),
            color: Color32::WHITE,
            ..Default::default()
        },
    );
    job
}

/// Background color of the section covering `needle`.
fn bg_for(job: &egui::text::LayoutJob, needle: &str) -> Option<Color32> {
    job.sections
        .iter()
        .find(|s| &job.text[s.byte_range.clone()] == needle)
        .map(|s| s.format.background)
}

/// Background color of the section containing `byte_index`.
fn bg_at(job: &egui::text::LayoutJob, byte_index: usize) -> Option<Color32> {
    job.sections
        .iter()
        .find(|s| s.byte_range.contains(&byte_index))
        .map(|s| s.format.background)
}

#[test]
fn find_overlay_splits_base_section_at_match_boundaries() {
    let mut job = plain_job("cat dog cat");
    apply_find_highlights(&mut job, &[(0, 3, false), (8, 11, false)], MATCH_BG, CURRENT_BG);

    assert_eq!(job.text, "cat dog cat", "overlay must not alter the text");
    assert_eq!(bg_for(&job, "cat"), Some(MATCH_BG));
    assert_eq!(bg_for(&job, " dog "), Some(Color32::TRANSPARENT));
    // The trailing "cat" is its own section with the match background.
    assert_eq!(bg_at(&job, 8), Some(MATCH_BG));
}

#[test]
fn find_overlay_distinguishes_current_match() {
    let mut job = plain_job("cat cat cat");
    apply_find_highlights(
        &mut job,
        &[(0, 3, false), (4, 7, true), (8, 11, false)],
        MATCH_BG,
        CURRENT_BG,
    );

    let bg = |start: usize| {
        job.sections
            .iter()
            .find(|s| s.byte_range == (start..start + 3))
            .map(|s| s.format.background)
    };
    assert_eq!(bg(0), Some(MATCH_BG));
    assert_eq!(bg(4), Some(CURRENT_BG), "the current match uses the stronger accent");
    assert_eq!(bg(8), Some(MATCH_BG));
}

#[test]
fn find_overlay_preserves_syntax_format() {
    let mut job = plain_job("box.pos");
    job.sections[0].format.color = Color32::from_rgb(10, 20, 30);
    apply_find_highlights(&mut job, &[(4, 7, false)], MATCH_BG, CURRENT_BG);

    for section in &job.sections {
        assert_eq!(
            section.format.color,
            Color32::from_rgb(10, 20, 30),
            "split sections must keep the original text color"
        );
    }
}

#[test]
fn find_overlay_is_a_noop_without_ranges() {
    let mut job = plain_job("box");
    let before = job.sections.len();
    apply_find_highlights(&mut job, &[], MATCH_BG, CURRENT_BG);
    assert_eq!(job.sections.len(), before);
    assert_eq!(job.sections[0].format.background, Color32::TRANSPARENT);
}

#[test]
fn find_overlay_clamps_out_of_range_offsets() {
    let mut job = plain_job("box");
    // A stale/oversized range must not panic.
    apply_find_highlights(&mut job, &[(1, 999, false)], MATCH_BG, CURRENT_BG);
    assert_eq!(job.text, "box");
    assert_eq!(bg_at(&job, 0), Some(Color32::TRANSPARENT));
    assert_eq!(bg_at(&job, 1), Some(MATCH_BG));
    assert_eq!(bg_at(&job, 2), Some(MATCH_BG));
}
