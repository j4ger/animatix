use std::path::{Path, PathBuf};

use animatix_analyzer::{CompletionItem, CompletionKind};
use egui::{Context, Event, Id, Key, Modifiers, Pos2, Rect, Vec2};

use super::EditorBuffer;
use super::completion::CompletionAnchor;
use crate::app::document::DocumentSession;
use crate::app::stores::DocumentStore;

fn input(events: Vec<Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 420.0))),
        events,
        ..Default::default()
    }
}

fn show(
    editor: &mut EditorBuffer,
    ctx: &Context,
    events: Vec<Event>,
    request_focus: bool,
) -> (Id, Rect, bool) {
    let mut result = None;
    ctx.run_ui(input(events), |ui| {
        let response = editor.show(ui);
        if request_focus {
            response.request_focus();
        }
        result = Some((response.id, response.rect, response.changed()));
    })
    .drop_without_applying_deltas();
    result.expect("editor was shown")
}

fn set_cursor(ctx: &Context, id: Id, char_offset: usize) {
    let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(char_offset))));
    state.store(ctx, id);
}

fn type_text(editor: &mut EditorBuffer, ctx: &Context, id: Id, text: &str) {
    let char_offset = editor.text().chars().count();
    set_cursor(ctx, id, char_offset);
    show(editor, ctx, vec![Event::Text(text.to_owned())], true);
}

fn undo_event() -> Event {
    Event::Key {
        key: Key::Z,
        physical_key: Some(Key::Z),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }
}

fn completion_item(label: &str, insert_text: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_owned(),
        kind: CompletionKind::Property,
        detail: None,
        detail_message: None,
        documentation: None,
        insert_text: Some(insert_text.to_owned()),
    }
}

#[test]
fn commit_only_ime_survives_full_editor_sync() {
    let ctx = Context::default();
    let path = PathBuf::from("ime.amx");
    let source = "box: Rect\n";
    let document =
        DocumentSession::from_source(path.clone(), source.to_owned()).expect("valid document");
    let mut store = DocumentStore::new(document, EditorBuffer::new(&path, source.to_owned()));
    let _ = show(&mut store.source.editor, &ctx, Vec::new(), true);
    assert!(ctx.text_edit_focused(), "the source TextEdit should own focus");

    // Some IME integrations provide the final Commit without a preceding
    // preedit event. Syncing the live editor into the source store must keep it.
    show(
        &mut store.source.editor,
        &ctx,
        vec![Event::Ime(egui::ImeEvent::Commit("你".to_owned()))],
        false,
    );
    store.sync_from_editor();

    assert!(
        store.source.text().contains('你'),
        "committed IME text must reach canonical source"
    );
    assert!(store.source.text().starts_with("box: Rect"));
}

#[test]
fn diagnostic_navigation_uses_egui_character_offsets_after_astral_unicode() {
    let ctx = Context::default();
    let source = "let face = \"😀\";\nbox: Rect\n";
    let mut editor = EditorBuffer::new(Path::new("unicode.amx"), source.to_owned());
    let line = source.lines().next().expect("first line");
    let after_emoji = line.find('😀').expect("emoji") + '😀'.len_utf8();
    let character_column = line[..after_emoji].chars().count();

    editor.focus_diagnostic(0, character_column);
    let _ = show(&mut editor, &ctx, Vec::new(), false);
    let (id, _, _) = show(&mut editor, &ctx, Vec::new(), false);

    let state = egui::text_edit::TextEditState::load(&ctx, id).expect("TextEdit state");
    let cursor = state.cursor.char_range().expect("navigation placed a cursor").primary.index.0;
    assert_eq!(
        cursor,
        line.chars().count(),
        "navigation should preserve the old token-end behavior"
    );
    assert_ne!(cursor, line.len(), "a UTF-8 byte offset must not be used as a cursor index");
    assert_eq!(editor.cursor_line, Some(0));
}

#[test]
fn authoritative_document_replacement_does_not_reuse_old_native_undo_history() {
    let ctx = Context::default();
    let mut editor = EditorBuffer::new(Path::new("first.amx"), "first\n".into());
    let (first_id, _, _) = show(&mut editor, &ctx, Vec::new(), true);
    type_text(&mut editor, &ctx, first_id, "!");
    assert!(editor.text().contains('!'));

    // Loading another document must not carry the first document's caret or
    // egui TextEdit undo stack into the new source.
    editor.set_document(Path::new("second.amx"), "second\n".into());
    assert_eq!(editor.analyzer().path(), Some(Path::new("second.amx")));
    let (second_id, _, _) = show(&mut editor, &ctx, Vec::new(), true);
    assert_ne!(first_id, second_id, "document changes need a distinct TextEdit state key");
    show(&mut editor, &ctx, vec![undo_event()], true);
    assert_eq!(editor.text(), "second\n");

    // A same-path reload is authoritative too; it must discard local native
    // undo history instead of making Ctrl+Z resurrect the pre-reload bytes.
    let (second_id, _, _) = show(&mut editor, &ctx, Vec::new(), true);
    type_text(&mut editor, &ctx, second_id, "?");
    assert!(editor.text().contains('?'));
    editor.set_document(Path::new("second.amx"), "external version\n".into());
    let (reloaded_id, _, _) = show(&mut editor, &ctx, Vec::new(), true);
    assert_ne!(second_id, reloaded_id, "same-path reloads need a new TextEdit generation");
    show(&mut editor, &ctx, vec![undo_event()], true);
    assert_eq!(editor.text(), "external version\n");
}

#[test]
fn completion_enter_is_consumed_before_the_text_edit_can_insert_a_newline() {
    let ctx = Context::default();
    let source = "box.po";
    let mut editor = EditorBuffer::new(Path::new("completion.amx"), source.to_owned());
    editor
        .completion
        .show(vec![completion_item("position", "position")], "po".into());
    editor.completion_anchor = Some(CompletionAnchor {
        caret: source.chars().count(),
        word_start: source.chars().count() - 2,
    });
    let (id, _, _) = show(&mut editor, &ctx, Vec::new(), true);
    set_cursor(&ctx, id, source.chars().count());

    show(
        &mut editor,
        &ctx,
        vec![Event::Key {
            key: Key::Enter,
            physical_key: Some(Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        true,
    );

    assert_eq!(editor.text(), "box.position");
    assert!(!editor.completion.is_visible());
}

#[test]
fn clicking_a_completion_item_inserts_it_at_the_saved_caret() {
    let ctx = Context::default();
    let source = "box.po";
    let mut editor = EditorBuffer::new(Path::new("completion-click.amx"), source.to_owned());
    editor
        .completion
        .show(vec![completion_item("position", "position")], "po".into());
    editor.completion_anchor = Some(CompletionAnchor {
        caret: source.chars().count(),
        word_start: source.chars().count() - 2,
    });
    let (_, rect, _) = show(&mut editor, &ctx, Vec::new(), true);

    let pos = Pos2::new(
        rect.left() + 12.0,
        rect.bottom() + 2.0 + 2.0 + crate::app::design_tokens::spatial::ROW_M * 0.5,
    );
    show(
        &mut editor,
        &ctx,
        vec![
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
        ],
        true,
    );

    assert_eq!(editor.text(), "box.position");
    assert!(!editor.completion.is_visible());
}

#[test]
fn navigating_to_the_end_of_a_large_source_keeps_the_target_line_active() {
    let ctx = Context::default();
    let source = (0..160)
        .map(|line| {
            let comment = if line == 0 {
                "long line ".repeat(40)
            } else {
                String::new()
            };
            format!("let item_{line} = {line}; // {comment}\n")
        })
        .collect::<String>();
    let target_line = 159;
    let mut editor = EditorBuffer::new(Path::new("large.amx"), source.clone());

    // Position the user's caret, then verify a timeline-only scroll/highlight
    // leaves both the caret and TextEdit focus alone.
    let (mut id, _, _) = show(&mut editor, &ctx, Vec::new(), true);
    set_cursor(&ctx, id, 2);
    let _ = show(&mut editor, &ctx, Vec::new(), false);
    show(
        &mut editor,
        &ctx,
        vec![
            Event::PointerMoved(Pos2::new(200.0, 100.0)),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(-240.0, 0.0),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: Modifiers::NONE,
            },
        ],
        false,
    );
    for _ in 0..30 {
        show(&mut editor, &ctx, Vec::new(), false);
    }
    let scroll_id = editor.current_scroll_area_id.expect("editor scroll area id");
    let offset_before = egui::scroll_area::State::load(&ctx, scroll_id)
        .expect("horizontal scroll state")
        .clamped_offset()
        .x;
    assert!(offset_before > 0.0, "the long source line must have scrolled horizontally");
    editor.scroll_to_line(target_line);
    editor.set_highlighted_line(Some(target_line));
    for _ in 0..3 {
        id = show(&mut editor, &ctx, Vec::new(), false).0;
    }
    let before_navigation = egui::text_edit::TextEditState::load(&ctx, id)
        .expect("focused TextEdit state")
        .cursor
        .char_range()
        .expect("existing caret")
        .primary
        .index
        .0;
    assert_eq!(before_navigation, 2);
    assert!(ctx.text_edit_focused(), "timeline sync should not steal editor focus");
    let scroll_id = editor.current_scroll_area_id.expect("editor scroll area id");
    let scroll = egui::scroll_area::State::load(&ctx, scroll_id).expect("scroll state");
    assert!((scroll.clamped_offset().x - offset_before).abs() < 1.0);
    assert!(
        scroll.clamped_offset().y > 0.0,
        "scroll_to_line should move the viewport to the distant target"
    );

    // A diagnostic navigation deliberately moves the caret to the token end.
    editor.focus_diagnostic(target_line, 0);
    let mut id = Id::NULL;
    for _ in 0..3 {
        id = show(&mut editor, &ctx, Vec::new(), false).0;
    }

    let state = egui::text_edit::TextEditState::load(&ctx, id).expect("TextEdit state");
    let cursor = state.cursor.char_range().expect("navigation placed a cursor").primary.index.0;
    let target_offset = source
        .split_inclusive('\n')
        .take(target_line)
        .map(|line| line.chars().count())
        .sum::<usize>();
    assert_eq!(cursor, target_offset + "let".chars().count());
    assert_eq!(editor.cursor_line, Some(target_line));
    assert_eq!(editor.text(), source, "scrolling and navigation must not edit the source");
}

#[test]
fn source_navigation_render_preserves_original_bytes() {
    let speech =
        include_str!("../../../animatix/tests/fixtures/amx/animation/41_speech_sync_captions.amx");
    for source in [
        speech,
        "// cue\r\n#voice[\"DSL\"]:\r\n    box.opacity = 1",
        "#0s { box: Rect }\n",
    ] {
        let mut editor = EditorBuffer::new(Path::new("navigation.amx"), source.to_owned());
        let line = source.lines().position(|line| line.contains("#voice[\"DSL\"]")).unwrap_or(0);
        editor.focus_diagnostic(line, 0);
        let ctx = Context::default();
        for _ in 0..3 {
            let (_, _, changed) = show(&mut editor, &ctx, Vec::new(), false);
            assert!(!changed, "navigation and rendering must not signal a source edit");
            assert_eq!(editor.text(), source, "navigation must preserve every source byte");
        }
    }

    let source = "#0s\nbox: Rect\n#5s\n";
    let mut editor = EditorBuffer::new(Path::new("end-marker.amx"), source.to_owned());
    let ctx = Context::default();
    editor.focus_diagnostic(2, 0);
    assert!(!show(&mut editor, &ctx, Vec::new(), false).2);
    editor.focus_diagnostic(0, 0);
    assert!(!show(&mut editor, &ctx, Vec::new(), false).2);
    assert_eq!(editor.text(), source, "navigation must preserve an empty end marker");
}

#[test]
fn format_source_reformats_unformatted_source() {
    let source = "box: Rect,size:(100,100)\n#0s\nbox.pos=(0,0)\n";
    let mut editor = EditorBuffer::new(Path::new("format.amx"), source.to_owned());

    assert!(editor.format_source(), "valid unformatted source should be reformatted");
    assert!(editor.text().contains("size: (100, 100)"));
}
