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

#[test]
fn visual_cells_follow_authored_headers_with_incomplete_code_and_unicode() {
    use super::cells::Kind;
    let cells_for = |source: &str| {
        let analyzer = super::analyzer_for_path(Path::new("headers.amx"), source);
        crate::highlighting::SourceLayout::new(
            source,
            &egui::Style::default(),
            Some(&analyzer),
            None,
            &[],
            None,
            &[],
        )
        .cells
    };
    let text = "let note = $$
#4s
$$

# Intro
#0s
#cue:start
// #3s

#+2b
  label.opacity =";
    let cells = cells_for(text);
    let ranges: Vec<_> = cells.iter().map(|c| (c.start, c.end, c.kind)).collect();
    assert_eq!(
        ranges,
        [
            (0, 3, Kind::Setup),
            (4, 5, Kind::Scene),
            (5, 8, Kind::Absolute),
            (9, 11, Kind::Relative)
        ]
    );
    let text = "#voice@intro\r\n#cue:start\r\n// 中文🎬\r\n#step\r\n#pause\r\n";
    let cells = cells_for(text);
    assert_eq!(
        cells.iter().map(|c| (c.start, c.end)).collect::<Vec<_>>(),
        [(0, 3), (3, 4), (4, 5)]
    );

    let source = "# Scene\n#0s\n#+1s\n#+1s\n#+";
    let kinds: Vec<_> = cells_for(source).iter().map(|c| (c.start, c.kind)).collect();
    assert!(kinds.starts_with(&[
        (0, Kind::Scene),
        (1, Kind::Absolute),
        (2, Kind::Relative),
        (3, Kind::Relative)
    ]));
}

#[test]
fn cross_cell_keyboard_selection_paste_and_undo_share_one_buffer() {
    let ctx = Context::default();
    let initial = "#0s
// 中文🎬
#2s
// after
";
    let mut editor = EditorBuffer::new(Path::new("cells.amx"), initial.to_owned());
    let (id, _, _) = show(&mut editor, &ctx, vec![], true);
    set_cursor(&ctx, id, 0);
    let key = |key, modifiers| Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    };
    show(&mut editor, &ctx, vec![key(Key::ArrowDown, Modifiers::ALT)], false);
    let pos = egui::text_edit::TextEditState::load(&ctx, id)
        .unwrap()
        .cursor
        .char_range()
        .unwrap();
    assert_eq!(pos.primary.index.0, initial[..initial.find("#2s").unwrap()].chars().count());
    show(&mut editor, &ctx, vec![key(Key::ArrowUp, Modifiers::SHIFT)], false);
    let selection = egui::text_edit::TextEditState::load(&ctx, id)
        .unwrap()
        .cursor
        .char_range()
        .unwrap();
    assert!(!selection.is_empty());
    show(&mut editor, &ctx, vec![Event::Paste("// pasted\n".into())], false);
    assert_eq!(
        editor.text(),
        "#0s
// pasted
#2s
// after
"
    );
    show(&mut editor, &ctx, vec![undo_event()], false);
    assert_eq!(editor.text(), initial);
}

#[test]
fn typing_coalesces_analysis_and_explicit_queries_use_the_current_draft() {
    let mut editor = EditorBuffer::new(Path::new("typing.amx"), "#0s".into());
    for input in ["#0s\na:", "#0s\na: Rect"] {
        editor.text = input.into();
        editor.after_source_replaced();
        assert!(editor.analyzer.get().is_none());
    }
    assert_eq!(editor.analyzer().source(), "#0s\na: Rect");
    assert!(editor.analyzer.get().is_some());
    editor.finish_analysis();
    assert!(editor.analysis_due.is_none());
}

#[test]
fn semantic_cells_share_ranges_with_search_diagnostics_and_deferred_edits() {
    use crate::highlighting::{
        FindHighlight, HighlightDiagnostic, HighlightSeverity, SourceLayout,
    };
    let source = include_str!("fixtures/cell-highlighting.amx");
    let mut editor = EditorBuffer::new(Path::new("cell-highlighting.amx"), source.into());
    let start = source.find("spring(6, 9)").unwrap();
    editor.diagnostics = vec![HighlightDiagnostic {
        start,
        end: start + 6,
        severity: HighlightSeverity::Warning,
    }];
    editor.find_highlights = vec![FindHighlight {
        start,
        end: start + 6,
        current: true,
    }];
    let ctx = Context::default();
    show(&mut editor, &ctx, vec![], true);
    let cache = editor.highlight_cache.as_ref().unwrap();
    let cell_lines =
        |layout: &SourceLayout| layout.cells.iter().map(|c| c.start + 1).collect::<Vec<_>>();
    assert_eq!(cell_lines(&cache.layout), [1, 7, 11, 14, 17, 19, 23]);
    let section_at = |byte| {
        cache
            .layout
            .job
            .sections
            .iter()
            .find(|s| s.byte_range.start.0 <= byte && byte < s.byte_range.end.0)
            .unwrap()
    };
    let decorated = &section_at(start).format;
    assert!(decorated.underline.width > 0.0);
    assert_ne!(decorated.background, egui::Color32::TRANSPARENT);
    let bare = source.find("ease: spring]").unwrap() + "ease: ".len();
    assert_eq!(decorated.color, section_at(bare).format.color);
    assert_ne!(
        decorated.color,
        section_at(source.find("spring: Ellipse").unwrap()).format.color
    );
    let draft = format!("// edit 🎬\n{source}");
    let layout =
        SourceLayout::new(&draft, &egui::Style::default(), None, Some(cache), &[], None, &[]);
    assert_eq!(cell_lines(&layout), [1, 2, 8, 12, 15, 18, 20, 24]);
    let draft = format!("let hidden = r###\"\n{source}\"###");
    let layout =
        SourceLayout::new(&draft, &egui::Style::default(), None, Some(cache), &[], None, &[]);
    assert_eq!(cell_lines(&layout), [1]);
}

#[test]
fn local_timeline_headers_and_multiline_keyframe_segments_do_not_split_cells() {
    let text =
        "fn show() -> Timeline { return #{\n#1s\ntile: Rect\n} }\n# // 中文🎬\n0s\n#cue:mark\n#+2s";
    let a = super::analyzer_for_path(Path::new("nested.amx"), text);
    let layout = crate::highlighting::SourceLayout::new(
        text,
        &egui::Style::default(),
        Some(&a),
        None,
        &[],
        None,
        &[],
    );
    assert_eq!(layout.cells.iter().map(|c| c.start + 1).collect::<Vec<_>>(), [1, 5, 8]);
}

#[test]
fn soft_wrap_resize_preserves_the_visible_source_character_not_the_caret() {
    use egui::Shape;
    use egui::epaint::TextShape;
    use egui::text::CCursor;

    fn paint_at(
        editor: &mut EditorBuffer,
        ctx: &Context,
        width: f32,
        time: &mut f64,
    ) -> (TextShape, f32) {
        *time += 0.1;
        let rendered = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 420.0))),
                time: Some(*time),
                ..Default::default()
            },
            |ui| {
                editor.show(ui);
            },
        );
        rendered
            .shapes
            .into_iter()
            .find_map(|clipped| match clipped.shape {
                Shape::Text(text) if text.galley.job.text == editor.text() => {
                    Some((text, clipped.clip_rect.top()))
                },
                _ => None,
            })
            .expect("painted source buffer")
    }

    let ctx = Context::default();
    let mut time = 0.0;
    let source: String = (0..160)
        .map(|i| {
            format!(
                "// {i:03} {}\n",
                if i == 80 {
                    "测试 😀 wrapped words ".repeat(24)
                } else {
                    "short".to_owned()
                },
            )
        })
        .collect();
    let mut editor = EditorBuffer::new(Path::new("reflow.amx"), source.clone());
    let _ = show(&mut editor, &ctx, vec![], true);
    set_cursor(&ctx, editor.text_edit_id, 2);
    let (narrow, viewport) = paint_at(&mut editor, &ctx, 470.0, &mut time);
    // Scroll enough to start at a soft-wrapped continuation row, not a new
    // logical line. Use the real galley for the fixture's target.
    let long_line_start =
        source.split_inclusive('\n').take(80).map(|s| s.chars().count()).sum::<usize>();
    let continuation = long_line_start + 110;
    let target = narrow.galley.pos_from_cursor(CCursor::new(continuation)).top();
    let scroll_delta = target - (viewport - narrow.pos.y) + 2.0;
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(470.0, 420.0))),
            time: Some(time + 0.1),
            events: vec![
                Event::PointerMoved(Pos2::new(160.0, 100.0)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0.0, -scroll_delta),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: Modifiers::NONE,
                },
            ],
            ..Default::default()
        },
        |ui| {
            editor.show(ui);
        },
    )
    .drop_without_applying_deltas();
    time += 0.1;
    let mut narrow = None;
    for _ in 0..30 {
        narrow = Some(paint_at(&mut editor, &ctx, 470.0, &mut time));
    }
    let (narrow, _) = narrow.unwrap();
    let anchor = editor.viewport_position.as_ref().unwrap().anchor;
    let anchor_char = anchor.char_index();
    assert!(
        anchor_char > long_line_start && anchor_char < long_line_start + 400,
        "test must start inside the continuation, got {anchor_char} (start {long_line_start})"
    );
    assert!(narrow.galley.pos_from_cursor(CCursor::new(anchor_char)).top() > 500.0);

    for width in [350.0, 610.0, 420.0, 470.0] {
        // Check the first presented frame, not just the settled state: a
        // splitter drag must not flash a different region of the document.
        let (text, viewport) = paint_at(&mut editor, &ctx, width, &mut time);
        let expected = text.pos.y + anchor.top(&text.galley);
        assert!(
            (viewport - expected).abs() < 2.0,
            "width {width}: viewport top {viewport}, expected {expected}, char {anchor_char}"
        );
        assert_eq!(editor.viewport_position.as_ref().unwrap().anchor.char_index(), anchor_char);
    }
    let state = egui::text_edit::TextEditState::load(&ctx, editor.text_edit_id).unwrap();
    assert_eq!(state.cursor.char_range().unwrap().primary.index.0, 2);
    assert_eq!(editor.text(), source);

    editor.scroll_to_line(120);
    for _ in 0..12 {
        let _ = paint_at(&mut editor, &ctx, 350.0, &mut time);
    }
    let explicit = editor.viewport_position.as_ref().unwrap().anchor.char_index();
    assert!(explicit > long_line_start + 400, "explicit jump must beat resize anchoring");

    editor.set_document(Path::new("new.amx"), source);
    assert!(editor.viewport_position.is_none());
    let _ = paint_at(&mut editor, &ctx, 610.0, &mut time);
    assert!(editor.viewport_position.as_ref().unwrap().scroll_y.abs() < 1.0);
}
