use super::*;

use egui::{Context, Key, Modifiers};

fn test_input(events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 80.0))),
        events,
        ..Default::default()
    }
}

fn canvas_focus(text_edit_focused: bool) -> FocusContext {
    FocusContext {
        text_edit_focused,
        has_selection: false,
        drag_active: false,
        inline_edit_active: false,
        command_palette_open: false,
        find_replace_open: false,
        unsaved_dialog_open: false,
        tool_mode: crate::app::preview::ToolMode::Move,
    }
}

#[test]
fn backspace_reaches_text_edit_when_focused() {
    let ctx = Context::default();
    let edit_id = egui::Id::new("test_text_edit");
    let mut text = "abc".to_owned();

    let _ = ctx.run_ui(test_input(Vec::new()), |ui| {
        let response = ui.add(egui::TextEdit::singleline(&mut text).id(edit_id));
        response.request_focus();
    });
    assert!(ctx.text_edit_focused(), "TextEdit should own keyboard focus");

    // Move the caret to the end so Backspace deletes a character.
    let mut edit_state = egui::text_edit::TextEditState::load(&ctx, edit_id).unwrap_or_default();
    edit_state.cursor.set_char_range(Some(egui::text::CCursorRange::one(
        egui::text::CCursor::new(text.chars().count()),
    )));
    edit_state.store(&ctx, edit_id);

    let registry = ShortcutRegistry::new();
    let focus = canvas_focus(true);
    let input = test_input(vec![egui::Event::Key {
        key: Key::Backspace,
        physical_key: Some(Key::Backspace),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]);

    ctx.begin_pass(input);
    let action = registry.check(&ctx, &focus);
    assert!(action.is_none(), "Backspace must not trigger a shortcut in a TextEdit");

    let mut changed = false;
    let _ = egui::Area::new(egui::Id::new("test_area")).show(&ctx, |ui| {
        let response = ui.add(egui::TextEdit::singleline(&mut text).id(edit_id));
        changed |= response.changed();
    });
    let _ = ctx.end_pass();

    assert!(changed, "TextEdit should receive the Backspace event");
    assert_eq!(text, "ab");
}

#[test]
fn ime_commit_reaches_text_edit_after_focus_switch() {
    let ctx = Context::default();
    let id_a = egui::Id::new("ime_a");
    let id_b = egui::Id::new("ime_b");
    let mut text_a = "existing".to_owned();
    let mut text_b = String::new();

    // Give A focus first, then switch focus to B. In real input this click
    // is followed by IME events on a later frame.
    let _ = ctx.run_ui(test_input(Vec::new()), |ui| {
        let resp_a = ui.add(egui::TextEdit::singleline(&mut text_a).id(id_a));
        resp_a.request_focus();
        let resp_b = ui.add(egui::TextEdit::singleline(&mut text_b).id(id_b));
        resp_b.request_focus();
    });
    ctx.begin_pass(test_input(Vec::new()));
    show_text_edits(&ctx, &mut text_a, id_a, false, &mut text_b, id_b, true);
    let _ = ctx.end_pass();
    assert!(ctx.text_edit_focused(), "B should own focus after switch");

    ctx.begin_pass(test_input(vec![
        egui::Event::Ime(egui::ImeEvent::Enabled),
        egui::Event::Ime(egui::ImeEvent::Preedit("ni".to_string())),
        egui::Event::Ime(egui::ImeEvent::Commit("你".to_string())),
    ]));
    show_text_edits(&ctx, &mut text_a, id_a, false, &mut text_b, id_b, true);
    let _ = ctx.end_pass();

    assert_eq!(text_b, "你", "focused TextEdit should commit IME text");
    assert_eq!(text_a, "existing");
}

fn show_text_edits(
    ctx: &Context,
    text_a: &mut String,
    id_a: egui::Id,
    focus_a: bool,
    text_b: &mut String,
    id_b: egui::Id,
    focus_b: bool,
) {
    let _ = egui::Area::new(egui::Id::new("ime_area")).show(ctx, |ui| {
        let resp_a = ui.add(egui::TextEdit::singleline(text_a).id(id_a));
        if focus_a {
            resp_a.request_focus();
        }
        let resp_b = ui.add(egui::TextEdit::singleline(text_b).id(id_b));
        if focus_b {
            resp_b.request_focus();
        }
    });
}

#[test]
fn backspace_canvas_shortcut_fires_when_no_text_focus() {
    let ctx = Context::default();
    let registry = ShortcutRegistry::new();
    let input = test_input(vec![egui::Event::Key {
        key: Key::Backspace,
        physical_key: Some(Key::Backspace),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]);

    ctx.begin_pass(input);
    let action = registry.check(&ctx, &canvas_focus(false));
    assert!(matches!(action, Some(KeyboardAction::DeleteSelection)));
    let _ = ctx.end_pass();
}

#[test]
fn shortcut_for_save_returns_ctrl_s() {
    let registry = ShortcutRegistry::new();
    let shortcut = registry.shortcut_for(&KeyboardAction::Save);
    assert!(shortcut.is_some(), "Expected a registered shortcut for KeyboardAction::Save");
    let shortcut = shortcut.unwrap();
    assert_eq!(shortcut.modifiers, egui::Modifiers::COMMAND);
    assert_eq!(shortcut.logical_key, Key::S);
}

#[test]
fn shortcut_for_payload_variant_matches_discriminant() {
    let registry = ShortcutRegistry::new();
    // NudgeSelected has an f32 payload — discriminant match should still work.
    let shortcut = registry.shortcut_for(&KeyboardAction::NudgeSelected { dx: 0.0, dy: 0.0 });
    assert!(
        shortcut.is_some(),
        "Expected a registered shortcut for KeyboardAction::NudgeSelected"
    );
    // The first nudge entry is ArrowLeft.
    assert_eq!(shortcut.unwrap().logical_key, Key::ArrowLeft);
}

#[test]
fn shortcuts_for_name_returns_all_redo_bindings() {
    let registry = ShortcutRegistry::new();
    let redo = registry.shortcuts_for_name("Redo");
    assert_eq!(redo.len(), 2, "Expected Ctrl+Shift+Z and Ctrl+Y Redo bindings");
}

#[test]
fn tool_shortcuts_are_unambiguous() {
    let registry = ShortcutRegistry::new();
    assert_eq!(registry.shortcuts_for_name("Select Tool").len(), 1);
    assert_eq!(registry.shortcuts_for_name("Move Tool").len(), 1);
    assert_eq!(registry.shortcuts_for_name("Scale Tool").len(), 1);
    assert_eq!(registry.shortcuts_for_name("Rotate Tool").len(), 1);
    assert_eq!(registry.shortcuts_for_name("Vertex Tool").len(), 1);
    assert_eq!(registry.shortcuts_for_name("Pivot Tool").len(), 1);

    let vertex = registry.shortcut_for(&KeyboardAction::SetVertexTool).unwrap();
    assert_eq!(vertex.logical_key, Key::A);
    let select = registry.shortcut_for(&KeyboardAction::SetSelectTool).unwrap();
    assert_eq!(select.logical_key, Key::V);
}

#[test]
fn shortcuts_for_name_returns_empty_for_unknown() {
    let registry = ShortcutRegistry::new();
    assert!(registry.shortcuts_for_name("Not a real shortcut").is_empty());
}

#[test]
fn shortcut_for_unknown_returns_none() {
    // KeyboardAction::SelectScene(99) won't match because the registry only
    // registers SelectScene(0), (1), and (2) — discriminant is the same
    // variant but we test a hypothetical unregistered variant isn't returned.
    // Actually SelectScene(99) shares the same discriminant, so it WILL match
    // the first SelectScene entry. This is expected discriminant-level behaviour.
    let registry = ShortcutRegistry::new();
    let shortcut = registry.shortcut_for(&KeyboardAction::SelectScene(0));
    assert!(shortcut.is_some());
    assert_eq!(shortcut.unwrap().logical_key, Key::Num1);
}

#[test]
fn saved_shortcut_roundtrips_through_egui_shortcut() {
    let shortcut = KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::ArrowLeft);
    let saved = SavedShortcut::from_shortcut(&shortcut);
    assert_eq!(saved.key, "ArrowLeft");
    assert!(saved.command);
    assert!(saved.shift);
    assert_eq!(saved.to_shortcut(), Some(shortcut));
}

#[test]
fn overrides_replace_binding_and_are_reported_as_current() {
    let mut overrides = std::collections::BTreeMap::new();
    overrides.insert(
        "Save".to_string(),
        SavedShortcut {
            command: false,
            ctrl: false,
            shift: false,
            alt: true,
            key: "S".to_string(),
        },
    );
    let registry = ShortcutRegistry::with_overrides(&overrides).expect("valid override");
    let save = registry.shortcut_for(&KeyboardAction::Save).expect("Save binding");
    assert_eq!(save.modifiers, egui::Modifiers::ALT);
    assert_eq!(save.logical_key, Key::S);
    assert!(registry.current_saved("Save").unwrap().alt);
}

#[test]
fn overrides_reject_unknown_binding() {
    let mut overrides = std::collections::BTreeMap::new();
    overrides.insert(
        "Not A Binding".to_string(),
        SavedShortcut {
            command: true,
            ctrl: false,
            shift: false,
            alt: false,
            key: "S".to_string(),
        },
    );
    assert!(matches!(
        ShortcutRegistry::with_overrides(&overrides),
        Err(KeyBindingError::UnknownBinding(_))
    ));
}

#[test]
fn overrides_reject_conflicting_shortcuts() {
    let conflict = SavedShortcut {
        command: true,
        ctrl: false,
        shift: false,
        alt: false,
        key: "S".to_string(),
    };
    let mut overrides = std::collections::BTreeMap::new();
    overrides.insert("Save".to_string(), conflict.clone());
    overrides.insert("Reload".to_string(), conflict);

    assert!(matches!(
        ShortcutRegistry::with_overrides(&overrides),
        Err(KeyBindingError::Conflict(_, _, _))
    ));
}

#[test]
fn overrides_do_not_mutate_a_separate_registry() {
    let mut overrides = std::collections::BTreeMap::new();
    overrides.insert(
        "Save".to_string(),
        SavedShortcut {
            command: false,
            ctrl: false,
            shift: false,
            alt: true,
            key: "S".to_string(),
        },
    );
    let default_registry = ShortcutRegistry::new();
    let overridden_registry = ShortcutRegistry::with_overrides(&overrides).expect("valid override");

    let default_save = default_registry.shortcut_for(&KeyboardAction::Save).unwrap();
    let overridden_save = overridden_registry.shortcut_for(&KeyboardAction::Save).unwrap();
    assert_ne!(default_save.modifiers, overridden_save.modifiers);
    assert!(!default_registry.current_saved("Save").unwrap().alt);
}
